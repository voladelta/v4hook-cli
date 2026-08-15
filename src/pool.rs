use std::{collections::BTreeMap, path::Path};

use anyhow::{Result, bail};
use serde_json::{Value, json};

use crate::{
    anvil::start_anvil,
    config::{load_config, rpc_url_from_env},
    deploy::{verify_hook_at_rpc, verify_hook_deployment, verify_network_at_rpc},
    model::{CommandEvidence, DeploymentPlan, PoolPlan, PoolSimulationEvidence},
    plan::{absolute_path, read_deployment_plan, verify_plan_inputs},
    process::{FoundryTestKind, redact_command, require_foundry_tests, require_success},
    rpc::{block_hash, block_number, chain_id},
    util::{
        assert_digest, calculate_digest, interpolate, normalize_address, now_iso, read_json,
        sha256_bytes, status, write_json,
    },
};

fn target_rpc(plan: &DeploymentPlan) -> Result<String> {
    rpc_url_from_env(&plan.network.rpc_url_env, Path::new(&plan.project_root))
}

pub fn create_pool_plan(deployment_plan_file: impl AsRef<Path>) -> Result<PoolPlan> {
    status("Verifying the live hook before planning the pool...");
    let deployment_plan_file = deployment_plan_file.as_ref();
    let deployment = read_deployment_plan(deployment_plan_file)?;
    verify_plan_inputs(&deployment)?;
    let config = load_config(&deployment.config_path)?;
    let pool = config
        .value
        .pool
        .ok_or_else(|| anyhow::anyhow!("config does not define a pool launch"))?;
    verify_hook_deployment(deployment_plan_file)?;
    let rpc_url = target_rpc(&deployment)?;
    let fork_block_number = block_number(&rpc_url)?;
    let mut plan = PoolPlan {
        schema_version: "v4hook.pool-plan.v1".to_owned(),
        created_at: now_iso(),
        deployment_plan_digest: deployment.digest.clone(),
        hook_address: deployment.hook.predicted_address.clone(),
        chain_id: deployment.network.chain_id,
        fork_block_number,
        fork_block_hash: block_hash(&rpc_url, fork_block_number)?,
        max_fork_block_drift: deployment.simulation.max_fork_block_drift,
        pool,
        digest: String::new(),
    };
    plan.digest = calculate_digest(&plan)?;
    Ok(plan)
}

pub fn read_pool_plan(path: impl AsRef<Path>) -> Result<PoolPlan> {
    let plan: PoolPlan = read_json(path)?;
    if plan.schema_version != "v4hook.pool-plan.v1" {
        bail!(
            "unsupported pool plan schemaVersion: {}",
            plan.schema_version
        );
    }
    assert_digest(&plan, &plan.digest, "pool plan")?;
    Ok(plan)
}

fn pool_variables(
    deployment: &DeploymentPlan,
    pool: &PoolPlan,
    sender: &str,
    pool_plan_path: &Path,
) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("anvilSender".to_owned(), sender.to_owned()),
        ("hookAddress".to_owned(), pool.hook_address.clone()),
        (
            "poolPlanPath".to_owned(),
            pool_plan_path.to_string_lossy().into_owned(),
        ),
        ("poolPlanDigest".to_owned(), pool.digest.clone()),
        ("chainId".to_owned(), pool.chain_id.to_string()),
        (
            "poolManager".to_owned(),
            deployment
                .network
                .contracts
                .get("poolManager")
                .map_or_else(String::new, |value| value.address.clone()),
        ),
        (
            "positionManager".to_owned(),
            deployment
                .network
                .contracts
                .get("positionManager")
                .map_or_else(String::new, |value| value.address.clone()),
        ),
        (
            "permit2".to_owned(),
            deployment
                .network
                .contracts
                .get("permit2")
                .map_or_else(String::new, |value| value.address.clone()),
        ),
    ])
}

