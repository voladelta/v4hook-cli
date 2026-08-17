use std::{collections::BTreeSet, path::Path};

use crate::{
    config::{project_readiness_issues, rpc_url_from_env},
    model::{LoadedConfig, ReadinessReport, ReadinessStage, SimulationEvidence, SimulationKind},
    plan::{read_deployment_plan, verify_plan_inputs},
    util::{assert_digest, read_json, sha256_bytes},
};

pub struct ReadinessInput<'a> {
    pub config: &'a LoadedConfig,
    pub plan: Option<&'a Path>,
    pub simulation: Option<&'a Path>,
}

fn stage(evidence: Vec<String>, issues: Vec<String>) -> ReadinessStage {
    ReadinessStage {
        ready: issues.is_empty(),
        evidence,
        issues,
    }
}

fn configuration_stage(config: &LoadedConfig) -> ReadinessStage {
    let mut issues = project_readiness_issues(config);
    if let Err(error) = rpc_url_from_env(
        &config.value.network.rpc_url_env,
        Path::new(&config.project_root),
    ) {
        issues.push(format!("RPC configuration is unavailable: {error}"));
    }
    stage(vec![format!("config {}", config.path)], issues)
}

fn validate_plan(config: &LoadedConfig, path: &Path) -> anyhow::Result<String> {
    let plan = read_deployment_plan(path)?;
    if plan.config_path != config.path || plan.config_digest != sha256_bytes(&config.raw) {
        anyhow::bail!("deployment plan does not bind the supplied configuration");
    }
    verify_plan_inputs(&plan)?;
    let names = plan
        .checks
        .iter()
        .map(|check| check.name.as_str())
        .collect::<BTreeSet<_>>();
    for required in [
        "format",
        "lint",
        "static-analysis",
        "build",
        "code-size",
        "gas-budget",
        "unit",
        "fuzz",
        "invariant",
    ] {
        if !names.contains(required) {
            anyhow::bail!("deployment plan is missing the {required} check");
        }
    }
    if !plan
        .checks
        .iter()
        .any(|check| check.name == "static-analysis" && check.slither_summary.is_some())
    {
        anyhow::bail!("deployment plan lacks structured Slither evidence");
    }
    if !plan
        .checks
        .iter()
        .any(|check| check.name == "code-size" && check.code_size_summary.is_some())
    {
        anyhow::bail!("deployment plan lacks code-size evidence");
    }
    Ok(plan.digest)
}

fn local_stage(
    config: &LoadedConfig,
    configuration_ready: bool,
    path: Option<&Path>,
) -> (ReadinessStage, Option<String>) {
    let mut issues = Vec::new();
    if !configuration_ready {
        issues.push("project configuration is not ready".to_owned());
    }
    let Some(path) = path else {
        issues.push("no immutable deployment plan was supplied".to_owned());
        return (stage(Vec::new(), issues), None);
    };
    match validate_plan(config, path) {
        Ok(digest) => (
            stage(
                vec![format!("deployment plan {} ({digest})", path.display())],
                issues,
            ),
            Some(digest),
        ),
        Err(error) => {
            issues.push(format!("deployment plan is invalid: {error:#}"));
            (stage(Vec::new(), issues), None)
        }
    }
}

fn validate_simulation(path: &Path, plan_digest: Option<&str>) -> anyhow::Result<String> {
    let evidence: SimulationEvidence = read_json(path)?;
    if evidence.schema_version != "v4hook.simulation-evidence.v1" {
        anyhow::bail!(
            "unsupported simulation evidence schemaVersion: {}",
            evidence.schema_version
        );
    }
    assert_digest(&evidence, &evidence.digest, "simulation evidence")?;
    if Some(evidence.plan_digest.as_str()) != plan_digest {
        anyhow::bail!("simulation evidence does not match the deployment plan");
    }
    if !evidence.passed {
        anyhow::bail!("simulation evidence is not passing");
    }
    for required in [
        SimulationKind::Deploy,
        SimulationKind::Pool,
        SimulationKind::Quadrants,
        SimulationKind::Postconditions,
    ] {
        let command = evidence
            .commands
            .iter()
            .find(|command| command.kind == required)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "simulation evidence is missing the {} stage",
                    required.as_str()
                )
            })?;
        if matches!(
            required,
            SimulationKind::Quadrants | SimulationKind::Postconditions
        ) && command
            .test_summary
            .as_ref()
            .is_none_or(|summary| summary.total == 0)
        {
            anyhow::bail!(
                "{} simulation stage lacks executed-test evidence",
                required.as_str()
            );
        }
    }
    Ok(evidence.digest)
}

fn testnet_stage(
    local_ready: bool,
    plan_digest: Option<&str>,
    path: Option<&Path>,
) -> ReadinessStage {
    let mut issues = Vec::new();
    if !local_ready {
        issues.push("local readiness has not been established".to_owned());
    }
    let Some(path) = path else {
        issues.push("no pinned-fork simulation evidence was supplied".to_owned());
        return stage(Vec::new(), issues);
    };
    match validate_simulation(path, plan_digest) {
        Ok(digest) => stage(
            vec![format!("fork simulation {} ({digest})", path.display())],
            issues,
        ),
        Err(error) => {
            issues.push(format!("simulation evidence is invalid: {error:#}"));
            stage(Vec::new(), issues)
        }
    }
}

fn launch_stage() -> ReadinessStage {
    ReadinessStage {
        ready: false,
        evidence: Vec::new(),
        issues: vec![
            "independent security and economic review is external evidence".to_owned(),
            "production monitoring and incident response must be established".to_owned(),
            "the user must explicitly authorize the named network, wallet, and live actions"
                .to_owned(),
        ],
    }
}

pub fn assess(input: &ReadinessInput<'_>) -> ReadinessReport {
    let configuration = configuration_stage(input.config);
    let (local, plan_digest) = local_stage(input.config, configuration.ready, input.plan);
    let testnet = testnet_stage(local.ready, plan_digest.as_deref(), input.simulation);
    let launch = launch_stage();
    let highest_stage = if testnet.ready {
        "testnet"
    } else if local.ready {
        "local"
    } else if configuration.ready {
        "configuration"
    } else {
        "not-ready"
    }
    .to_owned();
    ReadinessReport {
        schema_version: "v4hook.readiness.v1".to_owned(),
        highest_stage,
        configuration,
        local,
        testnet,
        launch,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_readiness_is_never_self_attested_by_the_cli() {
        let launch = launch_stage();
        assert!(!launch.ready);
        assert!(!launch.issues.is_empty());
    }
}
