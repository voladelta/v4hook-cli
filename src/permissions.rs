use std::{collections::BTreeSet, path::Path};

use alloy_primitives::Address;
use anyhow::{Context, Result, bail};
use regex::Regex;

use crate::process::require_success;

pub const HOOK_PERMISSIONS: [(&str, u16); 14] = [
    ("beforeInitialize", 1 << 13),
    ("afterInitialize", 1 << 12),
    ("beforeAddLiquidity", 1 << 11),
    ("afterAddLiquidity", 1 << 10),
    ("beforeRemoveLiquidity", 1 << 9),
    ("afterRemoveLiquidity", 1 << 8),
    ("beforeSwap", 1 << 7),
    ("afterSwap", 1 << 6),
    ("beforeDonate", 1 << 5),
    ("afterDonate", 1 << 4),
    ("beforeSwapReturnDelta", 1 << 3),
    ("afterSwapReturnDelta", 1 << 2),
    ("afterAddLiquidityReturnDelta", 1 << 1),
    ("afterRemoveLiquidityReturnDelta", 1),
];

pub fn permission_flags(permissions: &[String]) -> Result<u16> {
    if permissions.len() > 14 {
        bail!("hook permissions cannot contain more than 14 entries");
    }
    let unique: BTreeSet<&str> = permissions.iter().map(String::as_str).collect();
    let dependencies = [
        ("beforeSwapReturnDelta", "beforeSwap"),
        ("afterSwapReturnDelta", "afterSwap"),
        ("afterAddLiquidityReturnDelta", "afterAddLiquidity"),
        ("afterRemoveLiquidityReturnDelta", "afterRemoveLiquidity"),
    ];
    for permission in &unique {
        if !HOOK_PERMISSIONS.iter().any(|(name, _)| name == permission) {
            bail!("unknown hook permission: {permission}");
        }
    }
    for (permission, dependency) in dependencies {
        if unique.contains(permission) && !unique.contains(dependency) {
            bail!("{permission} requires {dependency}");
        }
    }
    Ok(HOOK_PERMISSIONS
        .iter()
        .filter(|(name, _)| unique.contains(name))
        .fold(0, |flags, (_, bit)| flags | bit))
}

pub fn permission_suffix(flags: u16) -> String {
    format!("{flags:04x}")
}

pub fn verify_address_flags(address: &str, expected: u16) -> Result<()> {
    let address: Address = address.parse().context("parse hook address")?;
    let bytes = address.as_slice();
    let actual = (u16::from(bytes[18]) << 8 | u16::from(bytes[19])) & 0x3fff;
    if actual != expected {
        bail!("hook address flags mismatch: expected 0x{expected:x}, got 0x{actual:x}");
    }
    Ok(())
}

pub fn validate_hook_address_fee(permissions: &[String], fee: u32) -> Result<()> {
    if fee > 0x00ff_ffff {
        bail!("pool fee exceeds uint24");
    }
    if permissions.is_empty() && fee & 0x0080_0000 == 0 {
        bail!("a nonzero hook with no callback permissions requires a dynamic pool fee");
    }
    Ok(())
}

pub fn probe_hook_permissions(
    rpc_url: &str,
    address: &str,
    expected: &[String],
    cwd: &Path,
) -> Result<()> {
    let command = vec![
        "cast".to_owned(),
        "call".to_owned(),
        address.to_owned(),
        "getHookPermissions()((bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,bool,bool))".to_owned(),
        "--rpc-url".to_owned(),
        rpc_url.to_owned(),
    ];
    let result = require_success(&command, cwd, None, false)?;
    let regex = Regex::new(r"\b(true|false)\b").expect("static regex");
    let values: Vec<bool> = regex
        .captures_iter(&result.stdout)
        .map(|capture| &capture[1] == "true")
        .collect();
    if values.len() != HOOK_PERMISSIONS.len() {
        bail!(
            "getHookPermissions returned {} booleans; expected {}",
            values.len(),
            HOOK_PERMISSIONS.len()
        );
    }
    let actual: BTreeSet<&str> = HOOK_PERMISSIONS
        .iter()
        .zip(values)
        .filter_map(|((name, _), enabled)| enabled.then_some(*name))
        .collect();
    let expected: BTreeSet<&str> = expected.iter().map(String::as_str).collect();
    if actual != expected {
        bail!(
            "getHookPermissions mismatch: expected [{}], got [{}]",
            expected.into_iter().collect::<Vec<_>>().join(", "),
            actual.into_iter().collect::<Vec<_>>().join(", ")
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_canonical_mask() {
        let permissions = ["beforeSwap", "afterSwap", "beforeSwapReturnDelta"].map(str::to_owned);
        assert_eq!(permission_flags(&permissions).unwrap(), 0x00c8);
        assert_eq!(permission_suffix(0x00c8), "00c8");
    }

    #[test]
    fn return_delta_requires_callback() {
        assert!(permission_flags(&["afterSwapReturnDelta".to_owned()]).is_err());
    }
}
