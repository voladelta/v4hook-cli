use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::{
    anvil::{AnvilStartOptions, start_anvil_with_options},
    artifact::code_hash,
    config::load_config,
    model::{
        DevnetDownResult, DevnetHookManifest, DevnetManifest, DevnetPoolManifest,
        DevnetReservedAccountEvidence, DevnetScenarioAssertion, DevnetScenarioEvidence,
        DevnetScenarioReport, DevnetScenarioVerification, DevnetScenarioVerificationEvidence,
        DevnetState, DevnetStatus, DevnetTransactionEvidence,
    },
    plan::{absolute_path, read_deployment_plan},
    process::{redact_command, run},
    rpc::{
        anvil_accounts, block_hash, block_number, chain_id, code_at, reset_fork, rpc_json,
        set_anvil_code,
    },
    simulate::{
        DeploymentSimulationContext, execute_deployment_simulation, prepare_deployment_simulation,
    },
    util::{
        assert_digest, calculate_digest, interpolate, normalize_address, normalize_hex, now_iso,
        read_json, sha256_bytes, sha256_file, status as report_status, write_json,
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

#[derive(Clone)]
struct AccountSnapshot {
    index: u16,
    address: String,
    nonce: String,
    balance: String,
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
        .any(|pair| (pair[0] == long || pair[0] == short) && pair[1] == value)
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

fn read_generated_manifest(path: &Path) -> Result<DevnetManifest> {
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
    assert_digest(&manifest, &manifest.digest, "existing devnet manifest")?;
    Ok(manifest)
}

fn require_generated_manifest(path: &Path) -> Result<()> {
    read_generated_manifest(path).map(|_| ())
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
            persistent: true,
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

fn rpc_quantity(value: &Value, label: &str) -> Result<String> {
    let value = value
        .as_str()
        .with_context(|| format!("{label} is not an RPC quantity"))?;
    let body = value
        .strip_prefix("0x")
        .with_context(|| format!("{label} is missing its 0x prefix"))?;
    if body.is_empty() || !body.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("{label} is not a hexadecimal RPC quantity");
    }
    Ok(format!("0x{}", body.to_ascii_lowercase()))
}

fn quantity_u64(value: &Value, label: &str) -> Result<u64> {
    let value = rpc_quantity(value, label)?;
    u64::from_str_radix(&value[2..], 16).with_context(|| format!("parse {label}"))
}

fn account_snapshot(state: &DevnetState, index: u16) -> Result<AccountSnapshot> {
    let address = state
        .accounts
        .get(usize::from(index))
        .with_context(|| format!("reserved account index {index} is unavailable"))?
        .clone();
    Ok(AccountSnapshot {
        index,
        address: address.clone(),
        nonce: rpc_quantity(
            &rpc_json(
                &state.rpc_url,
                "eth_getTransactionCount",
                &[json!(address), json!("latest")],
            )?,
            "account nonce",
        )?,
        balance: rpc_quantity(
            &rpc_json(
                &state.rpc_url,
                "eth_getBalance",
                &[json!(address), json!("latest")],
            )?,
            "account balance",
        )?,
    })
}

fn block_managed_transactions(
    state: &DevnetState,
    start_block: u64,
    end_block: u64,
) -> Result<BTreeSet<String>> {
    let accounts = state.accounts.iter().cloned().collect::<BTreeSet<_>>();
    let mut hashes = BTreeSet::new();
    for number in start_block.saturating_add(1)..=end_block {
        let block = rpc_json(
            &state.rpc_url,
            "eth_getBlockByNumber",
            &[json!(format!("0x{number:x}")), json!(true)],
        )?;
        let transactions = block
            .get("transactions")
            .and_then(Value::as_array)
            .with_context(|| format!("block {number} is missing transactions"))?;
        for transaction in transactions {
            let sender = transaction
                .get("from")
                .and_then(Value::as_str)
                .map(|value| normalize_address(value, "transaction sender"))
                .transpose()?;
            if sender
                .as_ref()
                .is_some_and(|sender| accounts.contains(sender))
            {
                let hash = transaction
                    .get("hash")
                    .and_then(Value::as_str)
                    .context("managed transaction is missing its hash")?;
                let hash = normalize_hex(hash, "transaction hash")?;
                if hash.len() != 66 {
                    bail!("transaction hash must be exactly 32 bytes");
                }
                hashes.insert(hash);
            }
        }
    }
    Ok(hashes)
}

fn resolved_address(value: &str, hook: &str) -> String {
    if value == "hook" {
        hook.to_owned()
    } else {
        value.to_owned()
    }
}

fn failed_verification(
    policy: &DevnetScenarioVerification,
    issue: impl Into<String>,
) -> DevnetScenarioVerificationEvidence {
    DevnetScenarioVerificationEvidence {
        expected_transactions: policy.expected_transactions,
        observed_transactions: 0,
        expected_senders: policy.expected_senders,
        observed_senders: 0,
        assertions: vec![DevnetScenarioAssertion {
            name: "verification-executed".to_owned(),
            passed: false,
        }],
        transactions: Vec::new(),
        reserved_accounts: Vec::new(),
        issues: vec![issue.into()],
        passed: false,
    }
}

struct ScenarioPolicyContext {
    managed_accounts: BTreeSet<String>,
    reserved_addresses: BTreeSet<String>,
    allowed_targets: BTreeSet<String>,
    required_events: Vec<(String, String)>,
}

struct CheckedTransaction {
    evidence: DevnetTransactionEvidence,
    events_present: bool,
    issues: Vec<String>,
}

fn read_scenario_report(path: &Path) -> Result<(BTreeSet<String>, Vec<String>)> {
    let report: DevnetScenarioReport = read_json(path)?;
    if report.schema_version != "v4hook.devnet-scenario-report.v1" {
        bail!(
            "unsupported devnet scenario report schemaVersion: {}",
            report.schema_version
        );
    }
    let mut hashes = BTreeSet::new();
    let mut issues = Vec::new();
    for hash in report.transactions {
        let hash = normalize_hex(&hash, "scenario transaction hash")?;
        if hash.len() != 66 {
            bail!("scenario transaction hash must be exactly 32 bytes");
        }
        if !hashes.insert(hash) {
            issues.push("scenario report contains a duplicate transaction hash".to_owned());
        }
    }
    Ok((hashes, issues))
}

fn scenario_policy_context(
    state: &DevnetState,
    policy: &DevnetScenarioVerification,
    before: &[AccountSnapshot],
) -> ScenarioPolicyContext {
    ScenarioPolicyContext {
        managed_accounts: state.accounts.iter().cloned().collect(),
        reserved_addresses: before.iter().map(|item| item.address.clone()).collect(),
        allowed_targets: policy
            .allowed_targets
            .iter()
            .map(|value| resolved_address(value, &state.hook_address))
            .collect(),
        required_events: policy
            .required_events
            .iter()
            .map(|event| {
                (
                    resolved_address(&event.address, &state.hook_address),
                    event.topic0.clone(),
                )
            })
            .collect(),
    }
}

fn log_matches_event(log: &Value, address: &str, topic0: &str) -> bool {
    let actual_address = log
        .get("address")
        .and_then(Value::as_str)
        .and_then(|value| normalize_address(value, "log address").ok());
    let actual_topic = log
        .get("topics")
        .and_then(Value::as_array)
        .and_then(|topics| topics.first())
        .and_then(Value::as_str)
        .and_then(|value| normalize_hex(value, "log topic").ok());
    actual_address.as_deref() == Some(address) && actual_topic.as_deref() == Some(topic0)
}

fn check_transaction(
    state: &DevnetState,
    policy: &ScenarioPolicyContext,
    hash: &str,
    start_block: u64,
    end_block: u64,
) -> Result<CheckedTransaction> {
    let transaction = rpc_json(&state.rpc_url, "eth_getTransactionByHash", &[json!(hash)])?;
    let receipt = rpc_json(&state.rpc_url, "eth_getTransactionReceipt", &[json!(hash)])?;
    if transaction.is_null() || receipt.is_null() {
        bail!("transaction or receipt is unavailable: {hash}");
    }
    let sender = normalize_address(
        transaction
            .get("from")
            .and_then(Value::as_str)
            .context("transaction is missing from")?,
        "transaction sender",
    )?;
    let target = normalize_address(
        transaction
            .get("to")
            .and_then(Value::as_str)
            .context("scenario transactions cannot create contracts")?,
        "transaction target",
    )?;
    let block_number = quantity_u64(
        receipt
            .get("blockNumber")
            .context("receipt is missing blockNumber")?,
        "receipt blockNumber",
    )?;
    let mut issues = Vec::new();
    if !policy.managed_accounts.contains(&sender) {
        issues.push(format!(
            "transaction sender is not a generated account: {sender}"
        ));
    }
    if policy.reserved_addresses.contains(&sender) {
        issues.push(format!(
            "reserved account sent a scenario transaction: {sender}"
        ));
    }
    if !policy.allowed_targets.contains(&target) {
        issues.push(format!("transaction target is not allowed: {target}"));
    }
    if rpc_quantity(
        receipt.get("status").context("receipt is missing status")?,
        "receipt status",
    )? != "0x1"
    {
        issues.push(format!("transaction reverted: {hash}"));
    }
    if block_number <= start_block || block_number > end_block {
        issues.push(format!(
            "transaction is outside the scenario block range: {hash}"
        ));
    }
    let logs = receipt
        .get("logs")
        .and_then(Value::as_array)
        .context("receipt is missing logs")?;
    let events_present = policy.required_events.iter().all(|(address, topic)| {
        let present = logs
            .iter()
            .any(|log| log_matches_event(log, address, topic));
        if !present {
            issues.push(format!(
                "transaction {hash} is missing required event {topic} from {address}"
            ));
        }
        present
    });
    Ok(CheckedTransaction {
        evidence: DevnetTransactionEvidence {
            hash: hash.to_owned(),
            sender,
            target,
            block_number,
            gas_used: rpc_quantity(
                receipt
                    .get("gasUsed")
                    .context("receipt is missing gasUsed")?,
                "receipt gasUsed",
            )?,
        },
        events_present,
        issues,
    })
}

fn verify_reserved_accounts(
    state: &DevnetState,
    before: &[AccountSnapshot],
) -> Result<(Vec<DevnetReservedAccountEvidence>, Vec<String>)> {
    let mut evidence = Vec::new();
    let mut issues = Vec::new();
    for snapshot in before {
        let after = account_snapshot(state, snapshot.index)?;
        let unchanged = snapshot.nonce == after.nonce && snapshot.balance == after.balance;
        if !unchanged {
            issues.push(format!(
                "reserved account {} changed nonce or native balance",
                snapshot.address
            ));
        }
        evidence.push(DevnetReservedAccountEvidence {
            index: snapshot.index,
            address: snapshot.address.clone(),
            nonce_before: snapshot.nonce.clone(),
            nonce_after: after.nonce,
            balance_before: snapshot.balance.clone(),
            balance_after: after.balance,
            unchanged,
        });
    }
    Ok((evidence, issues))
}

fn verify_scenario_report(
    state: &DevnetState,
    policy: &DevnetScenarioVerification,
    report_path: &Path,
    start_block: u64,
    end_block: u64,
    before: &[AccountSnapshot],
) -> Result<DevnetScenarioVerificationEvidence> {
    let (reported, mut issues) = read_scenario_report(report_path)?;
    let managed = block_managed_transactions(state, start_block, end_block)?;
    let managed_transactions_complete = reported == managed;
    if !managed_transactions_complete {
        issues.push(format!(
            "scenario report covers {} hashes but the block range contains {} managed-account transactions",
            reported.len(),
            managed.len()
        ));
    }
    if u64::try_from(reported.len()).unwrap_or(u64::MAX) != policy.expected_transactions {
        issues.push(format!(
            "expected {} transactions, observed {}",
            policy.expected_transactions,
            reported.len()
        ));
    }
    let context = scenario_policy_context(state, policy, before);
    let mut senders = BTreeSet::new();
    let mut transactions = Vec::new();
    let mut required_events_present = true;
    for hash in &reported {
        let checked = check_transaction(state, &context, hash, start_block, end_block)?;
        senders.insert(checked.evidence.sender.clone());
        required_events_present &= checked.events_present;
        issues.extend(checked.issues);
        transactions.push(checked.evidence);
    }
    let observed_senders = u64::try_from(senders.len()).unwrap_or(u64::MAX);
    if observed_senders != policy.expected_senders {
        issues.push(format!(
            "expected {} unique senders, observed {observed_senders}",
            policy.expected_senders
        ));
    }
    let (reserved_accounts, reserved_issues) = verify_reserved_accounts(state, before)?;
    issues.extend(reserved_issues);
    let reserved_accounts_unchanged = reserved_accounts.iter().all(|account| account.unchanged);
    transactions.sort_by(|left, right| left.hash.cmp(&right.hash));
    let passed = issues.is_empty();
    Ok(DevnetScenarioVerificationEvidence {
        expected_transactions: policy.expected_transactions,
        observed_transactions: u64::try_from(reported.len()).unwrap_or(u64::MAX),
        expected_senders: policy.expected_senders,
        observed_senders,
        assertions: vec![
            DevnetScenarioAssertion {
                name: "managed-transactions-complete".to_owned(),
                passed: managed_transactions_complete,
            },
            DevnetScenarioAssertion {
                name: "required-events-present".to_owned(),
                passed: required_events_present,
            },
            DevnetScenarioAssertion {
                name: "reserved-accounts-unchanged".to_owned(),
                passed: reserved_accounts_unchanged,
            },
        ],
        transactions,
        reserved_accounts,
        issues,
        passed,
    })
}

struct PreparedScenario {
    state: DevnetState,
    scenario: crate::model::DevnetScenario,
    report_path: PathBuf,
    before: Vec<AccountSnapshot>,
    command: Vec<String>,
    environment: BTreeMap<String, String>,
}

fn prepare_scenario(input: &DevnetScenarioInput<'_>) -> Result<PreparedScenario> {
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
    let scenario = scenario.clone();
    let report_path = absolute_output(
        &input
            .output
            .with_extension(format!("scenario-report-{}.json", std::process::id())),
    )?;
    if report_path.exists() {
        bail!(
            "refusing to replace stale scenario report: {}",
            report_path.display()
        );
    }
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create scenario report directory {}", parent.display()))?;
    }
    let before = scenario
        .verification
        .reserved_account_indices
        .iter()
        .map(|index| account_snapshot(&state, *index))
        .collect::<Result<Vec<_>>>()?;
    let variables = BTreeMap::from([
        ("devnetRpc".to_owned(), state.rpc_url.clone()),
        ("devnetManifest".to_owned(), state.manifest_path.clone()),
        ("hookAddress".to_owned(), state.hook_address.clone()),
        ("projectRoot".to_owned(), state.project_root.clone()),
        ("seed".to_owned(), input.seed.to_string()),
        ("walletCount".to_owned(), state.accounts.len().to_string()),
        (
            "scenarioReport".to_owned(),
            report_path.to_string_lossy().into_owned(),
        ),
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
        (
            "V4HOOK_SCENARIO_REPORT".to_owned(),
            report_path.to_string_lossy().into_owned(),
        ),
    ]);
    Ok(PreparedScenario {
        state,
        scenario,
        report_path,
        before,
        command,
        environment,
    })
}

fn remove_scenario_report(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path)
            .with_context(|| format!("remove temporary scenario report {}", path.display()))?;
    }
    Ok(())
}

