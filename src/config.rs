use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::Path,
};

use alloy_primitives::Address;
use anyhow::{Context, Result, bail};
use regex::Regex;

use crate::{
    model::{
        DevnetConfig, LoadedConfig, PoolConfig, SimulationConfig, SimulationKind, SimulationStep,
        V4HookConfig, default_max_init_code_size, default_max_runtime_code_size,
        default_minimum_fuzz_runs, default_minimum_invariant_depth, default_minimum_invariant_runs,
    },
    permissions::{permission_flags, validate_hook_address_fee},
    process::validate_foundry_test_command,
    util::{normalize_address, normalize_hex, parse_unsigned, resolve_from},
};

fn validate_command(command: &[String], label: &str) -> Result<()> {
    if command.is_empty() || command.iter().any(String::is_empty) {
        bail!("{label} must contain at least one non-empty argument");
    }
    Ok(())
}

fn require_steps(steps: &[SimulationStep], required: &[SimulationKind], label: &str) -> Result<()> {
    for kind in required {
        if !steps.iter().any(|step| step.kind == *kind) {
            bail!("{label} is missing required {} step", kind.as_str());
        }
    }
    for step in steps {
        validate_command(&step.command, "simulation command")?;
        if matches!(
            step.kind,
            SimulationKind::Quadrants | SimulationKind::Postconditions
        ) {
            validate_foundry_test_command(
                &step.command,
                &format!("{} simulation step", step.kind.as_str()),
            )?;
        }
    }
    Ok(())
}

fn reject_state_changing_verification(command: &[String]) -> Result<()> {
    let executable = command
        .first()
        .and_then(|value| Path::new(value).file_name())
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let subcommand = command
        .get(1)
        .map_or_else(String::new, |value| value.to_ascii_lowercase());
    let broadcast = command
        .iter()
        .any(|argument| argument.eq_ignore_ascii_case("--broadcast"));
    if broadcast
        || (executable == "cast" && subcommand == "send")
        || (executable == "forge" && subcommand == "create")
    {
        bail!("pool.liveVerify must be read-only; state-changing commands are not allowed");
    }
    Ok(())
}

fn reject_live_rpc_arguments(command: &[String]) -> Result<()> {
    for argument in command {
        let lower = argument.to_ascii_lowercase();
        if matches!(lower.as_str(), "--rpc-url" | "--fork-url")
            || lower.starts_with("--rpc-url=")
            || lower.starts_with("--fork-url=")
            || argument.contains("{rpcUrl}")
            || argument.contains("{anvilRpc}")
        {
            bail!(
                "pool.liveVerify must read the live endpoint from FOUNDRY_ETH_RPC_URL or ETH_RPC_URL; RPC command arguments are not allowed"
            );
        }
    }
    Ok(())
}

fn validate_check_commands(config: &V4HookConfig) -> Result<()> {
    for (label, command) in [
        ("checks.unit", &config.checks.unit),
        ("checks.fuzz", &config.checks.fuzz),
        ("checks.invariant", &config.checks.invariant),
        ("checks.staticAnalysis", &config.checks.static_analysis),
    ] {
        validate_command(command, label)?;
    }
    validate_foundry_test_command(&config.checks.unit, "checks.unit")?;
    validate_foundry_test_command(&config.checks.fuzz, "checks.fuzz")?;
    validate_foundry_test_command(&config.checks.invariant, "checks.invariant")?;
    validate_slither_command(&config.checks.static_analysis)?;
    validate_slither_policy(config)?;
    if !config.checks.gas_snapshot.is_empty() {
        validate_gas_snapshot_command(&config.checks.gas_snapshot)?;
    }
    if config.checks.code_size.max_runtime_bytes == 0
        || config.checks.code_size.max_runtime_bytes > default_max_runtime_code_size()
    {
        bail!(
            "checks.codeSize.maxRuntimeBytes must be between 1 and {}",
            default_max_runtime_code_size()
        );
    }
    if config.checks.code_size.max_init_code_bytes == 0
        || config.checks.code_size.max_init_code_bytes > default_max_init_code_size()
    {
        bail!(
            "checks.codeSize.maxInitCodeBytes must be between 1 and {}",
            default_max_init_code_size()
        );
    }
    if config.checks.minimum_fuzz_runs < default_minimum_fuzz_runs() {
        bail!(
            "checks.minimumFuzzRuns must be at least {}",
            default_minimum_fuzz_runs()
        );
    }
    if config.checks.minimum_invariant_runs < default_minimum_invariant_runs() {
        bail!(
            "checks.minimumInvariantRuns must be at least {}",
            default_minimum_invariant_runs()
        );
    }
    if config.checks.minimum_invariant_depth < default_minimum_invariant_depth() {
        bail!(
            "checks.minimumInvariantDepth must be at least {}",
            default_minimum_invariant_depth()
        );
    }
    Ok(())
}

