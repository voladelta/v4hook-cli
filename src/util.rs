use std::{
    fs,
    io::{self, IsTerminal},
    path::{Path, PathBuf},
};

use alloy_primitives::{Address, U256, keccak256};
use anyhow::{Context, Result, bail};
use chrono::{SecondsFormat, Utc};
use regex::Regex;
use serde::Serialize;
use sha2::{Digest, Sha256};

pub fn stable_json<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).context("serialize canonical JSON")
}

pub fn sha256_bytes(value: impl AsRef<[u8]>) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(value.as_ref())))
}

pub fn sha256_file(path: impl AsRef<Path>) -> Result<String> {
    let path = path.as_ref();
    Ok(sha256_bytes(
        fs::read(path).with_context(|| format!("read {}", path.display()))?,
    ))
}

pub fn calculate_digest<T: Serialize>(value: &T) -> Result<String> {
    let mut json = serde_json::to_value(value).context("serialize digest input")?;
    let object = json
        .as_object_mut()
        .context("digest input must be a JSON object")?;
    object.remove("digest");
    Ok(sha256_bytes(stable_json(&json)?))
}

pub fn assert_digest<T: Serialize>(value: &T, actual: &str, label: &str) -> Result<()> {
    if calculate_digest(value)? != actual {
        bail!("{label} digest mismatch; the file was modified");
    }
    Ok(())
}

pub fn normalize_hex(value: &str, label: &str) -> Result<String> {
    let body = value.strip_prefix("0x").unwrap_or(value);
    if !body.len().is_multiple_of(2) || !body.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("{label} must be an even-length hexadecimal value");
    }
    Ok(format!("0x{}", body.to_ascii_lowercase()))
}

pub fn decode_hex(value: &str, label: &str) -> Result<Vec<u8>> {
    let normalized = normalize_hex(value, label)?;
    hex::decode(&normalized[2..]).with_context(|| format!("decode {label}"))
}

pub fn normalize_address(value: &str, label: &str) -> Result<String> {
    let address: Address = value
        .parse()
        .with_context(|| format!("{label} must be a 20-byte address"))?;
    Ok(format!("{address:#x}"))
}

pub fn parse_unsigned(value: &str, label: &str, bits: usize) -> Result<U256> {
    let valid_decimal = value == "0"
        || (!value.starts_with('0') && value.bytes().all(|byte| byte.is_ascii_digit()));
    let valid_hex = value
        .strip_prefix("0x")
        .is_some_and(|body| !body.is_empty() && body.bytes().all(|b| b.is_ascii_hexdigit()));
    if !valid_decimal && !valid_hex {
        bail!("{label} must be an unsigned integer");
    }
    let parsed = if let Some(body) = value.strip_prefix("0x") {
        U256::from_str_radix(body, 16)
    } else {
        U256::from_str_radix(value, 10)
    }
    .with_context(|| format!("parse {label}"))?;
    if bits < 256 && parsed >= (U256::from(1_u8) << bits) {
        bail!("{label} exceeds uint{bits}");
    }
    Ok(parsed)
}

pub fn keccak_hex(value: &str) -> Result<String> {
    Ok(format!(
        "{:#x}",
        keccak256(decode_hex(value, "keccak input")?)
    ))
}

pub fn resolve_from(base: impl AsRef<Path>, value: impl AsRef<Path>) -> PathBuf {
    let value = value.as_ref();
    if value.is_absolute() {
        value.to_path_buf()
    } else {
        base.as_ref().join(value)
    }
}

pub fn interpolate(
    value: &str,
    variables: &std::collections::BTreeMap<String, String>,
) -> Result<String> {
    let regex = Regex::new(r"\{([A-Za-z][A-Za-z0-9]*)\}").expect("static regex");
    let mut missing = None;
    let replaced = regex.replace_all(value, |captures: &regex::Captures<'_>| {
        let key = &captures[1];
        if let Some(value) = variables.get(key) {
            value.clone()
        } else {
            missing = Some(key.to_owned());
            String::new()
        }
    });
    if let Some(key) = missing {
        bail!("unknown command placeholder: {{{key}}}");
    }
    Ok(replaced.into_owned())
}

pub fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn requires_mainnet_acknowledgement(chain_id: u64) -> bool {
    matches!(chain_id, 1 | 4663)
}

pub fn status(message: &str) {
    if io::stderr().is_terminal() {
        eprintln!("{message}");
    }
}

pub fn write_json(path: impl AsRef<Path>, value: &impl Serialize) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let mut bytes = serde_json::to_vec_pretty(value).context("serialize JSON")?;
    bytes.push(b'\n');
    let temporary = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|v| v.to_str()).unwrap_or("json")
    ));
    fs::write(&temporary, bytes).with_context(|| format!("write {}", temporary.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(&temporary, path).with_context(|| format!("replace {}", path.display()))?;
    Ok(())
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: impl AsRef<Path>) -> Result<T> {
    let path = path.as_ref();
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_json_orders_keys() {
        let value = serde_json::json!({"z": 1, "a": {"d": 2, "b": 3}});
        assert_eq!(stable_json(&value).unwrap(), r#"{"a":{"b":3,"d":2},"z":1}"#);
    }

    #[test]
    fn interpolation_fails_closed() {
        let variables =
            std::collections::BTreeMap::from([("rpc".to_owned(), "http://localhost".to_owned())]);
        assert_eq!(
            interpolate("{rpc}/x", &variables).unwrap(),
            "http://localhost/x"
        );
        assert!(
            interpolate("{missing}", &variables)
                .unwrap_err()
                .to_string()
                .contains("unknown command placeholder")
        );
    }

    #[test]
    fn known_mainnets_require_explicit_acknowledgement() {
        assert!(requires_mainnet_acknowledgement(1));
        assert!(requires_mainnet_acknowledgement(4663));
        assert!(!requires_mainnet_acknowledgement(46630));
        assert!(!requires_mainnet_acknowledgement(31337));
    }
}
