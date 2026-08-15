use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use regex::Regex;
use serde_json::Value;

use crate::{
    model::ImmutableReference,
    process::require_success,
    util::{decode_hex, keccak_hex, normalize_address, normalize_hex, sha256_file},
};

#[derive(Debug, Clone)]
pub struct LoadedArtifact {
    pub file_digest: String,
    pub creation_bytecode: String,
    pub runtime_bytecode: String,
    pub immutable_references: Vec<ImmutableReference>,
}

fn bytecode_object(value: Option<&Value>) -> Option<&str> {
    match value {
        Some(Value::String(value)) => Some(value),
        Some(Value::Object(value)) => value.get("object").and_then(Value::as_str),
        _ => None,
    }
}

pub fn load_artifact(path: &Path) -> Result<LoadedArtifact> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("read artifact {}", path.display()))?;
    let artifact: Value = serde_json::from_str(&raw)
        .with_context(|| format!("invalid Foundry artifact JSON: {}", path.display()))?;
    let creation_bytecode = normalize_hex(
        bytecode_object(artifact.get("bytecode"))
            .context("artifact must contain bytecode.object")?,
        "creation bytecode",
    )?;
    let deployed = artifact
        .get("deployedBytecode")
        .context("artifact must contain deployedBytecode.object")?;
    let runtime_bytecode = normalize_hex(
        bytecode_object(Some(deployed)).context("artifact must contain deployedBytecode.object")?,
        "runtime bytecode",
    )?;
    if creation_bytecode == "0x" || runtime_bytecode == "0x" {
        bail!("artifact bytecode is empty: {}", path.display());
    }
    if deployed
        .get("linkReferences")
        .and_then(Value::as_object)
        .is_some_and(|links| !links.is_empty())
    {
        bail!(
            "linked-library runtime bytecode is not supported in v1; deploy a fully linked artifact"
        );
    }
    let mut immutable_references = Vec::new();
    if let Some(groups) = deployed
        .get("immutableReferences")
        .and_then(Value::as_object)
    {
        for references in groups.values() {
            for reference in references
                .as_array()
                .context("invalid immutableReferences")?
            {
                immutable_references.push(
                    serde_json::from_value(reference.clone())
                        .context("invalid immutable reference")?,
                );
            }
        }
    }
    Ok(LoadedArtifact {
        file_digest: sha256_file(path)?,
        creation_bytecode,
        runtime_bytecode,
        immutable_references,
    })
}

pub fn make_init_code(creation_bytecode: &str, constructor_args: &str) -> Result<String> {
    normalize_hex(
        &format!(
            "{}{}",
            creation_bytecode,
            constructor_args.trim_start_matches("0x")
        ),
        "init code",
    )
}

pub fn mine_create2(
    init_code: &str,
    deployer: &str,
    suffix: &str,
    cwd: &Path,
) -> Result<(String, String)> {
    let command = [
        "cast",
        "create2",
        "--ends-with",
        suffix,
        "--init-code",
        init_code,
        "--deployer",
        deployer,
        "--no-random",
    ]
    .map(str::to_owned)
    .to_vec();
    let result = require_success(&command, cwd, None, false)?;
    let address = Regex::new(r"Address:\s*(0x[0-9a-fA-F]{40})")
        .expect("static regex")
        .captures(&result.stdout)
        .map(|value| value[1].to_owned())
        .context("could not parse cast create2 address")?;
    let salt = Regex::new(r"Salt:\s*(0x[0-9a-fA-F]{64})")
        .expect("static regex")
        .captures(&result.stdout)
        .map(|value| value[1].to_owned())
        .context("could not parse cast create2 salt")?;
    Ok((
        normalize_address(&address, "mined address")?,
        normalize_hex(&salt, "mined salt")?,
    ))
}

pub fn mask_immutable_references(code: &str, references: &[ImmutableReference]) -> Result<String> {
    let mut bytes = decode_hex(code, "runtime bytecode")?;
    for reference in references {
        let end = reference
            .start
            .checked_add(reference.length)
            .context("immutable reference overflow")?;
        if end > bytes.len() {
            bail!("immutable reference is outside runtime bytecode");
        }
        bytes[reference.start..end].fill(0);
    }
    Ok(format!("0x{}", hex::encode(bytes)))
}

pub fn code_hash(code: &str) -> Result<String> {
    keccak_hex(code)
}