fn pool_env(pool: &PoolPlan, sender: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("V4HOOK_SIMULATOR_ADDRESS".to_owned(), sender.to_owned()),
        ("V4HOOK_POOL_PLAN_DIGEST".to_owned(), pool.digest.clone()),
        ("V4HOOK_HOOK_ADDRESS".to_owned(), pool.hook_address.clone()),
        ("V4HOOK_CURRENCY0".to_owned(), pool.pool.currency0.clone()),
        ("V4HOOK_CURRENCY1".to_owned(), pool.pool.currency1.clone()),
        ("V4HOOK_POOL_FEE".to_owned(), pool.pool.fee.to_string()),
        (
            "V4HOOK_TICK_SPACING".to_owned(),
            pool.pool.tick_spacing.to_string(),
        ),
        (
            "V4HOOK_SQRT_PRICE_X96".to_owned(),
            pool.pool.sqrt_price_x96.clone(),
        ),
        (
            "V4HOOK_TICK_LOWER".to_owned(),
            pool.pool.tick_lower.to_string(),
        ),
        (
            "V4HOOK_TICK_UPPER".to_owned(),
            pool.pool.tick_upper.to_string(),
        ),
        ("V4HOOK_LIQUIDITY".to_owned(), pool.pool.liquidity.clone()),
        (
            "V4HOOK_AMOUNT0_MAX".to_owned(),
            pool.pool.amount0_max.clone(),
        ),
        (
            "V4HOOK_AMOUNT1_MAX".to_owned(),
            pool.pool.amount1_max.clone(),
        ),
        ("V4HOOK_RECIPIENT".to_owned(), pool.pool.recipient.clone()),
        ("V4HOOK_HOOK_DATA".to_owned(), pool.pool.hook_data.clone()),
    ])
}

pub struct SimulatePoolInput<'a> {
    pub deployment_plan: &'a Path,
    pub pool_plan: &'a Path,
    pub output: Option<&'a Path>,
}

pub fn simulate_pool(input: &SimulatePoolInput<'_>) -> Result<PoolSimulationEvidence> {
    status("Verifying the pool plan and deployment inputs...");
    let deployment = read_deployment_plan(input.deployment_plan)?;
    verify_plan_inputs(&deployment)?;
    let pool = read_pool_plan(input.pool_plan)?;
    if pool.deployment_plan_digest != deployment.digest {
        bail!("pool plan belongs to a different deployment plan");
    }
    verify_hook_deployment(input.deployment_plan)?;
    let rpc_url = target_rpc(&deployment)?;
    if block_hash(&rpc_url, pool.fork_block_number)? != pool.fork_block_hash {
        bail!("pool fork block hash changed");
    }
    let project_root = Path::new(&deployment.project_root);
    status("Starting a pinned Anvil fork for the pool simulation...");
    let mut anvil = start_anvil(
        &rpc_url,
        &deployment.network.rpc_url_env,
        pool.fork_block_number,
        pool.chain_id,
        &deployment.simulation.anvil_args,
        project_root,
    )?;
    let result = (|| {
        verify_network_at_rpc(&deployment, &anvil.rpc_url, false)?;
        verify_hook_at_rpc(&deployment, &anvil.rpc_url)?;
        let pool_plan_path = absolute_path(input.pool_plan)?;
        let mut variables = pool_variables(&deployment, &pool, &anvil.sender, &pool_plan_path);
        variables.insert("anvilRpc".to_owned(), anvil.rpc_url.clone());
        let mut environment = pool_env(&pool, &anvil.sender);
        environment.insert("V4HOOK_ANVIL_RPC_URL".to_owned(), anvil.rpc_url.clone());
        let mut commands = Vec::new();
        for step in &pool.pool.simulation_steps {
            status(&format!(
                "Running {} pool simulation step...",
                step.kind.as_str()
            ));
            let command = step
                .command
                .iter()
                .map(|part| interpolate(part, &variables))
                .collect::<Result<Vec<_>>>()?;
            let result = if matches!(
                step.kind,
                crate::model::SimulationKind::Quadrants
                    | crate::model::SimulationKind::Postconditions
            ) {
                require_foundry_tests(
                    &command,
                    project_root,
                    Some(&environment),
                    FoundryTestKind::Any,
                )?
            } else {
                require_success(&command, project_root, Some(&environment), false)?
            };
            commands.push(CommandEvidence {
                kind: step.kind.clone(),
                command: redact_command(&command),
                exit_code: result.exit_code,
                duration_ms: result.duration_ms,
                stdout_hash: sha256_bytes(&result.stdout),
                stderr_hash: sha256_bytes(&result.stderr),
            });
        }
        let mut evidence = PoolSimulationEvidence {
            schema_version: "v4hook.pool-simulation-evidence.v1".to_owned(),
            created_at: now_iso(),
            pool_plan_digest: pool.digest.clone(),
            fork_block_number: pool.fork_block_number,
            fork_block_hash: pool.fork_block_hash.clone(),
            commands,
            passed: true,
            digest: String::new(),
        };
        evidence.digest = calculate_digest(&evidence)?;
        if let Some(output_file) = input.output {
            write_json(output_file, &evidence)?;
        }
        Ok(evidence)
    })();
    anvil.stop();
    result
}

