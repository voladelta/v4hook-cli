use std::{path::Path, time::Instant};

use anyhow::{Result, bail};

use crate::{
    artifact::{load_artifact, make_init_code},
    model::{CheckEvidence, CodeSizeSummary, FoundryTestSummary, LoadedConfig},
    process::{
        CommandResult, FoundryTestKind, FoundryTestRequirements, require_foundry_tests,
        require_gas_snapshot, require_success,
    },
    slither::run_slither,
    util::{decode_hex, resolve_from, sha256_bytes, stable_json, status},
};

fn evidence(
    name: &str,
    result: &CommandResult,
    test_summary: Option<FoundryTestSummary>,
) -> CheckEvidence {
    CheckEvidence {
        name: name.to_owned(),
        command: result.command.clone(),
        duration_ms: result.duration_ms,
        stdout_hash: sha256_bytes(&result.stdout),
        stderr_hash: sha256_bytes(&result.stderr),
        test_summary,
        slither_summary: None,
        code_size_summary: None,
    }
}

fn code_size_evidence(config: &LoadedConfig) -> Result<CheckEvidence> {
    let started = Instant::now();
    let artifact_path = resolve_from(&config.project_root, &config.value.contract.artifact);
    let artifact = load_artifact(&artifact_path)?;
    let init_code = make_init_code(
        &artifact.creation_bytecode,
        &config.value.contract.constructor_args,
    )?;
    let summary = CodeSizeSummary {
        unit: "bytes".to_owned(),
        runtime: u64::try_from(decode_hex(&artifact.runtime_bytecode, "runtime bytecode")?.len())?,
        runtime_limit: config.value.checks.code_size.max_runtime_bytes,
        init_code: u64::try_from(decode_hex(&init_code, "init code")?.len())?,
        init_code_limit: config.value.checks.code_size.max_init_code_bytes,
    };
    if summary.runtime > summary.runtime_limit {
        bail!(
            "hook runtime bytecode is {} bytes, above the configured {} byte limit",
            summary.runtime,
            summary.runtime_limit
        );
    }
    if summary.init_code > summary.init_code_limit {
        bail!(
            "hook init code is {} bytes, above the configured {} byte limit",
            summary.init_code,
            summary.init_code_limit
        );
    }
    let encoded = stable_json(&summary)?;
    Ok(CheckEvidence {
        name: "code-size".to_owned(),
        command: vec![
            "artifact".to_owned(),
            artifact_path.to_string_lossy().into_owned(),
        ],
        duration_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
        stdout_hash: sha256_bytes(encoded),
        stderr_hash: sha256_bytes([]),
        test_summary: None,
        slither_summary: None,
        code_size_summary: Some(summary),
    })
}

pub fn run_check_suite(config: &LoadedConfig) -> Result<Vec<CheckEvidence>> {
    let commands = vec![
        (
            "format",
            ["forge", "fmt", "--check"].map(str::to_owned).to_vec(),
            None,
        ),
        ("lint", ["forge", "lint"].map(str::to_owned).to_vec(), None),
        (
            "build",
            ["forge", "build", "--force"].map(str::to_owned).to_vec(),
            None,
        ),
        (
            "unit",
            config.value.checks.unit.clone(),
            Some(FoundryTestRequirements::kind(FoundryTestKind::Unit)),
        ),
        (
            "fuzz",
            config.value.checks.fuzz.clone(),
            Some(FoundryTestRequirements {
                kind: FoundryTestKind::Fuzz,
                minimum_fuzz_runs: Some(config.value.checks.minimum_fuzz_runs),
                minimum_invariant_runs: None,
                minimum_invariant_depth: None,
            }),
        ),
        (
            "invariant",
            config.value.checks.invariant.clone(),
            Some(FoundryTestRequirements {
                kind: FoundryTestKind::Invariant,
                minimum_fuzz_runs: None,
                minimum_invariant_runs: Some(config.value.checks.minimum_invariant_runs),
                minimum_invariant_depth: Some(config.value.checks.minimum_invariant_depth),
            }),
        ),
    ];
    let cwd = Path::new(&config.project_root);
    let mut checks = Vec::new();
    for (name, command, test_requirements) in commands {
        status(&format!("Running {name} check..."));
        let (result, test_summary) = if let Some(requirements) = test_requirements {
            let (result, summary) = require_foundry_tests(&command, cwd, None, requirements)?;
            (result, Some(summary))
        } else {
            (require_success(&command, cwd, None, false)?, None)
        };
        checks.push(evidence(name, &result, test_summary));
        if name == "lint" {
            status("Running structured Slither check...");
            let (result, summary) = run_slither(config)?;
            let mut item = evidence("static-analysis", &result, None);
            item.slither_summary = Some(summary);
            checks.push(item);
        }
        if name == "build" {
            status("Checking hook code-size budgets...");
            checks.push(code_size_evidence(config)?);
            if !config.value.checks.gas_snapshot.is_empty() {
                status("Checking the committed gas budget...");
                let (result, identical_diffs) =
                    require_gas_snapshot(&config.value.checks.gas_snapshot, cwd, None)?;
                if identical_diffs > 0 {
                    status(&format!(
                        "Foundry returned exit 1 for {identical_diffs} byte-identical snapshot rows; the committed gas budget is unchanged."
                    ));
                }
                checks.push(evidence("gas-budget", &result, None));
            }
        }
    }
    Ok(checks)
}