fn executable_name(argument: &str) -> &str {
    Path::new(argument)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
}

fn validate_slither_command(command: &[String]) -> Result<()> {
    if !command
        .iter()
        .any(|argument| executable_name(argument) == "slither")
    {
        bail!("checks.staticAnalysis must invoke slither");
    }
    for argument in command {
        let lower = argument.to_ascii_lowercase();
        if lower == "--exclude"
            || (lower.starts_with("--exclude-") && lower != "--exclude-dependencies")
            || lower == "--include-detectors"
            || lower == "--filter-paths"
            || lower.starts_with("--filter-paths=")
            || lower == "--json"
            || lower.starts_with("--json=")
            || lower == "--sarif"
            || lower.starts_with("--sarif=")
            || lower.starts_with("--fail-")
            || lower == "--no-fail-pedantic"
        {
            bail!(
                "checks.staticAnalysis cannot contain {argument}; v4hook owns detector scope, JSON output, and failure policy"
            );
        }
    }
    Ok(())
}

fn validate_slither_policy(config: &V4HookConfig) -> Result<()> {
    let policy = &config.checks.slither_policy;
    if policy.require_triage_on > policy.fail_on {
        bail!("checks.slitherPolicy.requireTriageOn cannot exceed failOn");
    }
    let fingerprint = Regex::new(r"^sha256:[0-9a-f]{64}$").expect("static regex");
    let dependency_path = Regex::new(r"^[A-Za-z0-9._/-]+/$").expect("static regex");
    let mut dependency_paths = BTreeSet::new();
    for path in &policy.dependency_paths {
        if !dependency_path.is_match(path)
            || Path::new(path).is_absolute()
            || path.split('/').any(|part| part == "..")
        {
            bail!("Slither dependency paths must be safe relative directories ending in /");
        }
        if !dependency_paths.insert(path) {
            bail!("duplicate Slither dependency path: {path}");
        }
    }
    let mut fingerprints = BTreeSet::new();
    for allowance in &policy.allowed_findings {
        if !fingerprint.is_match(&allowance.fingerprint) {
            bail!("Slither allowance fingerprints must be lowercase sha256 digests");
        }
        if allowance.reason.trim().is_empty() {
            bail!("every Slither allowance requires a non-empty reason");
        }
        if !fingerprints.insert(&allowance.fingerprint) {
            bail!(
                "duplicate Slither allowance fingerprint: {}",
                allowance.fingerprint
            );
        }
    }
    Ok(())
}

fn validate_gas_snapshot_command(command: &[String]) -> Result<()> {
    if executable_name(command.first().map_or("", String::as_str)) != "forge"
        || command.get(1).map(String::as_str) != Some("snapshot")
        || !command
            .iter()
            .any(|argument| argument == "--check" || argument.starts_with("--check="))
    {
        bail!("checks.gasSnapshot must run `forge snapshot --check`");
    }
    if command.iter().any(|argument| {
        matches!(argument.as_str(), "--allow-failure" | "--diff" | "--snap")
            || argument.starts_with("--diff=")
            || argument.starts_with("--snap=")
    }) {
        bail!("checks.gasSnapshot cannot write snapshots, diff without failure, or allow failure");
    }
    Ok(())
}

