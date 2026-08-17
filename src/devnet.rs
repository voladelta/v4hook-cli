use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::{
    anvil::{AnvilStartOptions, start_anvil_with_options},
    artifact::code_hash,
    config::load_config,
    model::{
        DevnetHookManifest, DevnetManifest, DevnetPoolManifest, DevnetScenarioEvidence,
        DevnetState, DevnetStatus,
    },
    plan::{absolute_path, read_deployment_plan},
    process::{redact_command, run},
    rpc::{
        anvil_accounts, block_hash, block_number, chain_id, code_at, reset_fork, set_anvil_code,
    },
    simulate::{
        DeploymentSimulationContext, execute_deployment_simulation, prepare_deployment_simulation,
    },
    util::{
        assert_digest, calculate_digest, interpolate, normalize_address, now_iso, read_json,
        sha256_bytes, sha256_file, status as report_status, write_json,
    },
};

pub struct DevnetUpInput<'a> {
    pub plan_file: &'a Path,
    pub state_file: &'a Path,
    pub manifest_file: &'a Path,
    pub port: u16,
    pub accounts: Option<u16>,
    pub block_time_seconds: Option<u64>,
}

pub struct DevnetScenarioInput<'a> {
    pub state_file: &'a Path,
    pub scenario: &'a str,
    pub seed: u64,
    pub output: &'a Path,
}

fn absolute_output(path: &Path) -> Result<PathBuf> {
    absolute_path(path)
}

fn read_state(path: &Path) -> Result<DevnetState> {
    let state: DevnetState = read_json(path)?;
    if state.schema_version != "v4hook.devnet-state.v1" {
        bail!(
            "unsupported devnet state schemaVersion: {}",
            state.schema_version
        );
    }
    assert_digest(&state, &state.digest, "devnet state")?;
    Ok(state)
}

fn process_command(pid: u32) -> Result<Option<String>> {
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .context("inspect devnet process")?;
    if !output.status.success() {
        return Ok(None);
    }
    let command = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok((!command.is_empty()).then_some(command))
}

fn has_argument(parts: &[&str], long: &str, short: &str, value: &str) -> bool {
    parts
        .windows(2)
        .any(|pair| (pair[0] == long || pair[0] == short) && pair.get(1).copied() == Some(value))
        || parts.iter().any(|part| *part == format!("{long}={value}"))
}

fn command_matches_anvil(command: &str, port: u16, chain_id: u64) -> bool {
    let parts = command.split_whitespace().collect::<Vec<_>>();
    let executable_is_anvil = parts.first().is_some_and(|part| {
        Path::new(part).file_name().and_then(|name| name.to_str()) == Some("anvil")
    });
    executable_is_anvil
        && has_argument(&parts, "--port", "-p", &port.to_string())
        && has_argument(&parts, "--chain-id", "", &chain_id.to_string())
}

fn require_owned_process(state: &DevnetState) -> Result<()> {
    let command = process_command(state.pid)?
        .with_context(|| format!("devnet process {} is not running", state.pid))?;
    if !command_matches_anvil(&command, state.port, state.chain_id) {
        bail!(
            "refusing to control PID {}; it is not the Anvil process recorded by this devnet",
            state.pid
        );
    }
    Ok(())
}

fn validate_live_state(state: &DevnetState) -> Result<DevnetStatus> {
    require_owned_process(state)?;
    let actual_chain_id = chain_id(&state.rpc_url)?;
    if actual_chain_id != state.chain_id {
        bail!("devnet RPC chain ID changed");
    }
    if block_hash(&state.rpc_url, state.fork_block_number)? != state.fork_block_hash {
        bail!("devnet fork block hash changed");
    }
    if code_at(&state.rpc_url, &state.marker_address)? != state.marker_code {
        bail!("devnet ownership marker is missing or changed");
    }
    let hook_code = code_at(&state.rpc_url, &state.hook_address)?;
    if hook_code == "0x" || code_hash(&hook_code)? != state.deployed_runtime_code_hash {
        bail!("devnet hook runtime code changed");
    }
    let accounts = anvil_accounts(&state.rpc_url)?;
    if accounts != state.accounts {
        bail!("devnet account set changed");
    }
    Ok(DevnetStatus {
        running: true,
        rpc_url: state.rpc_url.clone(),
        chain_id: state.chain_id,
        block_number: block_number(&state.rpc_url)?,
        fork_block_number: state.fork_block_number,
        hook_address: state.hook_address.clone(),
        accounts: accounts.len(),
        plan_digest: state.plan_digest.clone(),
        manifest_path: state.manifest_path.clone(),
        log_path: state.log_path.clone(),
    })
}

