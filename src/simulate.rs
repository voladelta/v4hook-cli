use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Result, bail};

use crate::{
    anvil::start_anvil,
    artifact::{code_hash, mask_immutable_references},
    checks::run_check_suite,
    config::{broadcast_authority, load_config, rpc_url_from_env},
    model::{CommandEvidence, DeploymentPlan, SimulationEvidence},
    permissions::probe_hook_permissions,
    plan::{absolute_path, network_contract_environment, read_deployment_plan, verify_plan_inputs},
    process::{
        FoundryTestKind, FoundryTestRequirements, redact_command, require_foundry_tests,
        require_success,
    },
    rpc::{block_hash, code_at, prepare_anvil_sender},
    util::{calculate_digest, interpolate, now_iso, sha256_bytes, status, write_json},
};

pub struct DeploymentSimulationContext {
    pub plan: DeploymentPlan,
    pub plan_file: PathBuf,
    pub target_rpc: String,
}

pub fn prepare_deployment_simulation(
    plan_file: impl AsRef<Path>,
) -> Result<DeploymentSimulationContext> {
    status("Verifying the deployment plan and project inputs...");
    let plan_file = absolute_path(plan_file)?;
    let plan = read_deployment_plan(&plan_file)?;
    verify_plan_inputs(&plan)?;
    let config = load_config(&plan.config_path)?;
    run_check_suite(&config)?;
    let target_rpc = rpc_url_from_env(&plan.network.rpc_url_env, Path::new(&plan.project_root))?;
    if block_hash(&target_rpc, plan.network.fork_block_number)? != plan.network.fork_block_hash {
        bail!("planned fork block hash no longer matches the target chain");
    }
    Ok(DeploymentSimulationContext {
        plan,
        plan_file,
        target_rpc,
    })
}