fn normalize_authorities(authorities: &mut BTreeMap<String, String>, label: &str) -> Result<()> {
    for (role, address) in authorities {
        if role.trim().is_empty() {
            bail!("{label} authority role names cannot be empty");
        }
        *address = normalize_address(address, &format!("{label}.{role}"))?;
    }
    Ok(())
}

fn validate_simulation(simulation: &mut SimulationConfig) -> Result<()> {
    if simulation.max_fork_block_drift == 0 {
        bail!("simulation.maxForkBlockDrift must be positive");
    }
    for (index, step) in simulation.steps.iter_mut().enumerate() {
        normalize_authorities(
            &mut step.required_authorities,
            &format!("simulation.steps[{index}].requiredAuthorities"),
        )?;
    }
    require_steps(
        &simulation.steps,
        &[
            SimulationKind::Deploy,
            SimulationKind::Pool,
            SimulationKind::Quadrants,
            SimulationKind::Postconditions,
        ],
        "simulation",
    )
}

fn normalize_scenario_address(value: &mut String, label: &str) -> Result<()> {
    if value != "hook" {
        *value = normalize_address(value, label)?;
    }
    Ok(())
}

fn validate_devnet(devnet: &mut DevnetConfig) -> Result<()> {
    if devnet.accounts == 0 || devnet.accounts > 1_000 {
        bail!("devnet.accounts must be between 1 and 1000");
    }
    if devnet.block_time_seconds == Some(0) {
        bail!("devnet.blockTimeSeconds must be positive when configured");
    }
    let name = Regex::new(r"^[a-z][a-z0-9_-]*$").expect("static regex");
    let mut names = BTreeSet::new();
    for scenario in &mut devnet.scenarios {
        if !name.is_match(&scenario.name) {
            bail!(
                "devnet scenario names must start with a lowercase letter and contain only lowercase letters, digits, '-' or '_'"
            );
        }
        if !names.insert(&scenario.name) {
            bail!("duplicate devnet scenario name: {}", scenario.name);
        }
        validate_command(
            &scenario.command,
            &format!("devnet scenario {} command", scenario.name),
        )?;
        let verification = &mut scenario.verification;
        if verification.expected_transactions == 0 || verification.expected_senders == 0 {
            bail!("devnet scenario verification counts must be positive");
        }
        if verification.expected_senders > verification.expected_transactions {
            bail!("devnet scenario expectedSenders cannot exceed expectedTransactions");
        }
        if verification.expected_senders > u64::from(devnet.accounts) {
            bail!("devnet scenario expectedSenders exceeds the generated account count");
        }
        if verification.allowed_targets.is_empty() {
            bail!("devnet scenario allowedTargets cannot be empty");
        }
        for (index, target) in verification.allowed_targets.iter_mut().enumerate() {
            normalize_scenario_address(
                target,
                &format!("devnet scenario {} allowedTargets[{index}]", scenario.name),
            )?;
        }
        if verification
            .allowed_targets
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            != verification.allowed_targets.len()
        {
            bail!("devnet scenario allowedTargets must be unique");
        }
        for (index, event) in verification.required_events.iter_mut().enumerate() {
            normalize_scenario_address(
                &mut event.address,
                &format!(
                    "devnet scenario {} requiredEvents[{index}].address",
                    scenario.name
                ),
            )?;
            event.topic0 = normalize_hex(
                &event.topic0,
                &format!(
                    "devnet scenario {} requiredEvents[{index}].topic0",
                    scenario.name
                ),
            )?;
            if event.topic0.len() != 66 {
                bail!("devnet required event topic0 must be exactly 32 bytes");
            }
        }
        let reserved = verification
            .reserved_account_indices
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if reserved.len() != verification.reserved_account_indices.len() {
            bail!("devnet reservedAccountIndices must be unique");
        }
        if reserved.iter().any(|index| *index >= devnet.accounts) {
            bail!("devnet reserved account index is outside the generated account set");
        }
        let available = u64::from(devnet.accounts) - u64::try_from(reserved.len()).unwrap_or(0);
        if verification.expected_senders > available {
            bail!("devnet scenario expectedSenders includes reserved accounts");
        }
    }
    Ok(())
}

