use std::{collections::BTreeMap, path::Path};

use anyhow::{Result, bail};

use crate::{
    anvil::start_anvil,
    artifact::{code_hash, mask_immutable_references},
    checks::run_check_suite,
    config::{load_config, rpc_url_from_env},
    model::{CommandEvidence, SimulationEvidence},
    permissions::probe_hook_permissions,
    plan::{absolute_path, read_deployment_plan, verify_plan_inputs},
    process::{redact_command, require_success},
    rpc::{block_hash, code_at},
    util::{calculate_digest, interpolate, now_iso, sha256_bytes, write_json},
};

#[allow(clippy::too_many_lines)]
pub fn simulate_deployment(
    plan_file: impl AsRef<Path>,
    output_file: Option<&Path>,
) -> Result<SimulationEvidence> {
    let absolute_plan_file = absolute_path(plan_file)?;
    let plan = read_deployment_plan(&absolute_plan_file)?;
    verify_plan_inputs(&plan)?;
    let config = load_config(&plan.config_path)?;
    run_check_suite(&config)?;
    let target_rpc = rpc_url_from_env(&plan.network.rpc_url_env)?;
    if block_hash(&target_rpc, plan.network.fork_block_number)? != plan.network.fork_block_hash {
        bail!("planned fork block hash no longer matches the target chain");
    }

    let project_root = Path::new(&plan.project_root);
    let mut anvil = start_anvil(
        &target_rpc,
        plan.network.fork_block_number,
        plan.network.chain_id,
        &plan.simulation.anvil_args,
        project_root,
    )?;
    let result = (|| {
        for (name, expected) in &plan.network.contracts {
            let code = code_at(&anvil.rpc_url, &expected.address)?;
            if code == "0x" || code_hash(&code)? != expected.code_hash {
                bail!("{name} code in the fork does not match the deployment plan");
            }
        }
        if code_at(&anvil.rpc_url, &plan.hook.predicted_address)? != "0x" {
            bail!("predicted hook address is occupied in the fork before simulation");
        }
        let variables = BTreeMap::from([
            ("anvilRpc".to_owned(), anvil.rpc_url.clone()),
            ("anvilSender".to_owned(), anvil.sender.clone()),
            ("projectRoot".to_owned(), plan.project_root.clone()),
            (
                "planPath".to_owned(),
                absolute_plan_file.to_string_lossy().into_owned(),
            ),
            ("planDigest".to_owned(), plan.digest.clone()),
            (
                "predictedAddress".to_owned(),
                plan.hook.predicted_address.clone(),
            ),
            ("chainId".to_owned(), plan.network.chain_id.to_string()),
        ]);
        let environment = BTreeMap::from([
            ("V4HOOK_ANVIL_RPC_URL".to_owned(), anvil.rpc_url.clone()),
            ("V4HOOK_SIMULATOR_ADDRESS".to_owned(), anvil.sender.clone()),
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
            (
                "V4HOOK_POOL_MANAGER".to_owned(),
                plan.network
                    .contracts
                    .get("poolManager")
                    .map_or_else(String::new, |value| value.address.clone()),
            ),
        ]);
        let mut commands = Vec::new();
        for step in &plan.simulation.steps {
            let command = step
                .command
                .iter()
                .map(|part| interpolate(part, &variables))
                .collect::<Result<Vec<_>>>()?;
            let command_result =
                require_success(&command, project_root, Some(&environment), false)?;
            commands.push(CommandEvidence {
                kind: step.kind.clone(),
                command: redact_command(&command),
                exit_code: command_result.exit_code,
                duration_ms: command_result.duration_ms,
                stdout_hash: sha256_bytes(&command_result.stdout),
                stderr_hash: sha256_bytes(&command_result.stderr),
            });
        }
        let deployed_code = code_at(&anvil.rpc_url, &plan.hook.predicted_address)?;
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
            &anvil.rpc_url,
            &plan.hook.predicted_address,
            &plan.hook.permissions,
            project_root,
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
        if let Some(output_file) = output_file {
            write_json(output_file, &evidence)?;
        }
        Ok(evidence)
    })();
    anvil.stop();
    result
}
