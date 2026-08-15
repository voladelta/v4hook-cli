use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

use crate::{
    artifact::{code_hash, load_artifact, make_init_code, mine_create2},
    checks::run_check_suite,
    config::rpc_url_from_env,
    model::{
        ArtifactIdentity, ContractIdentity, DeploymentPlan, HookIdentity, LoadedConfig,
        NetworkIdentity, PlanDeployment, PlanSimulation, SourceIdentity, ToolchainIdentity,
    },
    permissions::{permission_flags, permission_suffix, verify_address_flags},
    process::{command, require_success},
    rpc::{block_hash, block_number, chain_id, code_at},
    util::{assert_digest, now_iso, resolve_from, sha256_bytes, sha256_file, status},
};

pub const RUST_TOOLCHAIN: &str = "1.97.1";

pub fn cli_identity() -> String {
    format!(
        "v4hook {} (rustc {RUST_TOOLCHAIN})",
        env!("CARGO_PKG_VERSION")
    )
}

fn tool_version(command_name: &str, cwd: &Path) -> Result<String> {
    let result = require_success(&command(&[command_name, "--version"]), cwd, None, false)?;
    Ok(result
        .stdout
        .lines()
        .next()
        .unwrap_or("unknown")
        .trim()
        .to_owned())
}

pub fn current_toolchain(cwd: &Path) -> Result<ToolchainIdentity> {
    Ok(ToolchainIdentity {
        node: cli_identity(),
        forge: tool_version("forge", cwd)?,
        cast: tool_version("cast", cwd)?,
        anvil: tool_version("anvil", cwd)?,
    })
}

pub fn source_identity(cwd: &Path) -> Result<SourceIdentity> {
    let status = require_success(
        &command(&["git", "status", "--porcelain"]),
        cwd,
        None,
        false,
    )?;
    if !status.stdout.trim().is_empty() {
        bail!("project git worktree must be clean before planning");
    }
    let commit = require_success(&command(&["git", "rev-parse", "HEAD"]), cwd, None, false)?
        .stdout
        .trim()
        .to_owned();
    let tree = require_success(
        &command(&["git", "rev-parse", "HEAD^{tree}"]),
        cwd,
        None,
        false,
    )?
    .stdout
    .trim()
    .to_owned();
    let submodules = require_success(
        &command(&["git", "submodule", "status", "--recursive"]),
        cwd,
        None,
        false,
    )?
    .stdout;
    Ok(SourceIdentity {
        commit,
        dirty: false,
        tree_digest: sha256_bytes(format!("{tree}\n{submodules}")),
    })
}