fn validate_pool(pool: &mut PoolConfig, permissions: &[String]) -> Result<()> {
    pool.currency0 = normalize_address(&pool.currency0, "currency0")?;
    pool.currency1 = normalize_address(&pool.currency1, "currency1")?;
    let currency0: Address = pool.currency0.parse()?;
    let currency1: Address = pool.currency1.parse()?;
    if currency0 >= currency1 {
        bail!("pool currencies must be sorted so currency0 < currency1");
    }
    if !(1..=32_767).contains(&pool.tick_spacing) {
        bail!("tickSpacing must be between 1 and 32767");
    }
    if pool.tick_lower < -887_272 || pool.tick_upper > 887_272 || pool.tick_lower >= pool.tick_upper
    {
        bail!("pool tick range is invalid");
    }
    if pool.tick_lower % pool.tick_spacing != 0 || pool.tick_upper % pool.tick_spacing != 0 {
        bail!("pool ticks must be multiples of tickSpacing");
    }
    parse_unsigned(&pool.sqrt_price_x96, "sqrtPriceX96", 256)?;
    parse_unsigned(&pool.liquidity, "liquidity", 256)?;
    parse_unsigned(&pool.amount0_max, "amount0Max", 256)?;
    parse_unsigned(&pool.amount1_max, "amount1Max", 256)?;
    pool.recipient = normalize_address(&pool.recipient, "recipient")?;
    pool.hook_data = normalize_hex(&pool.hook_data, "hookData")?;
    normalize_authorities(&mut pool.launch_authorities, "pool.launchAuthorities")?;
    for (index, step) in pool.simulation_steps.iter_mut().enumerate() {
        normalize_authorities(
            &mut step.required_authorities,
            &format!("pool.simulationSteps[{index}].requiredAuthorities"),
        )?;
    }
    validate_hook_address_fee(permissions, pool.fee)?;
    require_steps(
        &pool.simulation_steps,
        &[
            SimulationKind::Pool,
            SimulationKind::Quadrants,
            SimulationKind::Postconditions,
        ],
        "pool simulation",
    )?;
    validate_command(&pool.live_verify, "pool.liveVerify")?;
    reject_state_changing_verification(&pool.live_verify)?;
    reject_live_rpc_arguments(&pool.live_verify)
}

pub fn load_config(config_file: impl AsRef<Path>) -> Result<LoadedConfig> {
    let path = fs::canonicalize(config_file.as_ref())
        .with_context(|| format!("resolve {}", config_file.as_ref().display()))?;
    let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let mut value: V4HookConfig =
        serde_json::from_str(&raw).with_context(|| format!("invalid config {}", path.display()))?;
    if value.schema_version != "v4hook.config.v1" {
        bail!("unsupported config schemaVersion: {}", value.schema_version);
    }
    if value.network.chain_id == 0 {
        bail!("network.chainId must be positive");
    }
    let env_name = Regex::new(r"^[A-Z][A-Z0-9_]*$").expect("static regex");
    if !env_name.is_match(&value.network.rpc_url_env) {
        bail!("network.rpcUrlEnv must be an uppercase environment variable name");
    }
    for (name, address) in [
        ("poolManager", &mut value.network.pool_manager),
        ("positionManager", &mut value.network.position_manager),
        ("universalRouter", &mut value.network.universal_router),
        ("quoter", &mut value.network.quoter),
        ("stateView", &mut value.network.state_view),
        ("permit2", &mut value.network.permit2),
        ("create2Deployer", &mut value.network.create2_deployer),
    ] {
        *address = normalize_address(address, name)?;
    }
    value.contract.constructor_args =
        normalize_hex(&value.contract.constructor_args, "constructorArgs")?;
    permission_flags(&value.contract.permissions)?;
    validate_check_commands(&value)?;
    validate_simulation(&mut value.simulation)?;
    normalize_authorities(
        &mut value.deployment.required_authorities,
        "deployment.requiredAuthorities",
    )?;
    if let Some(devnet) = &mut value.devnet {
        validate_devnet(devnet)?;
    }
    if value.contract.artifact.is_empty() || value.deployment.script.is_empty() {
        bail!("contract.artifact and deployment.script cannot be empty");
    }
    if let Some(pool) = &mut value.pool {
        validate_pool(pool, &value.contract.permissions)?;
    }
    let directory = path
        .parent()
        .context("config has no parent directory")?
        .to_path_buf();
    let project_root = resolve_from(&directory, &value.project_root);
    Ok(LoadedConfig {
        path: path.to_string_lossy().into_owned(),
        project_root: project_root.to_string_lossy().into_owned(),
        value,
        raw,
    })
}