pub fn run_scenario(input: &DevnetScenarioInput<'_>) -> Result<DevnetScenarioEvidence> {
    let PreparedScenario {
        state,
        scenario,
        report_path,
        before,
        command,
        environment,
    } = prepare_scenario(input)?;
    let start_block = block_number(&state.rpc_url)?;
    report_status(&format!(
        "Running devnet scenario {} with seed {}...",
        scenario.name, input.seed
    ));
    let result = run(&command, &state.project_root, Some(&environment), false)?;
    let end_block = block_number(&state.rpc_url).ok();
    let integrity_passed = validate_live_state(&state).is_ok();
    let verification = if let Some(end_block) = end_block {
        match verify_scenario_report(
            &state,
            &scenario.verification,
            &report_path,
            start_block,
            end_block,
            &before,
        ) {
            Ok(evidence) => evidence,
            Err(error) => failed_verification(
                &scenario.verification,
                format!("scenario report verification failed: {error:#}"),
            ),
        }
    } else {
        failed_verification(
            &scenario.verification,
            "the end block could not be read after the scenario",
        )
    };
    remove_scenario_report(&report_path)?;
    let mut evidence = DevnetScenarioEvidence {
        schema_version: "v4hook.devnet-scenario-evidence.v2".to_owned(),
        created_at: now_iso(),
        plan_digest: state.plan_digest,
        scenario: scenario.name,
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
        passed: result.exit_code == 0
            && end_block.is_some()
            && integrity_passed
            && verification.passed,
        verification,
        digest: String::new(),
    };
    evidence.digest = calculate_digest(&evidence)?;
    write_json(input.output, &evidence)?;
    if !evidence.passed {
        let reason = if result.exit_code != 0 {
            format!("command exited with code {}", result.exit_code)
        } else if !evidence.verification.passed {
            "independent transaction verification failed".to_owned()
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

fn regular_generated_file_exists(path: &Path, label: &str) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect generated {label} {}", path.display()))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        bail!(
            "refusing to remove non-regular generated {label}: {}",
            path.display()
        );
    }
    Ok(true)
}