pub fn create_deployment_plan(config: &LoadedConfig) -> Result<DeploymentPlan> {
    status("Checking the target network...");
    let project_root = Path::new(&config.project_root);
    let rpc_url = rpc_url_from_env(&config.value.network.rpc_url_env, project_root)?;
    let actual_chain_id = chain_id(&rpc_url)?;
    if actual_chain_id != config.value.network.chain_id {
        bail!(
            "RPC chain ID {actual_chain_id} does not match configured {}",
            config.value.network.chain_id
        );
    }
    let checks = run_check_suite(config)?;
    let artifact_path = resolve_from(project_root, &config.value.contract.artifact);
    let artifact = load_artifact(&artifact_path)?;
    let init_code = make_init_code(
        &artifact.creation_bytecode,
        &config.value.contract.constructor_args,
    )?;
    let flags = permission_flags(&config.value.contract.permissions)?;
    status("Mining a CREATE2 address with the required hook flags...");
    let (predicted_address, salt) = mine_create2(
        &init_code,
        &config.value.network.create2_deployer,
        &permission_suffix(flags),
        project_root,
    )?;
    verify_address_flags(&predicted_address, flags)?;

    let fork_block_number = block_number(&rpc_url)?;
    let fork_block_hash = block_hash(&rpc_url, fork_block_number)?;
    let configured_contracts = [
        ("poolManager", &config.value.network.pool_manager),
        ("positionManager", &config.value.network.position_manager),
        ("universalRouter", &config.value.network.universal_router),
        ("quoter", &config.value.network.quoter),
        ("stateView", &config.value.network.state_view),
        ("permit2", &config.value.network.permit2),
        ("create2Deployer", &config.value.network.create2_deployer),
    ];
    status("Pinning deployed dependency code hashes...");
    let mut contracts = BTreeMap::new();
    for (name, address) in configured_contracts {
        let code = code_at(&rpc_url, address)?;
        if code == "0x" {
            bail!("{name} has no code at {address}");
        }
        contracts.insert(
            name.to_owned(),
            ContractIdentity {
                address: address.clone(),
                code_hash: code_hash(&code)?,
            },
        );
    }
    if code_at(&rpc_url, &predicted_address)? != "0x" {
        bail!("predicted hook address is already occupied: {predicted_address}");
    }

    let mut plan = DeploymentPlan {
        schema_version: "v4hook.deployment-plan.v1".to_owned(),
        created_at: now_iso(),
        config_path: config.path.clone(),
        config_digest: sha256_bytes(&config.raw),
        project_root: config.project_root.clone(),
        source: source_identity(project_root)?,
        toolchain: current_toolchain(project_root)?,
        network: NetworkIdentity {
            chain_id: actual_chain_id,
            rpc_url_env: config.value.network.rpc_url_env.clone(),
            fork_block_number,
            fork_block_hash,
            contracts,
        },
        artifact: ArtifactIdentity {
            path: artifact_path.to_string_lossy().into_owned(),
            file_digest: artifact.file_digest,
            creation_bytecode_hash: code_hash(&artifact.creation_bytecode)?,
            runtime_bytecode_hash: code_hash(&artifact.runtime_bytecode)?,
            constructor_args: config.value.contract.constructor_args.clone(),
            init_code_hash: code_hash(&init_code)?,
            immutable_references: artifact.immutable_references,
        },
        hook: HookIdentity {
            permissions: config.value.contract.permissions.clone(),
            flags: format!("0x{flags:04x}"),
            salt,
            predicted_address,
        },
        checks,
        simulation: PlanSimulation {
            max_fork_block_drift: config.value.simulation.max_fork_block_drift,
            anvil_args: config.value.simulation.anvil_args.clone(),
            steps: config.value.simulation.steps.clone(),
        },
        deployment: PlanDeployment {
            script: config.value.deployment.script.clone(),
        },
        digest: String::new(),
    };
    plan.digest = crate::util::calculate_digest(&plan)?;
    Ok(plan)
}

pub fn read_deployment_plan(path: impl AsRef<Path>) -> Result<DeploymentPlan> {
    let path = path.as_ref();
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let plan: DeploymentPlan =
        serde_json::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
    if plan.schema_version != "v4hook.deployment-plan.v1" {
        bail!(
            "unsupported deployment plan schemaVersion: {}",
            plan.schema_version
        );
    }
    assert_digest(&plan, &plan.digest, "deployment plan")?;
    Ok(plan)
}

pub fn verify_plan_inputs(plan: &DeploymentPlan) -> Result<()> {
    let config_raw = fs::read_to_string(&plan.config_path)
        .with_context(|| format!("read {}", plan.config_path))?;
    if sha256_bytes(config_raw) != plan.config_digest {
        bail!("configuration changed after the deployment plan was created");
    }
    if sha256_file(&plan.artifact.path)? != plan.artifact.file_digest {
        bail!("Foundry artifact changed after the deployment plan was created");
    }
    let project_root = Path::new(&plan.project_root);
    let current_source = source_identity(project_root)?;
    if current_source.commit != plan.source.commit
        || current_source.tree_digest != plan.source.tree_digest
    {
        bail!("project source changed after the deployment plan was created");
    }
    let current = current_toolchain(project_root)?;
    for (name, expected, actual) in [
        ("v4hook", &plan.toolchain.node, &current.node),
        ("forge", &plan.toolchain.forge, &current.forge),
        ("cast", &plan.toolchain.cast, &current.cast),
        ("anvil", &plan.toolchain.anvil, &current.anvil),
    ] {
        if expected != actual {
            bail!("{name} version changed after the deployment plan was created");
        }
    }
    Ok(())
}

pub fn absolute_path(path: impl AsRef<Path>) -> Result<PathBuf> {
    let path = path.as_ref();
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}
