use std::path::Path;

use anyhow::Result;

use crate::{
    model::{CheckEvidence, LoadedConfig},
    process::{CommandResult, FoundryTestKind, require_foundry_tests, require_success},
    util::{sha256_bytes, status},
};

fn evidence(name: &str, result: &CommandResult) -> CheckEvidence {
    CheckEvidence {
        name: name.to_owned(),
        command: result.command.clone(),
        duration_ms: result.duration_ms,
        stdout_hash: sha256_bytes(&result.stdout),
        stderr_hash: sha256_bytes(&result.stderr),
    }
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
            "static-analysis",
            config.value.checks.static_analysis.clone(),
            None,
        ),
        (
            "build",
            ["forge", "build", "--force"].map(str::to_owned).to_vec(),
            None,
        ),
        (
            "unit",
            config.value.checks.unit.clone(),
            Some(FoundryTestKind::Unit),
        ),
        (
            "fuzz",
            config.value.checks.fuzz.clone(),
            Some(FoundryTestKind::Fuzz),
        ),
        (
            "invariant",
            config.value.checks.invariant.clone(),
            Some(FoundryTestKind::Invariant),
        ),
    ];
    let cwd = Path::new(&config.project_root);
    commands
        .into_iter()
        .map(|(name, command, test_kind)| {
            status(&format!("Running {name} check..."));
            let result = if let Some(kind) = test_kind {
                require_foundry_tests(&command, cwd, None, kind)?
            } else {
                require_success(&command, cwd, None, false)?
            };
            Ok(evidence(name, &result))
        })
        .collect()
}
