use std::{
    fs,
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
    time::{SystemTime, UNIX_EPOCH},
};

struct TemporaryDirectory(PathBuf);

impl TemporaryDirectory {
    fn new(name: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before Unix epoch")
            .as_nanos();
        Self(std::env::temp_dir().join(format!("v4hook-{name}-{}-{nonce}", std::process::id())))
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct DaemonGuard(Option<u32>);

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        if let Some(pid) = self.0.take() {
            let _ = Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

fn available_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("allocate test port");
    listener.local_addr().expect("read test port").port()
}

#[test]
fn pool_help_describes_each_workflow_step() {
    let output = Command::new(env!("CARGO_BIN_EXE_v4hook"))
        .args(["pool", "--help"])
        .output()
        .expect("run v4hook pool --help");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("help is UTF-8");
    assert!(stdout.contains("Bind the pool parameters"));
    assert!(stdout.contains("Exercise pool creation"));
    assert!(stdout.contains("broadcast the pool launch"));
}

#[test]
fn devnet_help_exposes_the_persistent_lifecycle() {
    let output = Command::new(env!("CARGO_BIN_EXE_v4hook"))
        .args(["devnet", "--help"])
        .output()
        .expect("run devnet help");
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    for command in ["up", "status", "reset", "export", "run", "down"] {
        assert!(help.contains(command), "missing devnet command {command}");
    }
    let down = Command::new(env!("CARGO_BIN_EXE_v4hook"))
        .args(["devnet", "down", "--help"])
        .output()
        .expect("run devnet down help");
    assert!(String::from_utf8_lossy(&down.stdout).contains("--purge-generated"));
}

#[test]
fn readiness_help_requires_bound_evidence_in_stages() {
    let output = Command::new(env!("CARGO_BIN_EXE_v4hook"))
        .args(["readiness", "--help"])
        .output()
        .expect("run readiness help");
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    for option in ["--config", "--plan", "--simulation"] {
        assert!(help.contains(option), "missing readiness option {option}");
    }
}

#[test]
fn verification_review_help_requires_a_structured_chief_adjudication() {
    let output = Command::new(env!("CARGO_BIN_EXE_v4hook"))
        .args(["verification", "review", "--help"])
        .output()
        .expect("run verification review help");
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(help.contains("v1 JSON chief-adjudicated review"));
    assert!(help.contains("--report"));
}

#[test]
#[ignore = "requires Foundry Anvil and unrestricted localhost sockets"]
fn daemonized_anvil_survives_launcher_exit() {
    let directory = TemporaryDirectory::new("daemon");
    fs::create_dir_all(&directory.0).expect("create daemon test directory");
    let pid_file = directory.0.join("anvil.pid");
    let log_file = directory.0.join("anvil.log");
    let port = available_port();
    let status = Command::new(env!("CARGO_BIN_EXE_v4hook"))
        .arg("__devnet-anvil")
        .arg("--pid-file")
        .arg(&pid_file)
        .arg("--log-file")
        .arg(&log_file)
        .arg("--working-directory")
        .arg(&directory.0)
        .arg("--")
        .args([
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--chain-id",
            "31337",
            "--accounts",
            "100",
            "--silent",
        ])
        .status()
        .expect("run daemon launcher");
    assert!(status.success());
    let pid = (0..100)
        .find_map(|_| {
            let pid = fs::read_to_string(&pid_file)
                .ok()
                .and_then(|raw| raw.trim().parse::<u32>().ok());
            if pid.is_none() {
                thread::sleep(Duration::from_millis(20));
            }
            pid
        })
        .expect("daemon writes PID file");
    let mut daemon = DaemonGuard(Some(pid));
    let address = format!("127.0.0.1:{port}");
    for _ in 0..100 {
        if TcpStream::connect(&address).is_ok() {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert!(TcpStream::connect(&address).is_ok());
    let rpc_url = format!("http://{address}");
    let accounts = Command::new("cast")
        .args(["rpc", "eth_accounts", "--rpc-url", &rpc_url])
        .output()
        .expect("read daemon accounts");
    assert!(accounts.status.success());
    let accounts: serde_json::Value =
        serde_json::from_slice(&accounts.stdout).expect("accounts are JSON");
    assert_eq!(
        accounts.as_array().expect("accounts are an array").len(),
        100
    );
    let pid = daemon.0.take().expect("daemon PID");
    let stopped = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status()
        .expect("stop daemon");
    assert!(stopped.success());
}

#[test]
fn init_keeps_captured_stdout_machine_readable() {
    let destination = TemporaryDirectory::new("init");
    let output = Command::new(env!("CARGO_BIN_EXE_v4hook"))
        .arg("init")
        .arg(&destination.0)
        .output()
        .expect("run v4hook init");
    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout contains only JSON");
    assert_eq!(value["directory"], destination.0.to_string_lossy().as_ref());
    assert!(destination.0.join(".git").is_dir());
    let agent_instructions =
        fs::read_to_string(destination.0.join("AGENTS.md")).expect("scaffold includes AGENTS.md");
    assert!(agent_instructions.contains("v4-security-foundations"));
    assert!(agent_instructions.contains("viem-integration"));
    assert!(agent_instructions.contains("v4-sdk-integration"));
    assert!(agent_instructions.contains("EVM-integration router"));
    assert!(agent_instructions.contains("Start from the project map"));
    assert!(agent_instructions.contains("Those early reads are preload only"));
    assert!(agent_instructions.contains("until step 1 establishes this project's pinned APIs"));
    assert!(agent_instructions.contains("ETHSkills root is not a Solidity startup dependency"));
    assert!(agent_instructions.contains("explicitly selects each child's model and reasoning"));
    assert!(agent_instructions.contains("keeps one non-writing chief"));
    assert!(!agent_instructions.contains("https://ethskills.com/SKILL.md"));
    let metadata = fs::read_to_string(destination.0.join(".v4hook.toml"))
        .expect("scaffold includes template metadata");
    assert!(metadata.contains(&format!(
        "created-with-cli = \"{}\"",
        env!("CARGO_PKG_VERSION")
    )));
    assert!(metadata.contains("version = \"2.2.7\""));
    assert!(destination.0.join(".env.example").is_file());
    assert!(destination.0.join(".gas-snapshot").is_file());
    assert!(destination.0.join("v4hook.config.example.json").is_file());
    assert!(
        destination
            .0
            .join("verification-contract.example.json")
            .is_file()
    );
    assert!(
        destination
            .0
            .join("test/utils/v4hook-testkit/V4HookTestkit.sol")
            .is_file()
    );
    assert!(!destination.0.join("vendor/hookmate").exists());
    let base_script = fs::read_to_string(destination.0.join("script/base/BaseScript.sol"))
        .expect("scaffold includes BaseScript.sol");
    assert!(base_script.contains("V4HOOK_HOOK_ADDRESS"));
    assert!(base_script.contains("V4HOOK_PREDICTED_ADDRESS"));
}

#[test]
fn orchestrated_delivery_requires_profiled_non_overlapping_delegation() {
    let workflow = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("skills/v4hook-cli/references/orchestrated-delivery.md"),
    )
    .expect("orchestrated delivery reference exists");

    assert!(workflow.contains(
        "Every delegation call explicitly passes `model`, `reasoning_effort`, and `fork_turns`"
    ));
    assert!(workflow.contains(
        "Set\n`fork_turns` to `\"none\"` or a bounded positive turn count compatible with the requested profile"
    ));
    assert!(workflow.contains("omitting any of the three arguments makes the\ndispatch invalid"));
    for profile in [
        "| Scout | `gpt-5.6-luna` | xhigh |",
        "| Implementor | `gpt-5.6-sol` | medium |",
        "| Reviewer | `gpt-5.6-sol` | high |",
        "| Fixer | `gpt-5.6-sol` | medium |",
        "| Verifier | `gpt-5.6-sol` | high |",
    ] {
        assert!(
            workflow.contains(profile),
            "missing delegated profile {profile}"
        );
    }
    assert!(workflow.contains(
        "do not begin until an\nexplicitly profiled implementor or fixer is recorded as their candidate owner"
    ));
    assert!(workflow.contains("The chief never\nauthors candidate changes"));
    assert!(workflow.contains(
        "Every\nchild that produces candidate changes has explicit Git working-tree authority and materializes\nthose changes in its assigned worktree"
    ));
    assert!(workflow.contains(
        "A child without working-tree authority is read-only and returns findings,\nevidence, or design only; it never returns a candidate patch for someone else to apply"
    ));
    assert!(workflow.contains(
        "dispatches a fresh explicitly profiled implementor or fixer with working-tree authority to\napply and validate it"
    ));
    assert!(workflow.contains("The chief never applies, authors, or alters candidate patches"));
    assert!(workflow.contains(
        "the chief may inspect and accept already-materialized child work,\nthen stage and commit it without changing the candidate contents"
    ));
    assert!(!workflow.contains("the child returns an uncommitted patch"));
    assert!(!workflow.contains("the chief alone\nintegrates"));
    assert!(
        workflow
            .contains("Never dispatch a replacement writer while the prior writer remains active")
    );

    let recovery_steps = [
        "Inspect the active child status",
        "Request a checkpoint containing",
        "record\n   `checkpoint unavailable`",
        "Preserve the actual partial patch and evidence",
        "Update the ledger with the inspection",
        "Interrupt the writer",
        "Confirm from child status that the interrupted writer is inactive",
        "Update the ledger with the stop confirmation",
        "Only after that ledger update, redispatch a fresh child with the exact role profile",
    ];
    let mut previous_position = None;
    for step in recovery_steps {
        let position = workflow
            .find(step)
            .unwrap_or_else(|| panic!("missing writer recovery step: {step}"));
        if let Some(previous) = previous_position {
            assert!(
                position > previous,
                "writer recovery step is out of order: {step}"
            );
        }
        previous_position = Some(position);
    }
}

#[test]
fn installer_replaces_the_global_skill_without_python_dependencies() {
    let installer = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("install.sh"))
        .expect("installer exists");

    let remove = "rm -rf -- \"$skill_destination\"";
    let copy = "cp -R -- \"$skill_source\" \"$skill_destination\"";
    let verify = "[ ! -f \"$skill_destination/SKILL.md\" ]";
    let remove_position = installer.find(remove).expect("installer removes old skill");
    let copy_position = installer
        .find(copy)
        .expect("installer copies repository skill");
    let verify_position = installer
        .find(verify)
        .expect("installer verifies copied skill");

    assert!(installer.contains("/.agents/skills"));
    assert!(remove_position < copy_position);
    assert!(copy_position < verify_position);
    assert!(!installer.contains("PyYAML"));
    assert!(!installer.contains("quick_validate"));
}

#[test]
fn scaffold_config_example_matches_repository_example() {
    assert_eq!(
        include_str!("../v4hook.config.example.json"),
        include_str!("../assets/v4-template/v4hook.config.example.json")
    );
}

#[test]
fn verification_freeze_requires_a_clean_committed_contract() {
    let destination = TemporaryDirectory::new("verification-freeze");
    let init = Command::new(env!("CARGO_BIN_EXE_v4hook"))
        .arg("init")
        .arg(&destination.0)
        .output()
        .expect("initialize verification fixture");
    assert!(init.status.success());
    fs::copy(
        destination.0.join("v4hook.config.example.json"),
        destination.0.join("v4hook.config.json"),
    )
    .expect("create active config");
    fs::copy(
        destination.0.join("verification-contract.example.json"),
        destination.0.join("verification-contract.json"),
    )
    .expect("create verification contract");

    let dirty = Command::new(env!("CARGO_BIN_EXE_v4hook"))
        .current_dir(&destination.0)
        .args([
            "verification",
            "freeze",
            "--config",
            "v4hook.config.json",
            "--contract",
            "verification-contract.json",
        ])
        .output()
        .expect("reject dirty verification freeze");
    assert!(!dirty.status.success());
    assert!(String::from_utf8_lossy(&dirty.stderr).contains("worktree must be clean"));

    for args in [
        vec!["config", "user.email", "v4hook@example.invalid"],
        vec!["config", "user.name", "v4hook test"],
        vec!["add", "."],
        vec!["commit", "-m", "test: freeze baseline"],
    ] {
        let status = Command::new("git")
            .current_dir(&destination.0)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("prepare committed verification fixture");
        assert!(status.success());
    }

    let frozen = Command::new(env!("CARGO_BIN_EXE_v4hook"))
        .current_dir(&destination.0)
        .args([
            "verification",
            "freeze",
            "--config",
            "v4hook.config.json",
            "--contract",
            "verification-contract.json",
        ])
        .output()
        .expect("freeze committed verification baseline");
    assert!(
        frozen.status.success(),
        "freeze failed: {}",
        String::from_utf8_lossy(&frozen.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&frozen.stdout).expect("freeze output is JSON");
    assert_eq!(value["stage"], "frozen");
    assert!(
        destination
            .0
            .join(".v4hook/verification-state.json")
            .is_file()
    );
}
