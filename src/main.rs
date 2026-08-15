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

use std::{
    io::{self, IsTerminal},
    path::PathBuf,
};

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
    about = "Foundry-backed verification and launch gates for Uniswap v4 hooks",
    after_help = "Examples:\n  v4hook init ../my-hook\n  v4hook doctor --config v4hook.config.json\n  v4hook plan --config v4hook.config.json --output .v4hook/deployment-plan.json\n\nDocumentation and support: https://github.com/voladelta/v4hook-cli"
)]
struct Cli {
    /// Print JSON even when stdout is an interactive terminal.
    #[arg(long, global = true)]
    json: bool,
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
    /// Bind the pool parameters to a verified hook deployment plan.
    Plan {
        #[arg(short = 'd', long)]
        deployment_plan: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Exercise pool creation and swap gates on a pinned Anvil fork.
    Simulate {
        #[arg(short = 'd', long)]
        deployment_plan: PathBuf,
        #[arg(short, long)]
        pool_plan: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Rerun the fork gates, broadcast the pool launch, and verify live state.
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

fn print_output(value: &impl Serialize, human: &str, force_json: bool) -> Result<()> {
    if force_json || !io::stdout().is_terminal() {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        println!("{human}");
    }
    Ok(())
}

fn text_field<'a>(value: &'a serde_json::Value, field: &str) -> &'a str {
    value
        .get(field)
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown")
}

#[allow(clippy::too_many_lines)]
fn run() -> Result<i32> {
    let cli = Cli::parse();
    let force_json = cli.json;
    match cli.command {
        Command::Init { directory } => {
            let result = initialize_project(&directory)?;
            print_output(
                &result,
                &format!(
                    "Created {}.\nNext: cd {}",
                    text_field(&result, "directory"),
                    text_field(&result, "directory")
                ),
                force_json,
            )?;
        }
        Command::Scaffold { command } => match command {
            ScaffoldCommand::Update {
                directory,
                dry_run,
                conflicts,
            } => {
                let result = update_scaffold(&ScaffoldUpdateInput {
                    directory: &directory,
                    dry_run,
                    conflicts: conflicts.map(|policy| match policy {
                        ConflictPolicy::Abort => scaffold::ConflictPolicy::Abort,
                        ConflictPolicy::Preserve => scaffold::ConflictPolicy::Preserve,
                        ConflictPolicy::Overwrite => scaffold::ConflictPolicy::Overwrite,
                    }),
                })?;
                let action = if dry_run { "Previewed" } else { "Updated" };
                print_output(
                    &result,
                    &format!("{action} scaffold in {}.", result.directory),
                    force_json,
                )?;
            }
        },
        Command::Template { command } => match command {
            TemplateCommand::Refresh {
                version,
                source,
                reference,
                repository,
            } => {
                let result = refresh_template(&TemplateRefreshInput {
                    repository: &repository,
                    version: &version,
                    source: &source,
                    reference: &reference,
                })?;
                print_output(
                    &result,
                    &format!(
                        "Prepared template {} from {} at {}.",
                        result.template_version, result.source, result.commit
                    ),
                    force_json,
                )?;
            }
        },
        Command::Doctor { config } => {
            let config = config.as_ref().map(load_config).transpose()?;
            let result = doctor(config.as_ref());
            let ok = result
                .get("ok")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let human = if ok {
                "Toolchain and project configuration are ready."
            } else {
                "Doctor found missing requirements. Run with --json for details."
            };
            print_output(&result, human, force_json)?;
            if !ok {
                return Ok(2);
            }
        }
        Command::Check { config } => {
            let config = load_config(config)?;
            let checks = run_check_suite(&config)?;
            let result = json!({"ok": true, "checks": checks});
            print_output(&result, "All configured checks passed.", force_json)?;
        }
        Command::Plan { config, output } => {
            let plan = create_deployment_plan(&load_config(config)?)?;
            write_json(&output, &plan)?;
            let result = json!({
                "ok": true,
                "output": output,
                "digest": plan.digest,
                "predictedAddress": plan.hook.predicted_address,
            });
            print_output(
                &result,
                &format!(
                    "Wrote deployment plan to {}.\nPredicted hook: {}",
                    output.display(),
                    plan.hook.predicted_address
                ),
                force_json,
            )?;
        }
        Command::Simulate { plan, output } => {
            let evidence = simulate_deployment(&plan, Some(&output))?;
            let result = json!({"ok": true, "output": output, "digest": evidence.digest});
            print_output(
                &result,
                &format!("Fork simulation passed. Evidence: {}", output.display()),
                force_json,
            )?;
        }
        Command::Deploy(args) => {
            let result = deploy_hook(&DeployInput {
                plan_file: &args.plan,
                account: &args.account,
                sender: &args.sender,
                confirmation: &args.confirm,
                mainnet: args.mainnet,
                verify: args.verify,
                evidence_output: &args.evidence_output,
                record_output: &args.record_output,
            })?;
            print_output(
                &result,
                &format!(
                    "Deployed and verified hook {}.\nRecord: {}",
                    text_field(&result, "hookAddress"),
                    args.record_output.display()
                ),
                force_json,
            )?;
        }
        Command::Verify { plan } => {
            let (address, runtime_code_hash) = verify_hook_deployment(plan)?;
            let result =
                json!({"ok": true, "address": address, "runtimeCodeHash": runtime_code_hash});
            print_output(
                &result,
                &format!("Verified hook {address}.\nRuntime code hash: {runtime_code_hash}"),
                force_json,
            )?;
        }
        Command::Pool { command } => match command {
            PoolCommand::Plan {
                deployment_plan,
                output,
            } => {
                let plan = create_pool_plan(&deployment_plan)?;
                write_json(&output, &plan)?;
                let result = json!({"ok": true, "output": output, "digest": plan.digest});
                print_output(
                    &result,
                    &format!("Wrote pool plan to {}.", output.display()),
                    force_json,
                )?;
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
                let result = json!({"ok": true, "output": output, "digest": evidence.digest});
                print_output(
                    &result,
                    &format!(
                        "Pool fork simulation passed. Evidence: {}",
                        output.display()
                    ),
                    force_json,
                )?;
            }
            PoolCommand::Launch(args) => {
                let result = launch_pool(&LaunchPoolInput {
                    deployment_plan: &args.deployment_plan,
                    pool_plan: &args.pool_plan,
                    account: &args.account,
                    sender: &args.sender,
                    confirmation: &args.confirm,
                    mainnet: args.mainnet,
                    evidence_output: &args.evidence_output,
                    record_output: &args.record_output,
                })?;
                print_output(
                    &result,
                    &format!(
                        "Launched and verified the pool for hook {}.\nRecord: {}",
                        text_field(&result, "hookAddress"),
                        args.record_output.display()
                    ),
                    force_json,
                )?;
            }
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