fn marker(plan_digest: &str, pid: u32, created_at: &str) -> Result<(String, String)> {
    let digest = sha256_bytes(format!("{plan_digest}:{pid}:{created_at}"));
    let body = digest
        .strip_prefix("sha256:")
        .context("invalid marker digest")?;
    let address = normalize_address(&format!("0x{}", &body[body.len() - 40..]), "devnet marker")?;
    let code = format!("0x7f{body}00");
    Ok((address, code))
}

fn install_marker(rpc_url: &str, address: &str, marker_code: &str) -> Result<()> {
    let existing = code_at(rpc_url, address)?;
    if existing != "0x" && existing != marker_code {
        bail!("refusing to overwrite code at the generated devnet marker address");
    }
    set_anvil_code(rpc_url, address, marker_code)
}

fn exact_config(context: &DeploymentSimulationContext) -> Result<crate::model::LoadedConfig> {
    let raw = fs::read_to_string(&context.plan.config_path)
        .with_context(|| format!("read {}", context.plan.config_path))?;
    if sha256_bytes(raw) != context.plan.config_digest {
        bail!("configuration changed after the deployment plan was created");
    }
    load_config(&context.plan.config_path)
}

fn build_manifest(state: &DevnetState) -> Result<DevnetManifest> {
    let plan = read_deployment_plan(&state.plan_path)?;
    if plan.digest != state.plan_digest {
        bail!("devnet state references a different deployment plan");
    }
    if sha256_file(&plan.artifact.path)? != plan.artifact.file_digest {
        bail!("Foundry artifact changed after the deployment plan was created");
    }
    let artifact: Value = serde_json::from_str(
        &fs::read_to_string(&plan.artifact.path)
            .with_context(|| format!("read artifact {}", plan.artifact.path))?,
    )
    .with_context(|| format!("parse artifact {}", plan.artifact.path))?;
    let abi = artifact
        .get("abi")
        .cloned()
        .context("Foundry artifact is missing abi")?;
    let config_raw = fs::read_to_string(&plan.config_path)
        .with_context(|| format!("read {}", plan.config_path))?;
    if sha256_bytes(config_raw) != plan.config_digest {
        bail!("configuration changed after the deployment plan was created");
    }
    let config = load_config(&plan.config_path)?;
    let contracts = plan
        .network
        .contracts
        .iter()
        .map(|(name, identity)| (name.clone(), identity.address.clone()))
        .collect();
    let scenarios = config
        .value
        .devnet
        .as_ref()
        .map(|devnet| {
            devnet
                .scenarios
                .iter()
                .map(|item| item.name.clone())
                .collect()
        })
        .unwrap_or_default();
    let pool = config.value.pool.map(|pool| DevnetPoolManifest {
        currency0: pool.currency0,
        currency1: pool.currency1,
        fee: pool.fee,
        tick_spacing: pool.tick_spacing,
        sqrt_price_x96: pool.sqrt_price_x96,
        tick_lower: pool.tick_lower,
        tick_upper: pool.tick_upper,
        liquidity: pool.liquidity,
        amount0_max: pool.amount0_max,
        amount1_max: pool.amount1_max,
        recipient: pool.recipient,
        hook_data: pool.hook_data,
    });
    let mut manifest = DevnetManifest {
        schema_version: "v4hook.devnet-manifest.v1".to_owned(),
        created_at: now_iso(),
        warning: "Local deterministic unlocked accounts only. Any browser page that can reach this RPC can mutate it. Never fund or reuse these accounts on a public network."
            .to_owned(),
        rpc_url: state.rpc_url.clone(),
        chain_id: state.chain_id,
        fork_block_number: state.fork_block_number,
        fork_block_hash: state.fork_block_hash.clone(),
        plan_digest: state.plan_digest.clone(),
        hook: DevnetHookManifest {
            address: state.hook_address.clone(),
            abi,
        },
        contracts,
        accounts: state.accounts.clone(),
        pool,
        scenarios,
        digest: String::new(),
    };
    manifest.digest = calculate_digest(&manifest)?;
    Ok(manifest)
}

