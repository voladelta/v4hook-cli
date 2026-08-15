mod anvil;
mod artifact;
mod checks;
mod config;
mod deploy;
mod doctor;
mod init;
mod model;
mod permissions;
mod plan;
mod pool;
mod process;
mod rpc;
mod scaffold;
mod simulate;
mod template;
mod util;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;
use serde_json::json;

use crate::{
    checks::run_check_suite,
    config::load_config,
    deploy::{DeployInput, deploy_hook, verify_hook_deployment},
    doctor::doctor,
    init::initialize_project,
    plan::create_deployment_plan,
    pool::{LaunchPoolInput, SimulatePoolInput, create_pool_plan, launch_pool, simulate_pool},
    scaffold::{ScaffoldUpdateInput, update_scaffold},
    simulate::simulate_deployment,
    template::{TemplateRefreshInput, refresh_template},
    util::write_json,
};

#[derive(Parser)]
#[command(
    name = "v4hook",
    version,
    about = "Foundry-backed verification and launch gates for Uniswap v4 hooks"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Copy the bundled, pinned Uniswap v4 hook scaffold.
    Init { directory: PathBuf },
    /// Update a user project from the scaffold in this CLI version.
    Scaffold {
        #[command(subcommand)]
        command: ScaffoldCommand,
    },
    /// Maintain the scaffold bundled in the v4hook CLI repository.
    Template {
        #[command(subcommand)]
        command: TemplateCommand,
    },
    /// Check the local toolchain and optional project configuration.
    Doctor {
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
    /// Run format, lint, static analysis, build, unit, fuzz, and invariant gates.
    Check {
        #[arg(short, long)]
        config: PathBuf,
    },
    /// Build, test, mine the hook address, and write an immutable deployment plan.
    Plan {
        #[arg(short, long)]
        config: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Run mandatory deployment, pool, quadrant, and postcondition gates on a pinned Anvil fork.
    Simulate {
        #[arg(short, long)]
        plan: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Rerun mandatory fork simulation and broadcast the exact planned hook.
    Deploy(DeployArgs),
    /// Verify live hook code and all pinned network dependencies.
    Verify {
        #[arg(short, long)]
        plan: PathBuf,
    },
    /// Separately plan, simulate, and launch a v4 pool.
    Pool {
        #[command(subcommand)]
        command: PoolCommand,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ConflictPolicy {
    Abort,
    Preserve,
    Overwrite,
}

#[derive(Subcommand)]
enum ScaffoldCommand {
    /// Update scaffold-managed files without cloning a repository.
    Update {
        #[arg(default_value = ".")]
        directory: PathBuf,
        #[arg(long)]
        dry_run: bool,
        #[arg(long, value_enum)]
        conflicts: Option<ConflictPolicy>,
    },
}

#[derive(Subcommand)]
enum TemplateCommand {
    /// Download and prepare a pinned upstream scaffold for the next CLI release.
    Refresh {
        #[arg(long)]
        version: String,
        #[arg(long, default_value = "Uniswap/v4-template")]
        source: String,
        #[arg(long, default_value = "main")]
        reference: String,
        #[arg(long, default_value = ".")]
        repository: PathBuf,
    },
}

#[derive(Args)]
struct DeployArgs {
    #[arg(short, long)]
    plan: PathBuf,
    #[arg(long)]
    account: String,
    #[arg(long)]
    sender: String,
    #[arg(long)]
    confirm: String,
    #[arg(long)]
    mainnet: bool,
    #[arg(long)]
    verify: bool,
    #[arg(long, default_value = ".v4hook/deployment-evidence.json")]
    evidence_output: PathBuf,
    #[arg(long, default_value = ".v4hook/deployment-record.json")]
    record_output: PathBuf,
}

#[derive(Subcommand)]
enum PoolCommand {
    Plan {
        #[arg(short = 'd', long)]
        deployment_plan: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    Simulate {
        #[arg(short = 'd', long)]
        deployment_plan: PathBuf,
        #[arg(short, long)]
        pool_plan: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    Launch(PoolLaunchArgs),
}

#[derive(Args)]
struct PoolLaunchArgs {
    #[arg(short = 'd', long)]
    deployment_plan: PathBuf,
    #[arg(short, long)]
    pool_plan: PathBuf,
    #[arg(long)]
    account: String,
    #[arg(long)]
    sender: String,
    #[arg(long)]
    confirm: String,
    #[arg(long)]
    mainnet: bool,
    #[arg(long, default_value = ".v4hook/pool-evidence.json")]
    evidence_output: PathBuf,
    #[arg(long, default_value = ".v4hook/pool-record.json")]
    record_output: PathBuf,
}

fn print_json(value: &impl Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn run() -> Result<i32> {
    match Cli::parse().command {
        Command::Init { directory } => print_json(&initialize_project(&directory)?)?,
        Command::Scaffold { command } => match command {
            ScaffoldCommand::Update {
                directory,
                dry_run,
                conflicts,
            } => print_json(&update_scaffold(&ScaffoldUpdateInput {
                directory: &directory,
                dry_run,
                conflicts: conflicts.map(|policy| match policy {
                    ConflictPolicy::Abort => scaffold::ConflictPolicy::Abort,
                    ConflictPolicy::Preserve => scaffold::ConflictPolicy::Preserve,
                    ConflictPolicy::Overwrite => scaffold::ConflictPolicy::Overwrite,
                }),
            })?)?,
        },
        Command::Template { command } => match command {
            TemplateCommand::Refresh {
                version,
                source,
                reference,
                repository,
            } => print_json(&refresh_template(&TemplateRefreshInput {
                repository: &repository,
                version: &version,
                source: &source,
                reference: &reference,
            })?)?,
        },
        Command::Doctor { config } => {
            let config = config.as_ref().map(load_config).transpose()?;
            let result = doctor(config.as_ref());
            let ok = result
                .get("ok")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            print_json(&result)?;
            if !ok {
                return Ok(2);
            }
        }
        Command::Check { config } => {
            let config = load_config(config)?;
            print_json(&json!({"ok": true, "checks": run_check_suite(&config)?}))?;
        }
        Command::Plan { config, output } => {
            let plan = create_deployment_plan(&load_config(config)?)?;
            write_json(&output, &plan)?;
            print_json(&json!({
                "ok": true,
                "output": output,
                "digest": plan.digest,
                "predictedAddress": plan.hook.predicted_address,
            }))?;
        }
        Command::Simulate { plan, output } => {
            let evidence = simulate_deployment(&plan, Some(&output))?;
            print_json(&json!({"ok": true, "output": output, "digest": evidence.digest}))?;
        }
        Command::Deploy(args) => print_json(&deploy_hook(&DeployInput {
            plan_file: &args.plan,
            account: &args.account,
            sender: &args.sender,
            confirmation: &args.confirm,
            mainnet: args.mainnet,
            verify: args.verify,
            evidence_output: &args.evidence_output,
            record_output: &args.record_output,
        })?)?,
        Command::Verify { plan } => {
            let (address, runtime_code_hash) = verify_hook_deployment(plan)?;
            print_json(
                &json!({"ok": true, "address": address, "runtimeCodeHash": runtime_code_hash}),
            )?;
        }
        Command::Pool { command } => match command {
            PoolCommand::Plan {
                deployment_plan,
                output,
            } => {
                let plan = create_pool_plan(&deployment_plan)?;
                write_json(&output, &plan)?;
                print_json(&json!({"ok": true, "output": output, "digest": plan.digest}))?;
            }
            PoolCommand::Simulate {
                deployment_plan,
                pool_plan,
                output,
            } => {
                let evidence = simulate_pool(&SimulatePoolInput {
                    deployment_plan: &deployment_plan,
                    pool_plan: &pool_plan,
                    output: Some(&output),
                })?;
                print_json(&json!({"ok": true, "output": output, "digest": evidence.digest}))?;
            }
            PoolCommand::Launch(args) => print_json(&launch_pool(&LaunchPoolInput {
                deployment_plan: &args.deployment_plan,
                pool_plan: &args.pool_plan,
                account: &args.account,
                sender: &args.sender,
                confirmation: &args.confirm,
                mainnet: args.mainnet,
                evidence_output: &args.evidence_output,
                record_output: &args.record_output,
            })?)?,
        },
    }
    Ok(0)
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("error: {error:#}");
            std::process::exit(1);
        }
    }
}