#[allow(clippy::too_many_lines)]
pub fn execute_deployment_simulation(
    context: &DeploymentSimulationContext,
    anvil_rpc_url: &str,
) -> Result<SimulationEvidence> {
    let plan = &context.plan;
    for (name, expected) in &plan.network.contracts {
        let code = code_at(anvil_rpc_url, &expected.address)?;
        if code == "0x" || code_hash(&code)? != expected.code_hash {
            bail!("{name} code in the fork does not match the deployment plan");
        }
    }
    if code_at(anvil_rpc_url, &plan.hook.predicted_address)? != "0x" {
        bail!("predicted hook address is occupied in the fork before simulation");
    }
    let base_variables = BTreeMap::from([
        ("anvilRpc".to_owned(), anvil_rpc_url.to_owned()),
        (
            "anvilSender".to_owned(),
            crate::anvil::ANVIL_DEFAULT_SENDER.to_owned(),
        ),
        ("projectRoot".to_owned(), plan.project_root.clone()),
        (
            "planPath".to_owned(),
            context.plan_file.to_string_lossy().into_owned(),
        ),
        ("planDigest".to_owned(), plan.digest.clone()),
        (
            "predictedAddress".to_owned(),
            plan.hook.predicted_address.clone(),
        ),
        ("chainId".to_owned(), plan.network.chain_id.to_string()),
    ]);
    let mut base_environment = network_contract_environment(&plan.network)?;
    base_environment.extend([
        ("V4HOOK_ANVIL_RPC_URL".to_owned(), anvil_rpc_url.to_owned()),
        (
            "V4HOOK_SIMULATOR_ADDRESS".to_owned(),
            crate::anvil::ANVIL_DEFAULT_SENDER.to_owned(),
        ),
        (
            "V4HOOK_PREDICTED_ADDRESS".to_owned(),
            plan.hook.predicted_address.clone(),
        ),
        ("V4HOOK_PLAN_DIGEST".to_owned(), plan.digest.clone()),
        ("V4HOOK_HOOK_SALT".to_owned(), plan.hook.salt.clone()),
        (
            "V4HOOK_CONSTRUCTOR_ARGS".to_owned(),
            plan.artifact.constructor_args.clone(),
        ),
    ]);
    let mut commands = Vec::new();
    for step in &plan.simulation.steps {
        status(&format!(
            "Running {} simulation step...",
            step.kind.as_str()
        ));
        let sender = broadcast_authority(&step.required_authorities)?
            .unwrap_or(crate::anvil::ANVIL_DEFAULT_SENDER);
        prepare_anvil_sender(anvil_rpc_url, sender)?;
        let mut variables = base_variables.clone();
        variables.insert("anvilSender".to_owned(), sender.to_owned());
        let mut environment = base_environment.clone();
        environment.insert("V4HOOK_SIMULATOR_ADDRESS".to_owned(), sender.to_owned());
        let command = step
            .command
            .iter()
            .map(|part| interpolate(part, &variables))
            .collect::<Result<Vec<_>>>()?;
        let (command_result, test_summary) = if matches!(
            step.kind,
            crate::model::SimulationKind::Quadrants | crate::model::SimulationKind::Postconditions
        ) {
            let (result, summary) = require_foundry_tests(
                &command,
                Path::new(&plan.project_root),
                Some(&environment),
                FoundryTestRequirements::kind(FoundryTestKind::Any),
            )?;
            (result, Some(summary))
        } else {
            (
                require_success(
                    &command,
                    Path::new(&plan.project_root),
                    Some(&environment),
                    false,
                )?,
                None,
            )
        };
        commands.push(CommandEvidence {
            kind: step.kind.clone(),
            command: redact_command(&command),
            exit_code: command_result.exit_code,
            duration_ms: command_result.duration_ms,
            stdout_hash: sha256_bytes(&command_result.stdout),
            stderr_hash: sha256_bytes(&command_result.stderr),
            test_summary,
        });
    }
    let deployed_code = code_at(anvil_rpc_url, &plan.hook.predicted_address)?;
    if deployed_code == "0x" {
        bail!("simulation completed without deploying code at the predicted hook address");
    }
    let normalized_runtime =
        mask_immutable_references(&deployed_code, &plan.artifact.immutable_references)?;
    if code_hash(&normalized_runtime)? != plan.artifact.runtime_bytecode_hash {
        bail!("simulated hook runtime bytecode does not match the planned artifact");
    }
    let deployed_runtime_code_hash = code_hash(&deployed_code)?;
    probe_hook_permissions(
        anvil_rpc_url,
        &plan.hook.predicted_address,
        &plan.hook.permissions,
        Path::new(&plan.project_root),
    )?;
    let mut evidence = SimulationEvidence {
        schema_version: "v4hook.simulation-evidence.v1".to_owned(),
        created_at: now_iso(),
        plan_digest: plan.digest.clone(),
        fork_block_number: plan.network.fork_block_number,
        fork_block_hash: plan.network.fork_block_hash.clone(),
        anvil_version: plan.toolchain.anvil.clone(),
        anvil_rpc_url: "local-anvil".to_owned(),
        commands,
        deployed_runtime_code_hash,
        passed: true,
        digest: String::new(),
    };
    evidence.digest = calculate_digest(&evidence)?;
    Ok(evidence)
}

pub fn simulate_deployment(
    plan_file: impl AsRef<Path>,
    output_file: Option<&Path>,
) -> Result<SimulationEvidence> {
    let context = prepare_deployment_simulation(plan_file)?;
    let plan = &context.plan;
    let project_root = Path::new(&context.plan.project_root);
    status("Starting a pinned Anvil fork...");
    let mut anvil = start_anvil(
        &context.target_rpc,
        &plan.network.rpc_url_env,
        plan.network.fork_block_number,
        plan.network.chain_id,
        &plan.simulation.anvil_args,
        project_root,
    )?;
    let result = execute_deployment_simulation(&context, &anvil.rpc_url).and_then(|evidence| {
        if let Some(output_file) = output_file {
            write_json(output_file, &evidence)?;
        }
        Ok(evidence)
    });
    anvil.stop();
    result
}