fn conspicuous_address_sentinel(address: &str, allow_native: bool) -> bool {
    let body = address.strip_prefix("0x").unwrap_or(address);
    let significant = body.trim_start_matches('0');
    if significant.is_empty() {
        return !allow_native;
    }
    significant.len() <= 4
}

fn glob_regex(pattern: &str) -> Regex {
    let mut expression = String::from("^");
    let mut characters = pattern.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '*' if characters.peek() == Some(&'*') => {
                characters.next();
                expression.push_str(".*");
            }
            '*' => expression.push_str("[^/]*"),
            '?' => expression.push_str("[^/]"),
            _ => expression.push_str(&regex::escape(&character.to_string())),
        }
    }
    expression.push('$');
    Regex::new(&expression).expect("escaped glob creates a valid regex")
}

fn any_matching_file(root: &Path, pattern: &str) -> bool {
    let Some(wildcard) = pattern.find(['*', '?']) else {
        return root.join(pattern).is_file();
    };
    let prefix = &pattern[..wildcard];
    let base = prefix
        .rfind('/')
        .map_or_else(|| root.to_path_buf(), |index| root.join(&prefix[..index]));
    let matcher = glob_regex(pattern);
    let mut pending = vec![base];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.is_file()
                && path
                    .strip_prefix(root)
                    .ok()
                    .map(|relative| relative.to_string_lossy().replace('\\', "/"))
                    .is_some_and(|relative| matcher.is_match(&relative))
            {
                return true;
            }
        }
    }
    false
}

fn command_match_path(command: &[String]) -> Option<&str> {
    for (index, argument) in command.iter().enumerate() {
        if argument == "--match-path" {
            return command.get(index + 1).map(String::as_str);
        }
        if let Some(value) = argument.strip_prefix("--match-path=") {
            return Some(value);
        }
    }
    None
}

fn gas_snapshot_path(command: &[String]) -> Option<&str> {
    for (index, argument) in command.iter().enumerate() {
        if argument == "--check" {
            return Some(
                command
                    .get(index + 1)
                    .filter(|value| !value.starts_with('-'))
                    .map_or(".gas-snapshot", String::as_str),
            );
        }
        if let Some(value) = argument.strip_prefix("--check=") {
            return Some(value);
        }
    }
    None
}

fn script_source(target: &str) -> Option<&str> {
    let source = target.split_once(':').map_or(target, |(source, _)| source);
    Path::new(source)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("sol"))
        .then_some(source)
}

fn command_script_source(command: &[String]) -> Option<&str> {
    command
        .iter()
        .skip(2)
        .find_map(|argument| script_source(argument))
}

fn push_command_path_issues(
    issues: &mut Vec<String>,
    project_root: &Path,
    command: &[String],
    label: &str,
) {
    if let Some(pattern) = command_match_path(command)
        && !any_matching_file(project_root, pattern)
    {
        issues.push(format!("{label} --match-path matches no files: {pattern}"));
    }
    if let Some(source) = command_script_source(command)
        && !project_root.join(source).is_file()
    {
        issues.push(format!("{label} script source does not exist: {source}"));
    }
}

fn push_authority_issues(
    issues: &mut Vec<String>,
    authorities: &BTreeMap<String, String>,
    label: &str,
) {
    let distinct = authorities.values().collect::<BTreeSet<_>>();
    if distinct.len() > 1 {
        let roles = authorities.keys().cloned().collect::<Vec<_>>().join(", ");
        issues.push(format!(
            "{label} is one broadcast but requires authorities with different addresses: {roles}"
        ));
    }
    for (role, address) in authorities {
        if conspicuous_address_sentinel(address, false) {
            issues.push(format!(
                "{label} authority {role} is still a sentinel: {address}"
            ));
        }
    }
}

