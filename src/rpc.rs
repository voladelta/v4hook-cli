use std::{
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::util::normalize_hex;

#[derive(Debug, Deserialize)]
struct RpcError {
    message: String,
}

#[derive(Debug, Deserialize)]
struct RpcResponse<T> {
    result: Option<T>,
    error: Option<RpcError>,
}

pub fn rpc<T: DeserializeOwned>(url: &str, method: &str, params: &[Value]) -> Result<T> {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(45))
        .build()
        .context("build RPC client")?;
    let response = client
        .post(url)
        .json(&json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}))
        .send()
        .with_context(|| format!("RPC {method} request failed"))?;
    if !response.status().is_success() {
        bail!("RPC {method} returned HTTP {}", response.status());
    }
    let payload: RpcResponse<T> = response
        .json()
        .with_context(|| format!("decode RPC {method} response"))?;
    if let Some(error) = payload.error {
        bail!("RPC {method} failed: {}", error.message);
    }
    payload
        .result
        .with_context(|| format!("RPC {method} returned no result"))
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
