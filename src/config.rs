use std::{env, fs, path::Path};

use alloy_primitives::Address;
use anyhow::{Context, Result, bail};
use regex::Regex;

use crate::{
    model::{LoadedConfig, SimulationKind, SimulationStep, V4HookConfig},
    permissions::{permission_flags, validate_hook_address_fee},
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
    validate_command(&value.checks.unit, "checks.unit")?;
    validate_command(&value.checks.fuzz, "checks.fuzz")?;
    validate_command(&value.checks.invariant, "checks.invariant")?;
    validate_command(&value.checks.static_analysis, "checks.staticAnalysis")?;
    if value.simulation.max_fork_block_drift == 0 {
        bail!("simulation.maxForkBlockDrift must be positive");
    }
    require_steps(
        &value.simulation.steps,
        &[
            SimulationKind::Deploy,
            SimulationKind::Pool,
            SimulationKind::Quadrants,
            SimulationKind::Postconditions,
        ],
        "simulation",
    )?;
    if value.contract.artifact.is_empty() || value.deployment.script.is_empty() {
        bail!("contract.artifact and deployment.script cannot be empty");
    }
    if let Some(pool) = &mut value.pool {
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
        if pool.tick_lower < -887_272
            || pool.tick_upper > 887_272
            || pool.tick_lower >= pool.tick_upper
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
        validate_hook_address_fee(&value.contract.permissions, pool.fee)?;
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

pub fn rpc_url_from_env(name: &str) -> Result<String> {
    env::var(name).with_context(|| format!("RPC environment variable is not set: {name}"))
}