pub fn broadcast_authority(authorities: &BTreeMap<String, String>) -> Result<Option<&str>> {
    let distinct = authorities.values().collect::<BTreeSet<_>>();
    if distinct.len() > 1 {
        bail!("one broadcast cannot use multiple authority addresses");
    }
    Ok(distinct.into_iter().next().map(String::as_str))
}

pub fn require_broadcast_sender(
    authorities: &BTreeMap<String, String>,
    sender: &str,
    label: &str,
) -> Result<()> {
    if let Some(required) = broadcast_authority(authorities)?
        && required != sender
    {
        bail!("{label} requires sender {required}, not {sender}");
    }
    Ok(())
}

pub fn project_readiness_issues(config: &LoadedConfig) -> Vec<String> {
    let project_root = Path::new(&config.project_root);
    let mut issues = Vec::new();
    for (label, address) in [
        ("network.poolManager", &config.value.network.pool_manager),
        (
            "network.positionManager",
            &config.value.network.position_manager,
        ),
        (
            "network.universalRouter",
            &config.value.network.universal_router,
        ),
        ("network.quoter", &config.value.network.quoter),
        ("network.stateView", &config.value.network.state_view),
        ("network.permit2", &config.value.network.permit2),
        (
            "network.create2Deployer",
            &config.value.network.create2_deployer,
        ),
    ] {
        if conspicuous_address_sentinel(address, false) {
            issues.push(format!("{label} is still a sentinel: {address}"));
        }
    }
    for (label, command) in [
        ("checks.unit", &config.value.checks.unit),
        ("checks.fuzz", &config.value.checks.fuzz),
        ("checks.invariant", &config.value.checks.invariant),
    ] {
        push_command_path_issues(&mut issues, project_root, command, label);
    }
    if config.value.checks.gas_snapshot.is_empty() {
        issues.push("checks.gasSnapshot is required before planning".to_owned());
    } else if let Some(snapshot) = gas_snapshot_path(&config.value.checks.gas_snapshot)
        && !project_root.join(snapshot).is_file()
    {
        issues.push(format!(
            "checks.gasSnapshot file does not exist: {snapshot}"
        ));
    }
    for path in &config.value.checks.slither_policy.dependency_paths {
        if !project_root.join(path).is_dir() {
            issues.push(format!("Slither dependency path does not exist: {path}"));
        }
    }
    if let Some(source) = script_source(&config.value.deployment.script)
        && !project_root.join(source).is_file()
    {
        issues.push(format!("deployment script source does not exist: {source}"));
    }
    push_authority_issues(
        &mut issues,
        &config.value.deployment.required_authorities,
        "deployment.requiredAuthorities",
    );
    for (index, step) in config.value.simulation.steps.iter().enumerate() {
        let label = format!("simulation.steps[{index}]");
        push_command_path_issues(&mut issues, project_root, &step.command, &label);
        push_authority_issues(&mut issues, &step.required_authorities, &label);
    }
    if let Some(pool) = &config.value.pool {
        for (label, address, allow_native) in [
            ("pool.currency0", &pool.currency0, true),
            ("pool.currency1", &pool.currency1, false),
            ("pool.recipient", &pool.recipient, false),
        ] {
            if conspicuous_address_sentinel(address, allow_native) {
                issues.push(format!("{label} is still a sentinel: {address}"));
            }
        }
        for (label, value) in [
            ("pool.liquidity", &pool.liquidity),
            ("pool.amount0Max", &pool.amount0_max),
            ("pool.amount1Max", &pool.amount1_max),
        ] {
            if parse_unsigned(value, label, 256).is_ok_and(|value| value.is_zero()) {
                issues.push(format!("{label} must be positive before planning"));
            }
        }
        if let Some(source) = script_source(&pool.launch_script)
            && !project_root.join(source).is_file()
        {
            issues.push(format!(
                "pool launch script source does not exist: {source}"
            ));
        }
        push_authority_issues(
            &mut issues,
            &pool.launch_authorities,
            "pool.launchAuthorities",
        );
        for (index, step) in pool.simulation_steps.iter().enumerate() {
            let label = format!("pool.simulationSteps[{index}]");
            push_command_path_issues(&mut issues, project_root, &step.command, &label);
            push_authority_issues(&mut issues, &step.required_authorities, &label);
        }
    }
    issues
}