pub struct LaunchPoolInput<'a> {
    pub deployment_plan: &'a Path,
    pub pool_plan: &'a Path,
    pub account: &'a str,
    pub sender: &'a str,
    pub confirmation: &'a str,
    pub mainnet: bool,
    pub evidence_output: &'a Path,
    pub record_output: &'a Path,
}

pub fn launch_pool(input: &LaunchPoolInput<'_>) -> Result<Value> {
    let deployment = read_deployment_plan(input.deployment_plan)?;
    verify_plan_inputs(&deployment)?;
    let pool = read_pool_plan(input.pool_plan)?;
    if pool.deployment_plan_digest != deployment.digest {
        bail!("pool plan belongs to a different deployment plan");
    }
    if pool.chain_id == 1 && !input.mainnet {
        bail!("Ethereum mainnet pool launch requires --mainnet");
    }
    let expected = format!(
        "POOL:{}:{}:{}",
        pool.chain_id,
        pool.hook_address.to_ascii_lowercase(),
        pool.digest
    );
    if input.confirmation != expected {
        bail!("confirmation mismatch; expected {expected}");
    }
    let sender = normalize_address(input.sender, "sender")?;
    status("Rerunning the mandatory pool fork simulation before broadcast...");
    let evidence = simulate_pool(&SimulatePoolInput {
        deployment_plan: input.deployment_plan,
        pool_plan: input.pool_plan,
        output: Some(input.evidence_output),
    })?;
    verify_plan_inputs(&deployment)?;
    let rpc_url = target_rpc(&deployment)?;
    if chain_id(&rpc_url)? != pool.chain_id {
        bail!("live RPC chain ID changed");
    }
    let head = block_number(&rpc_url)?;
    let drift = head
        .checked_sub(pool.fork_block_number)
        .ok_or_else(|| anyhow::anyhow!("pool fork block is ahead of the live chain"))?;
    if drift > pool.max_fork_block_drift {
        bail!("pool simulation is stale by {drift} blocks; create a new pool plan");
    }
    verify_network_at_rpc(&deployment, &rpc_url, false)?;
    verify_hook_at_rpc(&deployment, &rpc_url)?;
    let launch = vec![
        "forge".to_owned(),
        "script".to_owned(),
        pool.pool.launch_script.clone(),
        "--account".to_owned(),
        input.account.to_owned(),
        "--sender".to_owned(),
        sender.clone(),
        "--broadcast".to_owned(),
    ];
    let mut environment = pool_env(&pool, &sender);
    environment.insert("FOUNDRY_ETH_RPC_URL".to_owned(), rpc_url.clone());
    environment.insert("ETH_RPC_URL".to_owned(), rpc_url);
    status("Broadcasting the planned pool launch...");
    let launch_result =
        require_success(&launch, &deployment.project_root, Some(&environment), false)?;
    let pool_plan_path = absolute_path(input.pool_plan)?;
    let variables = pool_variables(&deployment, &pool, &sender, &pool_plan_path);
    let live_verify = pool
        .pool
        .live_verify
        .iter()
        .map(|part| interpolate(part, &variables))
        .collect::<Result<Vec<_>>>()?;
    status("Verifying the live pool state...");
    let verify_result = require_success(
        &live_verify,
        &deployment.project_root,
        Some(&environment),
        false,
    )?;
    let record = json!({
        "schemaVersion": "v4hook.pool-launch-record.v1",
        "createdAt": now_iso(),
        "deploymentPlanDigest": deployment.digest,
        "poolPlanDigest": pool.digest,
        "simulationEvidenceDigest": evidence.digest,
        "chainId": pool.chain_id,
        "hookAddress": pool.hook_address,
        "sender": sender,
        "account": input.account,
        "launchCommand": redact_command(&launch),
        "liveVerifyCommand": redact_command(&live_verify),
        "launchStdoutHash": sha256_bytes(&launch_result.stdout),
        "launchStderrHash": sha256_bytes(&launch_result.stderr),
        "verifyStdoutHash": sha256_bytes(&verify_result.stdout),
        "verifyStderrHash": sha256_bytes(&verify_result.stderr),
    });
    write_json(input.record_output, &record)?;
    Ok(record)
}