fn write_manifest(state: &DevnetState, output: &Path) -> Result<DevnetManifest> {
    let manifest = build_manifest(state)?;
    write_json(output, &manifest)?;
    Ok(manifest)
}

fn remove_generated_state(state_file: &Path) -> Result<()> {
    if state_file.exists() {
        fs::remove_file(state_file)
            .with_context(|| format!("remove devnet state {}", state_file.display()))?;
    }
    Ok(())
}

fn require_generated_manifest(path: &Path) -> Result<()> {
    let manifest: DevnetManifest = read_json(path).with_context(|| {
        format!(
            "existing file is not a v4hook devnet manifest: {}",
            path.display()
        )
    })?;
    if manifest.schema_version != "v4hook.devnet-manifest.v1" {
        bail!(
            "existing file is not a supported v4hook devnet manifest: {}",
            path.display()
        );
    }
    assert_digest(&manifest, &manifest.digest, "existing devnet manifest")
}

pub fn up(input: &DevnetUpInput<'_>) -> Result<DevnetStatus> {
    let state_file = absolute_output(input.state_file)?;
    if state_file.exists() {
        bail!(
            "devnet state already exists at {}; run `v4hook devnet status` or `v4hook devnet down` first",
            state_file.display()
        );
    }
    if input.port == 0 {
        bail!("devnet port must be positive");
    }
    let manifest_file = absolute_output(input.manifest_file)?;
    if manifest_file.exists() {
        require_generated_manifest(&manifest_file)?;
    }
    let context = prepare_deployment_simulation(input.plan_file)?;
    let config = exact_config(&context)?;
    let configured = config.value.devnet.as_ref();
    let accounts = input
        .accounts
        .or_else(|| configured.map(|devnet| devnet.accounts))
        .unwrap_or(100);
    if accounts == 0 || accounts > 1_000 {
        bail!("devnet accounts must be between 1 and 1000");
    }
    let block_time_seconds = input
        .block_time_seconds
        .or_else(|| configured.and_then(|devnet| devnet.block_time_seconds));
    if block_time_seconds == Some(0) {
        bail!("devnet block time must be positive");
    }
    let created_at = now_iso();
    let log_name = format!("anvil-{}.log", created_at.replace([':', '.'], "-"));
    let log_path = state_file
        .parent()
        .unwrap_or(Path::new("."))
        .join("devnet")
        .join(log_name);
    let mut anvil = start_anvil_with_options(
        &context.target_rpc,
        &context.plan.network.rpc_url_env,
        context.plan.network.fork_block_number,
        context.plan.network.chain_id,
        &context.plan.simulation.anvil_args,
        Path::new(&context.plan.project_root),
        &AnvilStartOptions {
            port: Some(input.port),
            accounts: Some(accounts),
            block_time_seconds,
            log_path: Some(log_path.clone()),
        },
    )?;
    let evidence = execute_deployment_simulation(&context, &anvil.rpc_url)?;
    let wallets = anvil_accounts(&anvil.rpc_url)?;
    if wallets.len() != usize::from(accounts) {
        bail!(
            "Anvil exposed {} accounts, expected {accounts}",
            wallets.len()
        );
    }
    let pid = anvil.pid()?;
    let (marker_address, marker_code) = marker(&context.plan.digest, pid, &created_at)?;
    install_marker(&anvil.rpc_url, &marker_address, &marker_code)?;
    let mut state = DevnetState {
        schema_version: "v4hook.devnet-state.v1".to_owned(),
        created_at,
        pid,
        port: input.port,
        rpc_url: anvil.rpc_url.clone(),
        log_path: log_path.to_string_lossy().into_owned(),
        plan_path: context.plan_file.to_string_lossy().into_owned(),
        plan_digest: context.plan.digest.clone(),
        project_root: context.plan.project_root.clone(),
        chain_id: context.plan.network.chain_id,
        fork_block_number: context.plan.network.fork_block_number,
        fork_block_hash: context.plan.network.fork_block_hash.clone(),
        hook_address: context.plan.hook.predicted_address.clone(),
        deployed_runtime_code_hash: evidence.deployed_runtime_code_hash,
        marker_address,
        marker_code,
        accounts: wallets,
        manifest_path: manifest_file.to_string_lossy().into_owned(),
        digest: String::new(),
    };
    state.digest = calculate_digest(&state)?;
    let status = validate_live_state(&state)?;
    let manifest = build_manifest(&state)?;
    write_json(&state_file, &state)?;
    if let Err(error) = write_json(&manifest_file, &manifest) {
        let _ = fs::remove_file(&state_file);
        return Err(error);
    }
    anvil.detach()?;
    Ok(status)
}

