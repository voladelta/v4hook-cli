use std::{
    fs::{self, OpenOptions},
    io::Read,
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use anyhow::{Context, Result, bail};

use crate::rpc::{block_hash, block_number, chain_id, reset_fork, wait_for_rpc};

pub const ANVIL_DEFAULT_SENDER: &str = "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266";

pub struct AnvilHandle {
    pub rpc_url: String,
    pub sender: String,
    child: Option<Child>,
}

impl AnvilHandle {
    pub fn pid(&self) -> Result<u32> {
        self.child
            .as_ref()
            .map(Child::id)
            .context("Anvil process is not owned by this handle")
    }

    pub fn detach(&mut self) -> Result<u32> {
        let child = self
            .child
            .take()
            .context("Anvil process is not owned by this handle")?;
        let pid = child.id();
        drop(child);
        Ok(pid)
    }

    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AnvilStartOptions {
    pub port: Option<u16>,
    pub accounts: Option<u16>,
    pub block_time_seconds: Option<u64>,
    pub log_path: Option<PathBuf>,
}

impl Drop for AnvilHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

fn reject_reserved_args(args: &[String]) -> Result<()> {
    const RESERVED: [(&str, Option<&str>); 5] = [
        ("--fork-url", Some("-f")),
        ("--fork-block-number", None),
        ("--host", None),
        ("--port", Some("-p")),
        ("--chain-id", None),
    ];
    for argument in args {
        if let Some((flag, _)) = RESERVED.iter().find(|(flag, short)| {
            argument == *flag
                || argument.starts_with(&format!("{flag}="))
                || short.is_some_and(|short| argument == short || argument.starts_with(short))
        }) {
            bail!("anvilArgs cannot override CLI-controlled {flag}");
        }
    }
    Ok(())
}

fn reject_conflicting_options(extra_args: &[String], options: &AnvilStartOptions) -> Result<()> {
    for (enabled, flag, short) in [
        (options.accounts.is_some(), "--accounts", "-a"),
        (options.block_time_seconds.is_some(), "--block-time", "-b"),
    ] {
        if enabled
            && extra_args.iter().any(|argument| {
                argument == flag
                    || argument == short
                    || argument.starts_with(&format!("{flag}="))
                    || argument.starts_with(short)
            })
        {
            bail!("{flag} cannot be set in both devnet options and simulation.anvilArgs");
        }
    }
    Ok(())
}

fn allocate_port(requested: Option<u16>) -> Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", requested.unwrap_or(0)))
        .context("allocate local Anvil port")?;
    Ok(listener
        .local_addr()
        .context("read allocated local Anvil port")?
        .port())
}

fn configure_output(process: &mut Command, log_path: Option<&Path>) -> Result<()> {
    if let Some(log_path) = log_path {
        if let Some(parent) = log_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create Anvil log directory {}", parent.display()))?;
        }
        if fs::symlink_metadata(log_path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            bail!("refusing to write Anvil log through a symbolic link");
        }
        let stdout = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(log_path)
            .with_context(|| format!("create Anvil log {}", log_path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            stdout
                .set_permissions(fs::Permissions::from_mode(0o600))
                .with_context(|| format!("protect Anvil log {}", log_path.display()))?;
        }
        let stderr = stdout
            .try_clone()
            .with_context(|| format!("clone Anvil log {}", log_path.display()))?;
        process
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
    } else {
        process.stdout(Stdio::piped()).stderr(Stdio::piped());
    }
    Ok(())
}

fn capture_output(child: &mut Child) -> Arc<Mutex<String>> {
    let output = Arc::new(Mutex::new(String::new()));
    for mut stream in [
        child.stdout.take().map(Stream::Stdout),
        child.stderr.take().map(Stream::Stderr),
    ]
    .into_iter()
    .flatten()
    {
        let output = Arc::clone(&output);
        thread::spawn(move || {
            let mut buffer = String::new();
            stream.read_to_string(&mut buffer).ok();
            if let Ok(mut output) = output.lock() {
                output.push_str(&buffer);
            }
        });
    }
    output
}

fn verify_fork(
    rpc_url: &str,
    target_rpc_url: &str,
    fork_block_number: u64,
    expected_chain_id: u64,
) -> Result<()> {
    reset_fork(rpc_url, target_rpc_url, fork_block_number)?;
    if chain_id(rpc_url)? != expected_chain_id {
        bail!("local Anvil chain ID does not match the target network");
    }
    if block_number(rpc_url)? != fork_block_number {
        bail!("local Anvil did not reset to the pinned fork block");
    }
    if block_hash(rpc_url, fork_block_number)? != block_hash(target_rpc_url, fork_block_number)? {
        bail!("local Anvil fork block hash does not match the target network");
    }
    Ok(())
}

pub fn start_anvil(
    target_rpc_url: &str,
    target_rpc_env_name: &str,
    fork_block_number: u64,
    expected_chain_id: u64,
    extra_args: &[String],
    cwd: &Path,
) -> Result<AnvilHandle> {
    start_anvil_with_options(
        target_rpc_url,
        target_rpc_env_name,
        fork_block_number,
        expected_chain_id,
        extra_args,
        cwd,
        &AnvilStartOptions::default(),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn start_anvil_with_options(
    target_rpc_url: &str,
    target_rpc_env_name: &str,
    fork_block_number: u64,
    expected_chain_id: u64,
    extra_args: &[String],
    cwd: &Path,
    options: &AnvilStartOptions,
) -> Result<AnvilHandle> {
    reject_reserved_args(extra_args)?;
    reject_conflicting_options(extra_args, options)?;
    let port = allocate_port(options.port)?;
    let mut args = [
        "--host".to_owned(),
        "127.0.0.1".to_owned(),
        "--port".to_owned(),
        port.to_string(),
        "--chain-id".to_owned(),
        expected_chain_id.to_string(),
    ]
    .into_iter()
    .chain(extra_args.iter().cloned())
    .collect::<Vec<_>>();
    if let Some(accounts) = options.accounts {
        args.extend(["--accounts".to_owned(), accounts.to_string()]);
    }
    if let Some(block_time) = options.block_time_seconds {
        args.extend(["--block-time".to_owned(), block_time.to_string()]);
    }
    let mut process = Command::new("anvil");
    process
        .args(&args)
        .current_dir(cwd)
        .env_remove(target_rpc_env_name)
        .stdin(Stdio::null());
    configure_output(&mut process, options.log_path.as_deref())?;
    let mut child = process.spawn().context("start Anvil")?;
    let output = if options.log_path.is_none() {
        capture_output(&mut child)
    } else {
        Arc::new(Mutex::new(String::new()))
    };
    let rpc_url = format!("http://127.0.0.1:{port}");
    if let Err(error) = wait_for_rpc(&rpc_url, Duration::from_secs(20)) {
        let _ = child.kill();
        let _ = child.wait();
        let output = output
            .lock()
            .map(|value| value.trim().to_owned())
            .unwrap_or_default();
        let output = output.replace(target_rpc_url, "[REDACTED RPC URL]");
        let context = if output.is_empty() {
            "failed to start Anvil".to_owned()
        } else {
            format!("failed to start Anvil: {output}")
        };
        return Err(error).context(context);
    }
    if let Err(error) = verify_fork(
        &rpc_url,
        target_rpc_url,
        fork_block_number,
        expected_chain_id,
    ) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error).context("failed to configure Anvil fork");
    }
    Ok(AnvilHandle {
        rpc_url,
        sender: ANVIL_DEFAULT_SENDER.to_owned(),
        child: Some(child),
    })
}

enum Stream {
    Stdout(std::process::ChildStdout),
    Stderr(std::process::ChildStderr),
}

impl Read for Stream {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Stdout(stream) => stream.read(buffer),
            Self::Stderr(stream) => stream.read(buffer),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::anvil_accounts;

    struct ProcessGuard(Child);

    impl Drop for ProcessGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn rejects_short_forms_of_cli_owned_anvil_options() {
        assert!(reject_reserved_args(&["-p8545".to_owned()]).is_err());
        assert!(reject_reserved_args(&["-f".to_owned()]).is_err());
        assert!(
            reject_conflicting_options(
                &["-a100".to_owned()],
                &AnvilStartOptions {
                    accounts: Some(100),
                    ..AnvilStartOptions::default()
                }
            )
            .is_err()
        );
    }

    #[test]
    #[ignore = "requires Foundry Anvil and unrestricted localhost sockets"]
    fn detached_fork_survives_with_one_hundred_accounts() {
        let source_port = allocate_port(None).unwrap();
        let source = Command::new("anvil")
            .args([
                "--host",
                "127.0.0.1",
                "--port",
                &source_port.to_string(),
                "--chain-id",
                "31337",
                "--silent",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let _source = ProcessGuard(source);
        let source_rpc = format!("http://127.0.0.1:{source_port}");
        wait_for_rpc(&source_rpc, Duration::from_secs(20)).unwrap();

        let destination_port = allocate_port(None).unwrap();
        let log_path = std::env::temp_dir().join(format!(
            "v4hook-anvil-smoke-{}-{destination_port}.log",
            std::process::id()
        ));
        let mut fork = start_anvil_with_options(
            &source_rpc,
            "V4HOOK_TEST_RPC_URL",
            0,
            31337,
            &[],
            Path::new("."),
            &AnvilStartOptions {
                port: Some(destination_port),
                accounts: Some(100),
                block_time_seconds: None,
                log_path: Some(log_path.clone()),
            },
        )
        .unwrap();
        assert_eq!(anvil_accounts(&fork.rpc_url).unwrap().len(), 100);
        let rpc_url = fork.rpc_url.clone();
        let pid = fork.detach().unwrap();
        assert_eq!(chain_id(&rpc_url).unwrap(), 31337);
        assert_eq!(block_number(&rpc_url).unwrap(), 0);

        let status = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .status()
            .unwrap();
        assert!(status.success());
        for _ in 0..50 {
            if wait_for_rpc(&rpc_url, Duration::from_millis(20)).is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(wait_for_rpc(&rpc_url, Duration::from_millis(20)).is_err());
        let _ = fs::remove_file(log_path);
    }
}
