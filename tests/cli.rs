use std::{
    fs,
    path::PathBuf,
    process::Command,
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
    let metadata = fs::read_to_string(destination.0.join(".v4hook.toml"))
        .expect("scaffold includes template metadata");
    assert!(metadata.contains("version = \"1.0.1\""));
}