pub fn status(state_file: &Path) -> Result<DevnetStatus> {
    validate_live_state(&read_state(state_file)?)
}

pub fn reset(state_file: &Path) -> Result<DevnetStatus> {
    let state_file = absolute_output(state_file)?;
    let mut state = read_state(&state_file)?;
    require_owned_process(&state)?;
    let context = prepare_deployment_simulation(&state.plan_path)?;
    if context.plan.digest != state.plan_digest {
        bail!("devnet state references a different deployment plan");
    }
    reset_fork(&state.rpc_url, &context.target_rpc, state.fork_block_number)?;
    let evidence = execute_deployment_simulation(&context, &state.rpc_url)?;
    install_marker(&state.rpc_url, &state.marker_address, &state.marker_code)?;
    state.deployed_runtime_code_hash = evidence.deployed_runtime_code_hash;
    if anvil_accounts(&state.rpc_url)? != state.accounts {
        bail!("Anvil account set changed after reset");
    }
    state.digest = calculate_digest(&state)?;
    write_json(&state_file, &state)?;
    write_manifest(&state, Path::new(&state.manifest_path))?;
    validate_live_state(&state)
}

pub fn export(state_file: &Path, output: &Path) -> Result<DevnetManifest> {
    let state = read_state(state_file)?;
    validate_live_state(&state)?;
    write_manifest(&state, output)
}

pub fn run_scenario(input: &DevnetScenarioInput<'_>) -> Result<DevnetScenarioEvidence> {
    let state = read_state(input.state_file)?;
    validate_live_state(&state)?;
    let plan = read_deployment_plan(&state.plan_path)?;
    let config_raw = fs::read_to_string(&plan.config_path)
        .with_context(|| format!("read {}", plan.config_path))?;
    if sha256_bytes(config_raw) != plan.config_digest {
        bail!("configuration changed after the deployment plan was created");
    }
    let config = load_config(&plan.config_path)?;
    let scenario = config
        .value
        .devnet
        .as_ref()
        .and_then(|devnet| {
            devnet
                .scenarios
                .iter()
                .find(|scenario| scenario.name == input.scenario)
        })
        .with_context(|| format!("unknown devnet scenario: {}", input.scenario))?;
    let variables = BTreeMap::from([
        ("devnetRpc".to_owned(), state.rpc_url.clone()),
        ("devnetManifest".to_owned(), state.manifest_path.clone()),
        ("hookAddress".to_owned(), state.hook_address.clone()),
        ("projectRoot".to_owned(), state.project_root.clone()),
        ("seed".to_owned(), input.seed.to_string()),
        ("walletCount".to_owned(), state.accounts.len().to_string()),
    ]);
    let command = scenario
        .command
        .iter()
        .map(|part| interpolate(part, &variables))
        .collect::<Result<Vec<_>>>()?;
    let environment = BTreeMap::from([
        ("V4HOOK_DEVNET_RPC_URL".to_owned(), state.rpc_url.clone()),
        (
            "V4HOOK_DEVNET_MANIFEST".to_owned(),
            state.manifest_path.clone(),
        ),
        ("V4HOOK_HOOK_ADDRESS".to_owned(), state.hook_address.clone()),
        ("V4HOOK_SCENARIO_SEED".to_owned(), input.seed.to_string()),
        (
            "V4HOOK_DEVNET_WALLET_COUNT".to_owned(),
            state.accounts.len().to_string(),
        ),
    ]);
    let start_block = block_number(&state.rpc_url)?;
    report_status(&format!(
        "Running devnet scenario {} with seed {}...",
        scenario.name, input.seed
    ));
    let result = run(&command, &state.project_root, Some(&environment), false)?;
    let end_block = block_number(&state.rpc_url).ok();
    let integrity_passed = validate_live_state(&state).is_ok();
    let mut evidence = DevnetScenarioEvidence {
        schema_version: "v4hook.devnet-scenario-evidence.v1".to_owned(),
        created_at: now_iso(),
        plan_digest: state.plan_digest,
        scenario: scenario.name.clone(),
        seed: input.seed,
        accounts: state.accounts.len(),
        start_block,
        end_block,
        command: redact_command(&command),
        exit_code: result.exit_code,
        duration_ms: result.duration_ms,
        stdout_hash: sha256_bytes(result.stdout),
        stderr_hash: sha256_bytes(result.stderr),
        integrity_passed,
        passed: result.exit_code == 0 && end_block.is_some() && integrity_passed,
        digest: String::new(),
    };
    evidence.digest = calculate_digest(&evidence)?;
    write_json(input.output, &evidence)?;
    if !evidence.passed {
        let reason = if result.exit_code != 0 {
            format!("command exited with code {}", result.exit_code)
        } else {
            "the devnet became unavailable or failed its post-run integrity check".to_owned()
        };
        bail!(
            "devnet scenario {} failed because {reason}; evidence: {}",
            evidence.scenario,
            input.output.display()
        );
    }
    Ok(evidence)
}

