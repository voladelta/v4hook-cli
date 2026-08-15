use std::{
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::util::normalize_hex;

fn result_from_payload(payload: &Value, url: &str, method: &str) -> Result<Value> {
    if let Some(error) = payload.get("error").filter(|error| !error.is_null()) {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown RPC error");
        let message = message.replace(url, "[REDACTED RPC URL]");
        bail!("RPC {method} failed: {message}");
    }
    payload
        .get("result")
        .cloned()
        .with_context(|| format!("RPC {method} returned no result"))
}

fn rpc_value(url: &str, method: &str, params: &[Value]) -> Result<Value> {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(45))
        .build()
        .context("build RPC client")?;
    let response = client
        .post(url)
        .json(&json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}))
        .send()
        .map_err(|_| anyhow::anyhow!("RPC {method} request failed"))?;
    if !response.status().is_success() {
        bail!("RPC {method} returned HTTP {}", response.status());
    }
    let payload: Value = response
        .json()
        .map_err(|_| anyhow::anyhow!("decode RPC {method} response"))?;
    result_from_payload(&payload, url, method)
}

pub fn rpc<T: DeserializeOwned>(url: &str, method: &str, params: &[Value]) -> Result<T> {
    serde_json::from_value(rpc_value(url, method, params)?)
        .with_context(|| format!("decode RPC {method} result"))
}

pub fn reset_fork(local_url: &str, target_url: &str, block_number: u64) -> Result<()> {
    rpc_value(
        local_url,
        "anvil_reset",
        &[json!({
            "forking": {
                "jsonRpcUrl": target_url,
                "blockNumber": block_number,
            }
        })],
    )?;
    Ok(())
}

fn parse_quantity(value: &str, label: &str) -> Result<u64> {
    let body = value
        .strip_prefix("0x")
        .context("RPC quantity is missing 0x prefix")?;
    u64::from_str_radix(body, 16).with_context(|| format!("parse {label}"))
}

pub fn chain_id(url: &str) -> Result<u64> {
    parse_quantity(&rpc::<String>(url, "eth_chainId", &[])?, "chain ID")
}

pub fn block_number(url: &str) -> Result<u64> {
    parse_quantity(&rpc::<String>(url, "eth_blockNumber", &[])?, "block number")
}

pub fn block_hash(url: &str, number: u64) -> Result<String> {
    let block: Option<Value> = rpc(
        url,
        "eth_getBlockByNumber",
        &[json!(format!("0x{number:x}")), json!(false)],
    )?;
    let hash = block
        .and_then(|value| value.get("hash").and_then(Value::as_str).map(str::to_owned))
        .with_context(|| format!("RPC has no block {number}"))?;
    normalize_hex(&hash, "block hash")
}

pub fn code_at(url: &str, address: &str) -> Result<String> {
    normalize_hex(
        &rpc::<String>(url, "eth_getCode", &[json!(address), json!("latest")])?,
        "contract code",
    )
}

pub fn wait_for_rpc(url: &str, timeout: Duration) -> Result<()> {
    let started = Instant::now();
    let mut last_error = None;
    while started.elapsed() < timeout {
        match chain_id(url) {
            Ok(_) => return Ok(()),
            Err(error) => last_error = Some(error),
        }
        thread::sleep(Duration::from_millis(100));
    }
    if let Some(error) = last_error {
        bail!(
            "Anvil RPC did not become ready within {}ms: {error}",
            timeout.as_millis()
        );
    }
    bail!(
        "Anvil RPC did not become ready within {}ms",
        timeout.as_millis()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_errors_do_not_echo_rpc_credentials() {
        let credential_url = "http://127.0.0.1:1/private-provider-token";
        let error = chain_id(credential_url).unwrap_err();
        assert!(!format!("{error:#}").contains(credential_url));
    }

    #[test]
    fn accepts_null_error_and_redacts_rpc_errors() {
        assert_eq!(
            result_from_payload(
                &json!({"result": null, "error": null}),
                "https://provider.example/secret",
                "anvil_reset",
            )
            .unwrap(),
            Value::Null
        );
        let credential_url = "https://provider.example/secret";
        let error = result_from_payload(
            &json!({"error": {"message": format!("failed at {credential_url}")}}),
            credential_url,
            "eth_chainId",
        )
        .unwrap_err()
        .to_string();
        assert!(!error.contains(credential_url));
    }
}
