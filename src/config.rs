use std::{env, fs, path::Path};

use alloy_primitives::Address;
use anyhow::{Context, Result, bail};
use regex::Regex;

use crate::{
    model::{LoadedConfig, SimulationKind, SimulationStep, V4HookConfig},
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
    validate_check_commands(&value)?;
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
        reject_live_rpc_arguments(&pool.live_verify)?;
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

pub fn rpc_url_from_env(name: &str, project_root: &Path) -> Result<String> {
    let from_environment = env::var(name).ok().filter(|value| !value.trim().is_empty());
    let value = if let Some(value) = from_environment {
        value
    } else {
        let path = project_root.join(".env");
        let mut configured = None;
        if path.is_file() {
            let entries = dotenvy::from_path_iter(&path)
                .map_err(|_| anyhow::anyhow!("could not read {}", path.display()))?;
            for item in entries {
                let (key, value) = item
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
        .map_err(|_| anyhow::anyhow!("RPC setting {name} must be a valid HTTP(S) URL"))?;
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
        let error = rpc_url_from_env("V4HOOK_TEST_RPC_URL", &root)
            .unwrap_err()
            .to_string();
        assert!(!error.contains(secret));
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
}