pub fn down(state_file: &Path) -> Result<bool> {
    let state_file = absolute_output(state_file)?;
    let state = read_state(&state_file)?;
    let Some(command) = process_command(state.pid)? else {
        remove_generated_state(&state_file)?;
        return Ok(false);
    };
    if !command_matches_anvil(&command, state.port, state.chain_id) {
        bail!(
            "refusing to stop PID {}; it is not the Anvil process recorded by this devnet",
            state.pid
        );
    }
    if chain_id(&state.rpc_url)? != state.chain_id
        || code_at(&state.rpc_url, &state.marker_address)? != state.marker_code
    {
        bail!("refusing to stop Anvil because the recorded devnet identity no longer matches");
    }
    let status = Command::new("kill")
        .args(["-TERM", &state.pid.to_string()])
        .status()
        .context("stop devnet Anvil")?;
    if !status.success() {
        bail!("failed to stop devnet Anvil PID {}", state.pid);
    }
    for _ in 0..50 {
        if process_command(state.pid)?.is_none() {
            remove_generated_state(&state_file)?;
            return Ok(true);
        }
        thread::sleep(Duration::from_millis(100));
    }
    bail!("devnet Anvil PID {} did not stop after SIGTERM", state.pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_manifest() -> DevnetManifest {
        let mut manifest = DevnetManifest {
            schema_version: "v4hook.devnet-manifest.v1".to_owned(),
            created_at: "2026-01-01T00:00:00.000Z".to_owned(),
            warning: "local only".to_owned(),
            rpc_url: "http://127.0.0.1:8545".to_owned(),
            chain_id: 31_337,
            fork_block_number: 1,
            fork_block_hash: "0x00".to_owned(),
            plan_digest: "sha256:plan".to_owned(),
            hook: DevnetHookManifest {
                address: "0x0000000000000000000000000000000000000001".to_owned(),
                abi: serde_json::json!([]),
            },
            contracts: BTreeMap::new(),
            accounts: Vec::new(),
            pool: None,
            scenarios: Vec::new(),
            digest: String::new(),
        };
        manifest.digest = calculate_digest(&manifest).unwrap();
        manifest
    }

    #[test]
    fn matches_only_the_recorded_anvil_process() {
        assert!(command_matches_anvil(
            "/Users/test/.foundry/bin/anvil --host 127.0.0.1 --port 8545 --chain-id 4663",
            8545,
            4663
        ));
        assert!(!command_matches_anvil(
            "node server.js --port 8545 --chain-id 4663",
            8545,
            4663
        ));
        assert!(!command_matches_anvil(
            "anvil --port 9545 --chain-id 4663",
            8545,
            4663
        ));
    }

    #[test]
    fn marker_is_deterministic_and_address_sized() {
        let (address, code) = marker("sha256:abc", 123, "2026-01-01T00:00:00Z").unwrap();
        assert_eq!(address.len(), 42);
        assert!(code.starts_with("0x7f"));
        assert_eq!(
            marker("sha256:abc", 123, "2026-01-01T00:00:00Z").unwrap(),
            (address, code)
        );
    }

    #[test]
    fn only_replaces_digest_valid_devnet_manifests() {
        let path = std::env::temp_dir().join(format!(
            "v4hook-devnet-manifest-test-{}.json",
            std::process::id()
        ));
        write_json(&path, &sample_manifest()).unwrap();
        require_generated_manifest(&path).unwrap();
        fs::write(&path, "{}\n").unwrap();
        assert!(require_generated_manifest(&path).is_err());
        fs::remove_file(path).unwrap();
    }
}
