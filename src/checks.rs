use std::path::Path;

use anyhow::Result;

use crate::{
    model::{CheckEvidence, FoundryTestSummary, LoadedConfig},
    process::{
        CommandResult, FoundryTestKind, FoundryTestRequirements, require_foundry_tests,
        require_success,
    },
    util::{sha256_bytes, status},
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
    commands
        .into_iter()
        .map(|(name, command, test_requirements)| {
            status(&format!("Running {name} check..."));
            let (result, test_summary) = if let Some(requirements) = test_requirements {
                let (result, summary) = require_foundry_tests(&command, cwd, None, requirements)?;
                (result, Some(summary))
            } else {
                (require_success(&command, cwd, None, false)?, None)
            };
            Ok(evidence(name, &result, test_summary))
        })
        .collect()
}