pub fn require_project_readiness(config: &LoadedConfig) -> Result<()> {
    let issues = project_readiness_issues(config);
    if !issues.is_empty() {
        bail!("project configuration is not ready: {}", issues.join("; "));
    }
    Ok(())
}

pub fn rpc_url_from_env(name: &str, project_root: &Path) -> Result<String> {
    let from_environment = match env::var(name) {
        Ok(value) if !value.trim().is_empty() => Some(value),
        Ok(_) | Err(env::VarError::NotPresent) => None,
        Err(env::VarError::NotUnicode(_)) => {
            bail!("RPC setting {name} contains non-Unicode data")
        }
    };
    let value = if let Some(value) = from_environment {
        value
    } else {
        let path = project_root.join(".env");
        let mut configured = None;
        if path.is_file() {
            let entries = dotenvy::from_path_iter(&path).map_err(|error| match error {
                dotenvy::Error::Io(error) => {
                    anyhow::Error::new(error).context(format!("could not read {}", path.display()))
                }
                // Other dotenv errors can contain the credential-bearing source line.
                _ => anyhow::anyhow!("could not read {}", path.display()),
            })?;
            for item in entries {
                let (key, value) = item
                    // A dotenv parse error includes the entire source line, so it
                    // cannot safely remain in the user-visible error chain.
                    .map_err(|_| anyhow::anyhow!("invalid dotenv syntax in {}", path.display()))?;
                if key == name && !value.trim().is_empty() {
                    configured = Some(value);
                    break;
                }
            }
        }
        configured.with_context(|| {
            format!(
                "RPC setting {name} is not set in the environment or {}",
                path.display()
            )
        })?
    };
    let parsed = reqwest::Url::parse(&value)
        .with_context(|| format!("RPC setting {name} must be a valid HTTP(S) URL"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        bail!("RPC setting {name} must be a valid HTTP(S) URL");
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn temporary_directory() -> std::path::PathBuf {
        let path = env::temp_dir().join(format!(
            "v4hook-config-test-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn reads_rpc_from_project_dotenv() {
        let root = temporary_directory();
        fs::write(
            root.join(".env"),
            "V4HOOK_TEST_RPC_URL='https://provider.example/key'\n",
        )
        .unwrap();
        assert_eq!(
            rpc_url_from_env("V4HOOK_TEST_RPC_URL", &root).unwrap(),
            "https://provider.example/key"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_non_http_rpc_values_without_echoing_them() {
        let root = temporary_directory();
        let secret = "file:///secret-provider-token";
        fs::write(root.join(".env"), format!("V4HOOK_TEST_RPC_URL={secret}\n")).unwrap();
        let error = rpc_url_from_env("V4HOOK_TEST_RPC_URL", &root).unwrap_err();
        assert!(!format!("{error:#}").contains(secret));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_rpc_urls_preserve_the_parse_cause() {
        let root = temporary_directory();
        fs::write(root.join(".env"), "V4HOOK_TEST_RPC_URL=https://[invalid\n").unwrap();

        let error = rpc_url_from_env("V4HOOK_TEST_RPC_URL", &root).unwrap_err();

        assert!(error.chain().count() > 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_rpc_arguments_in_live_verification() {
        assert!(
            reject_live_rpc_arguments(&[
                "forge".to_owned(),
                "script".to_owned(),
                "--rpc-url".to_owned(),
                "{rpcUrl}".to_owned(),
            ])
            .is_err()
        );
        reject_live_rpc_arguments(&[
            "forge".to_owned(),
            "script".to_owned(),
            "script/VerifyPool.s.sol:VerifyPool".to_owned(),
        ])
        .unwrap();
    }

    #[test]
    fn structured_slither_policy_owns_scope_and_exact_allowances() {
        assert!(
            validate_slither_command(&[
                "slither".to_owned(),
                ".".to_owned(),
                "--exclude".to_owned(),
                "timestamp".to_owned(),
            ])
            .is_err()
        );
        assert!(
            validate_slither_command(&[
                "slither".to_owned(),
                ".".to_owned(),
                "--filter-paths".to_owned(),
                "src/".to_owned(),
            ])
            .is_err()
        );
        validate_slither_command(&["uvx".to_owned(), "slither".to_owned(), ".".to_owned()])
            .unwrap();
    }

    #[test]
    fn gas_budget_must_be_a_failing_snapshot_check() {
        validate_gas_snapshot_command(&[
            "forge".to_owned(),
            "snapshot".to_owned(),
            "--check".to_owned(),
            ".gas-snapshot".to_owned(),
        ])
        .unwrap();
        assert!(
            validate_gas_snapshot_command(&[
                "forge".to_owned(),
                "snapshot".to_owned(),
                "--diff".to_owned(),
            ])
            .is_err()
        );
    }

    #[test]
    fn detects_stage_paths_sentinels_and_incompatible_authorities() {
        let root = temporary_directory();
        fs::create_dir_all(root.join("test/fork")).unwrap();
        fs::write(
            root.join("test/fork/SwapQuadrants.t.sol"),
            "contract Test {}\n",
        )
        .unwrap();

        assert!(any_matching_file(&root, "test/fork/**"));
        assert!(any_matching_file(&root, "test/fork/SwapQuadrants.t.sol"));
        assert!(!any_matching_file(&root, "test/invariant/**"));
        assert!(conspicuous_address_sentinel(
            "0x0000000000000000000000000000000000000003",
            false
        ));
        assert!(!conspicuous_address_sentinel(
            "0x0000000000000000000000000000000000000000",
            true
        ));
        assert!(!conspicuous_address_sentinel(
            "0x8366a39cc670b4001a1121b8f6a443a643e40951",
            false
        ));

        let authorities = BTreeMap::from([
            (
                "registrar".to_owned(),
                "0x1000000000000000000000000000000000000000".to_owned(),
            ),
            (
                "treasury".to_owned(),
                "0x2000000000000000000000000000000000000000".to_owned(),
            ),
        ]);
        let mut issues = Vec::new();
        push_authority_issues(&mut issues, &authorities, "pool stage");
        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("different addresses"));

        let shared = BTreeMap::from([
            (
                "registrar".to_owned(),
                "0x1000000000000000000000000000000000000000".to_owned(),
            ),
            (
                "treasury".to_owned(),
                "0x1000000000000000000000000000000000000000".to_owned(),
            ),
        ]);
        require_broadcast_sender(
            &shared,
            "0x1000000000000000000000000000000000000000",
            "pool stage",
        )
        .unwrap();
        assert!(
            require_broadcast_sender(
                &shared,
                "0x2000000000000000000000000000000000000000",
                "pool stage",
            )
            .unwrap_err()
            .to_string()
            .contains("requires sender")
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validates_devnet_account_and_scenario_boundaries() {
        let mut valid = DevnetConfig {
            accounts: 100,
            block_time_seconds: Some(1),
            scenarios: vec![crate::model::DevnetScenario {
                name: "mizu-market".to_owned(),
                command: vec!["pnpm".to_owned(), "simulate".to_owned()],
                verification: crate::model::DevnetScenarioVerification {
                    expected_transactions: 198,
                    expected_senders: 99,
                    allowed_targets: vec!["0x1000000000000000000000000000000000000000".to_owned()],
                    required_events: Vec::new(),
                    reserved_account_indices: vec![0],
                },
            }],
        };
        validate_devnet(&mut valid).unwrap();

        let mut invalid_accounts = valid.clone();
        invalid_accounts.accounts = 0;
        assert!(validate_devnet(&mut invalid_accounts).is_err());

        let mut duplicate = valid.clone();
        duplicate.scenarios.push(duplicate.scenarios[0].clone());
        assert!(
            validate_devnet(&mut duplicate)
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );
    }
}
