use std::path::Path;

use anyhow::{Result, bail};
use serde_json::{Value, json};

use crate::{
    artifact::{code_hash, mask_immutable_references},
    config::rpc_url_from_env,
    model::DeploymentPlan,
    permissions::probe_hook_permissions,
    plan::{absolute_path, network_contract_environment, read_deployment_plan, verify_plan_inputs},
    process::{redact_command, require_success},
    rpc::{block_number, chain_id, code_at},
    simulate::simulate_deployment,
    util::{
        normalize_address, now_iso, requires_mainnet_acknowledgement, sha256_bytes, status,
        write_json,
    },
};

pub fn verify_network_at_rpc(
    plan: &DeploymentPlan,
    rpc_url: &str,
    require_empty_hook: bool,
) -> Result<()> {
    if chain_id(rpc_url)? != plan.network.chain_id {
        bail!("live RPC chain ID no longer matches the plan");
    }
    for (name, expected) in &plan.network.contracts {
        let code = code_at(rpc_url, &expected.address)?;
        if code == "0x" {
            bail!("{name} no longer has code at {}", expected.address);
        }
        if code_hash(&code)? != expected.code_hash {
            bail!("{name} code hash changed after planning");
        }
    }
    if require_empty_hook && code_at(rpc_url, &plan.hook.predicted_address)? != "0x" {
        bail!(
            "predicted hook address is occupied: {}",
            plan.hook.predicted_address
        );
    }
    Ok(())
}

pub fn verify_hook_at_rpc(plan: &DeploymentPlan, rpc_url: &str) -> Result<(String, String)> {
    let code = code_at(rpc_url, &plan.hook.predicted_address)?;
    if code == "0x" {
        bail!("no hook code deployed at {}", plan.hook.predicted_address);
    }
    let normalized = mask_immutable_references(&code, &plan.artifact.immutable_references)?;
    if code_hash(&normalized)? != plan.artifact.runtime_bytecode_hash {
        bail!("deployed hook runtime does not match the planned artifact template");
    }
    probe_hook_permissions(
        rpc_url,
        &plan.hook.predicted_address,
        &plan.hook.permissions,
        Path::new(&plan.project_root),
    )?;
    Ok((plan.hook.predicted_address.clone(), code_hash(&code)?))
}

pub fn verify_hook_deployment(plan_file: impl AsRef<Path>) -> Result<(String, String)> {
    let plan = read_deployment_plan(plan_file)?;
    let rpc_url = rpc_url_from_env(&plan.network.rpc_url_env, Path::new(&plan.project_root))?;
    verify_network_at_rpc(&plan, &rpc_url, false)?;
    verify_hook_at_rpc(&plan, &rpc_url)
}

fn require_fresh_fork(plan: &DeploymentPlan, rpc_url: &str) -> Result<()> {
    let head = block_number(rpc_url)?;
    let drift = head
        .checked_sub(plan.network.fork_block_number)
        .ok_or_else(|| anyhow::anyhow!("fork evidence block is ahead of the live chain"))?;
    if drift > plan.simulation.max_fork_block_drift {
        bail!("fork evidence is stale by {drift} blocks; create a new plan");
    }
    Ok(())
}

pub struct DeployInput<'a> {
    pub plan_file: &'a Path,
    pub account: &'a str,
    pub sender: &'a str,
    pub confirmation: &'a str,
    pub mainnet: bool,
    pub verify: bool,
    pub evidence_output: &'a Path,
    pub record_output: &'a Path,
}

pub fn deploy_hook(input: &DeployInput<'_>) -> Result<Value> {
    let plan_file = absolute_path(input.plan_file)?;
    let plan = read_deployment_plan(&plan_file)?;
    verify_plan_inputs(&plan)?;
    if requires_mainnet_acknowledgement(plan.network.chain_id) && !input.mainnet {
        bail!(
            "chain {} mainnet deployment requires --mainnet",
            plan.network.chain_id
        );
    }
    let expected_confirmation = format!(
        "DEPLOY:{}:{}",
        plan.network.chain_id,
        plan.hook.predicted_address.to_ascii_lowercase()
    );
    if input.confirmation != expected_confirmation {
        bail!("confirmation mismatch; expected {expected_confirmation}");
    }
    let sender = normalize_address(input.sender, "sender")?;
    status("Rerunning the mandatory fork simulation before broadcast...");
    let evidence = simulate_deployment(&plan_file, Some(input.evidence_output))?;
    verify_plan_inputs(&plan)?;
    let rpc_url = rpc_url_from_env(&plan.network.rpc_url_env, Path::new(&plan.project_root))?;
    require_fresh_fork(&plan, &rpc_url)?;
    verify_network_at_rpc(&plan, &rpc_url, true)?;

    let mut command = vec![
        "forge".to_owned(),
        "script".to_owned(),
        plan.deployment.script.clone(),
        "--account".to_owned(),
        input.account.to_owned(),
        "--sender".to_owned(),
        sender.clone(),
        "--broadcast".to_owned(),
    ];
    if input.verify {
        command.push("--verify".to_owned());
    }
    let mut environment = network_contract_environment(&plan.network)?;
    environment.extend([
        (
            "V4HOOK_PLAN_PATH".to_owned(),
            plan_file.to_string_lossy().into_owned(),
        ),
        ("V4HOOK_PLAN_DIGEST".to_owned(), plan.digest.clone()),
        ("V4HOOK_HOOK_SALT".to_owned(), plan.hook.salt.clone()),
        (
            "V4HOOK_PREDICTED_ADDRESS".to_owned(),
            plan.hook.predicted_address.clone(),
        ),
        ("FOUNDRY_ETH_RPC_URL".to_owned(), rpc_url.clone()),
        ("ETH_RPC_URL".to_owned(), rpc_url.clone()),
    ]);
    status("Broadcasting the planned hook deployment...");
    let result = require_success(&command, &plan.project_root, Some(&environment), false)?;
    status("Verifying the live hook bytecode and permissions...");
    let (hook_address, runtime_code_hash) = verify_hook_at_rpc(&plan, &rpc_url)?;
    if runtime_code_hash != evidence.deployed_runtime_code_hash {
        bail!("live runtime code hash differs from the mandatory fork simulation");
    }
    let record = json!({
        "schemaVersion": "v4hook.deployment-record.v1",
        "createdAt": now_iso(),
        "planDigest": plan.digest,
        "simulationEvidenceDigest": evidence.digest,
        "chainId": plan.network.chain_id,
        "sender": sender,
        "account": input.account,
        "hookAddress": hook_address,
        "runtimeCodeHash": runtime_code_hash,
        "command": redact_command(&command),
        "stdoutHash": sha256_bytes(&result.stdout),
        "stderrHash": sha256_bytes(&result.stderr),
    });
    write_json(input.record_output, &record)?;
    Ok(record)
}