fn remove_regular_generated_file(path: &Path, label: &str) -> Result<bool> {
    if !regular_generated_file_exists(path, label)? {
        return Ok(false);
    }
    fs::remove_file(path)
        .with_context(|| format!("remove generated {label} {}", path.display()))?;
    Ok(true)
}

fn purge_generated_artifacts(state_file: &Path, state: &DevnetState) -> Result<Vec<String>> {
    let manifest_path = Path::new(&state.manifest_path);
    if manifest_path.exists() {
        let manifest = read_generated_manifest(manifest_path)?;
        if manifest.plan_digest != state.plan_digest
            || manifest.rpc_url != state.rpc_url
            || manifest.hook.address != state.hook_address
        {
            bail!("refusing to remove a devnet manifest that does not match the recorded state");
        }
    }
    let log_path = Path::new(&state.log_path);
    let expected_log_directory = state_file.parent().unwrap_or(Path::new(".")).join("devnet");
    let log_name = log_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if log_path.parent() != Some(expected_log_directory.as_path())
        || !log_name.starts_with("anvil-")
        || !Path::new(log_name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("log"))
    {
        bail!("refusing to remove an Anvil log outside the generated devnet directory");
    }
    regular_generated_file_exists(manifest_path, "manifest")?;
    regular_generated_file_exists(log_path, "Anvil log")?;

    let mut removed = Vec::new();
    if remove_regular_generated_file(manifest_path, "manifest")? {
        removed.push(manifest_path.to_string_lossy().into_owned());
    }
    if remove_regular_generated_file(log_path, "Anvil log")? {
        removed.push(log_path.to_string_lossy().into_owned());
    }
    Ok(removed)
}

pub fn down(state_file: &Path, purge_generated: bool) -> Result<DevnetDownResult> {
    let state_file = absolute_output(state_file)?;
    let state = read_state(&state_file)?;
    let Some(command) = process_command(state.pid)? else {
        let removed = if purge_generated {
            purge_generated_artifacts(&state_file, &state)?
        } else {
            Vec::new()
        };
        remove_generated_state(&state_file)?;
        return Ok(DevnetDownResult {
            stopped: false,
            removed,
        });
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
            let removed = if purge_generated {
                purge_generated_artifacts(&state_file, &state)?
            } else {
                Vec::new()
            };
            remove_generated_state(&state_file)?;
            return Ok(DevnetDownResult {
                stopped: true,
                removed,
            });
        }
        thread::sleep(Duration::from_millis(100));
    }
    bail!("devnet Anvil PID {} did not stop after SIGTERM", state.pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestAnvil(std::process::Child);

    impl Drop for TestAnvil {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn start_test_anvil() -> (TestAnvil, String, Vec<String>) {
        use std::{net::TcpListener, process::Stdio};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let child = Command::new("anvil")
            .args([
                "--silent",
                "--host",
                "127.0.0.1",
                "--port",
                &port.to_string(),
                "--chain-id",
                "31337",
                "--accounts",
                "3",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let rpc_url = format!("http://127.0.0.1:{port}");
        crate::rpc::wait_for_rpc(&rpc_url, Duration::from_secs(10)).unwrap();
        let accounts = anvil_accounts(&rpc_url).unwrap();
        (TestAnvil(child), rpc_url, accounts)
    }

    fn verification_test_state(rpc_url: &str, accounts: &[String]) -> DevnetState {
        DevnetState {
            schema_version: "v4hook.devnet-state.v1".to_owned(),
            created_at: now_iso(),
            pid: 0,
            port: rpc_url.rsplit(':').next().unwrap().parse().unwrap(),
            rpc_url: rpc_url.to_owned(),
            log_path: String::new(),
            plan_path: String::new(),
            plan_digest: "sha256:test".to_owned(),
            project_root: String::new(),
            chain_id: 31_337,
            fork_block_number: 0,
            fork_block_hash: block_hash(rpc_url, 0).unwrap(),
            hook_address: "0x0000000000000000000000000000000000000001".to_owned(),
            deployed_runtime_code_hash: String::new(),
            marker_address: String::new(),
            marker_code: String::new(),
            accounts: accounts.to_vec(),
            manifest_path: String::new(),
            digest: String::new(),
        }
    }

    fn send_test_transactions(rpc_url: &str, accounts: &[String]) -> Vec<String> {
        (0..2)
            .map(|_| {
                rpc_json(
                    rpc_url,
                    "eth_sendTransaction",
                    &[json!({
                        "from": accounts[1],
                        "to": accounts[2],
                        "value": "0x1"
                    })],
                )
                .unwrap()
                .as_str()
                .unwrap()
                .to_owned()
            })
            .collect()
    }

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

    #[test]
    fn purge_removes_only_state_bound_generated_artifacts() {
        let root =
            std::env::temp_dir().join(format!("v4hook-devnet-purge-test-{}", std::process::id()));
        let devnet_directory = root.join("devnet");
        fs::create_dir_all(&devnet_directory).unwrap();
        let state_path = root.join("state.json");
        let manifest_path = root.join("manifest.json");
        let log_path = devnet_directory.join("anvil-test.log");
        let mut manifest = sample_manifest();
        manifest.plan_digest = "sha256:plan".to_owned();
        manifest.rpc_url = "http://127.0.0.1:8545".to_owned();
        manifest.hook.address = "0x0000000000000000000000000000000000000001".to_owned();
        manifest.digest = calculate_digest(&manifest).unwrap();
        write_json(&manifest_path, &manifest).unwrap();
        fs::write(&log_path, "eth_chainId\n").unwrap();
        let state = DevnetState {
            schema_version: "v4hook.devnet-state.v1".to_owned(),
            created_at: "2026-01-01T00:00:00.000Z".to_owned(),
            pid: 1,
            port: 8545,
            rpc_url: manifest.rpc_url.clone(),
            log_path: log_path.to_string_lossy().into_owned(),
            plan_path: root.join("plan.json").to_string_lossy().into_owned(),
            plan_digest: manifest.plan_digest.clone(),
            project_root: root.to_string_lossy().into_owned(),
            chain_id: 31_337,
            fork_block_number: 1,
            fork_block_hash: "0x00".to_owned(),
            hook_address: manifest.hook.address,
            deployed_runtime_code_hash: "0x00".to_owned(),
            marker_address: "0x0000000000000000000000000000000000000002".to_owned(),
            marker_code: "0x00".to_owned(),
            accounts: Vec::new(),
            manifest_path: manifest_path.to_string_lossy().into_owned(),
            digest: String::new(),
        };
        let mut invalid = state.clone();
        invalid.log_path = root
            .join("not-a-generated-log.txt")
            .to_string_lossy()
            .into_owned();
        assert!(purge_generated_artifacts(&state_path, &invalid).is_err());
        assert!(manifest_path.exists());
        assert!(log_path.exists());

        let removed = purge_generated_artifacts(&state_path, &state).unwrap();
        assert_eq!(removed.len(), 2);
        assert!(!manifest_path.exists());
        assert!(!log_path.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "requires Foundry Anvil and unrestricted localhost sockets"]
    fn independently_verifies_every_reported_devnet_transaction() {
        let (_anvil, rpc_url, accounts) = start_test_anvil();
        assert_eq!(accounts.len(), 3);
        let state = verification_test_state(&rpc_url, &accounts);
        let policy = DevnetScenarioVerification {
            expected_transactions: 2,
            expected_senders: 1,
            allowed_targets: vec![accounts[2].clone()],
            required_events: Vec::new(),
            reserved_account_indices: vec![0],
        };
        let before = vec![account_snapshot(&state, 0).unwrap()];
        let start_block = block_number(&rpc_url).unwrap();
        let hashes = send_test_transactions(&rpc_url, &accounts);
        let end_block = block_number(&rpc_url).unwrap();
        let root = std::env::temp_dir().join(format!(
            "v4hook-scenario-verification-test-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let report_path = root.join("report.json");
        write_json(
            &report_path,
            &DevnetScenarioReport {
                schema_version: "v4hook.devnet-scenario-report.v1".to_owned(),
                transactions: hashes.clone(),
            },
        )
        .unwrap();
        let evidence = verify_scenario_report(
            &state,
            &policy,
            &report_path,
            start_block,
            end_block,
            &before,
        )
        .unwrap();
        assert!(evidence.passed, "{:?}", evidence.issues);
        assert_eq!(evidence.observed_transactions, 2);
        assert_eq!(evidence.observed_senders, 1);
        assert!(evidence.reserved_accounts[0].unchanged);

        write_json(
            &report_path,
            &DevnetScenarioReport {
                schema_version: "v4hook.devnet-scenario-report.v1".to_owned(),
                transactions: vec![hashes[0].clone()],
            },
        )
        .unwrap();
        let incomplete = verify_scenario_report(
            &state,
            &policy,
            &report_path,
            start_block,
            end_block,
            &before,
        )
        .unwrap();
        assert!(!incomplete.passed);
        fs::remove_dir_all(root).unwrap();
    }
}
