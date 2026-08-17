use std::{
    fs::{self, File, OpenOptions},
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
    process: Option<OwnedAnvilProcess>,
}

enum OwnedAnvilProcess {
    Child(Child),
    Daemon(u32),
}

impl AnvilHandle {
    pub fn pid(&self) -> Result<u32> {
        match self.process.as_ref() {
            Some(OwnedAnvilProcess::Child(child)) => Ok(child.id()),
            Some(OwnedAnvilProcess::Daemon(pid)) => Ok(*pid),
            None => bail!("Anvil process is not owned by this handle"),
        }
    }

    pub fn detach(&mut self) -> Result<u32> {
        let process = self
            .process
            .take()
            .context("Anvil process is not owned by this handle")?;
        match process {
            OwnedAnvilProcess::Child(child) => Ok(child.id()),
            OwnedAnvilProcess::Daemon(pid) => Ok(pid),
        }
    }

    pub fn stop(&mut self) {
        match self.process.take() {
            Some(OwnedAnvilProcess::Child(mut child)) => {
                let _ = child.kill();
                let _ = child.wait();
            }
            Some(OwnedAnvilProcess::Daemon(pid)) => terminate_daemon(pid),
            None => {}
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AnvilStartOptions {
    pub port: Option<u16>,
    pub accounts: Option<u16>,
    pub block_time_seconds: Option<u64>,
    pub log_path: Option<PathBuf>,
    pub persistent: bool,
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

fn ensure_quiet_output(args: &mut Vec<String>) {
    let quiet = args.iter().any(|argument| {
        argument == "--quiet"
            || argument == "-q"
            || argument == "--silent"
            || argument.starts_with("--quiet=")
    });
    if !quiet {
        args.push("--quiet".to_owned());
    }
}

fn contains_account_secrets(output: &str) -> bool {
    let normalized = output.to_ascii_lowercase();
    normalized.contains("private key") || normalized.contains("mnemonic")
}

fn allocate_port(requested: Option<u16>) -> Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", requested.unwrap_or(0)))
        .context("allocate local Anvil port")?;
    Ok(listener
        .local_addr()
        .context("read allocated local Anvil port")?
        .port())
}

fn open_private_file(path: &Path, label: &str) -> Result<File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create {label} directory {}", parent.display()))?;
    }
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        bail!("refusing to write {label} through a symbolic link");
    }
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .with_context(|| format!("create {label} {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .with_context(|| format!("protect {label} {}", path.display()))?;
    }
    Ok(file)
}

fn open_private_log(path: &Path) -> Result<(File, File)> {
    let stdout = open_private_file(path, "Anvil log")?;
    let stderr = stdout
        .try_clone()
        .with_context(|| format!("clone Anvil log {}", path.display()))?;
    Ok((stdout, stderr))
}

fn configure_output(process: &mut Command, log_path: Option<&Path>) -> Result<()> {
    if let Some(log_path) = log_path {
        let (stdout, stderr) = open_private_log(log_path)?;
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

fn terminate_daemon(pid: u32) {
    let _ = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .status();
    for _ in 0..50 {
        let running = Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        if !running {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(unix)]
pub fn daemonize_anvil(
    pid_file: &Path,
    log_file: &Path,
    working_directory: &Path,
    anvil_args: &[String],
) -> Result<()> {
    use daemonize::Daemonize;
    use std::os::unix::process::CommandExt;

    let pid_reservation = open_private_file(pid_file, "Anvil PID file")?;
    drop(pid_reservation);
    let (stdout, stderr) = open_private_log(log_file)?;
    if let Err(error) = Daemonize::new()
        .pid_file(pid_file)
        .working_directory(working_directory)
        .umask(0o077)
        .stdout(stdout)
        .stderr(stderr)
        .start()
    {
        let _ = fs::remove_file(pid_file);
        return Err(error).context("daemonize Anvil");
    }
    let error = Command::new("anvil").args(anvil_args).exec();
    bail!("start daemonized Anvil: {error}")
}

#[cfg(not(unix))]
pub fn daemonize_anvil(
    _pid_file: &Path,
    _log_file: &Path,
    _working_directory: &Path,
    _anvil_args: &[String],
) -> Result<()> {
    bail!("persistent Anvil devnets require a Unix-like operating system")
}

fn spawn_daemonized_anvil(
    args: &[String],
    cwd: &Path,
    target_rpc_env_name: &str,
    log_path: &Path,
) -> Result<u32> {
    let pid_file = log_path.with_extension("pid");
    if fs::symlink_metadata(&pid_file).is_ok() {
        bail!(
            "Anvil daemon PID file already exists: {}",
            pid_file.display()
        );
    }
    let output = Command::new(std::env::current_exe().context("resolve v4hook executable")?)
        .arg("__devnet-anvil")
        .arg("--pid-file")
        .arg(&pid_file)
        .arg("--log-file")
        .arg(log_path)
        .arg("--working-directory")
        .arg(cwd)
        .arg("--")
        .args(args)
        .current_dir(cwd)
        .env_remove(target_rpc_env_name)
        .stdin(Stdio::null())
        .output()
        .context("start Anvil daemon launcher")?;
    if !output.status.success() {
        let _ = fs::remove_file(&pid_file);
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if detail.is_empty() {
            bail!("Anvil daemon launcher failed");
        }
        bail!("Anvil daemon launcher failed: {detail}");
    }
    for _ in 0..100 {
        if let Ok(raw) = fs::read_to_string(&pid_file)
            && let Ok(pid) = raw.trim().parse::<u32>()
        {
            fs::remove_file(&pid_file)
                .with_context(|| format!("remove Anvil PID file {}", pid_file.display()))?;
            return Ok(pid);
        }
        thread::sleep(Duration::from_millis(50));
    }
    let _ = fs::remove_file(&pid_file);
    bail!("Anvil daemon did not publish its PID")
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
    ensure_quiet_output(&mut args);
    let (process, output) = if options.persistent {
        let log_path = options
            .log_path
            .as_deref()
            .context("persistent Anvil requires a log path")?;
        let pid = spawn_daemonized_anvil(&args, cwd, target_rpc_env_name, log_path)?;
        (
            OwnedAnvilProcess::Daemon(pid),
            Arc::new(Mutex::new(String::new())),
        )
    } else {
        let mut command = Command::new("anvil");
        command
            .args(&args)
            .current_dir(cwd)
            .env_remove(target_rpc_env_name)
            .stdin(Stdio::null());
        configure_output(&mut command, options.log_path.as_deref())?;
        let mut child = command.spawn().context("start Anvil")?;
        let output = if options.log_path.is_none() {
            capture_output(&mut child)
        } else {
            Arc::new(Mutex::new(String::new()))
        };
        (OwnedAnvilProcess::Child(child), output)
    };
    let rpc_url = format!("http://127.0.0.1:{port}");
    let mut handle = AnvilHandle {
        rpc_url: rpc_url.clone(),
        sender: ANVIL_DEFAULT_SENDER.to_owned(),
        process: Some(process),
    };
    if let Err(error) = wait_for_rpc(&rpc_url, Duration::from_secs(20)) {
        handle.stop();
        let mut output = output
            .lock()
            .map(|value| value.trim().to_owned())
            .unwrap_or_default();
        if output.is_empty()
            && let Some(log_path) = &options.log_path
        {
            output = fs::read_to_string(log_path).unwrap_or_default();
            output.truncate(output.trim_end().len());
        }
        if contains_account_secrets(&output) {
            if let Some(log_path) = &options.log_path {
                let _ = fs::remove_file(log_path);
            }
            "Anvil emitted sensitive account material; output was suppressed"
                .clone_into(&mut output);
        } else {
            output = output.replace(target_rpc_url, "[REDACTED RPC URL]");
        }
        let context = if output.is_empty() {
            "failed to start Anvil".to_owned()
        } else {
            format!("failed to start Anvil: {output}")
        };
        return Err(error).context(context);
    }
    if let Some(log_path) = &options.log_path {
        let log = fs::read_to_string(log_path).unwrap_or_default();
        if contains_account_secrets(&log) {
            handle.stop();
            let _ = fs::remove_file(log_path);
            bail!("Anvil emitted sensitive account material; stopped Anvil and removed its log");
        }
    }
    if let Err(error) = verify_fork(
        &rpc_url,
        target_rpc_url,
        fork_block_number,
        expected_chain_id,
    ) {
        handle.stop();
        return Err(error).context("failed to configure Anvil fork");
    }
    Ok(handle)
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
    fn adds_quiet_output_without_overriding_an_explicit_choice() {
        let mut default = vec!["--code-size-limit".to_owned(), "40000".to_owned()];
        ensure_quiet_output(&mut default);
        assert_eq!(default.last().map(String::as_str), Some("--quiet"));

        for flag in ["--quiet", "-q", "--silent"] {
            let mut explicit = vec![flag.to_owned()];
            ensure_quiet_output(&mut explicit);
            assert_eq!(explicit, [flag]);
        }
    }

    #[test]
    fn detects_anvil_account_secret_banners() {
        assert!(contains_account_secrets("Private Keys\n(0) 0xsecret"));
        assert!(contains_account_secrets("Private Key: 0xsecret"));
        assert!(contains_account_secrets("Mnemonic: test test test"));
        assert!(!contains_account_secrets("eth_chainId\neth_blockNumber"));
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
                persistent: false,
            },
        )
        .unwrap();
        assert_eq!(anvil_accounts(&fork.rpc_url).unwrap().len(), 100);
        let rpc_url = fork.rpc_url.clone();
        let pid = fork.detach().unwrap();
        assert_eq!(chain_id(&rpc_url).unwrap(), 31337);
        assert_eq!(block_number(&rpc_url).unwrap(), 0);
        let log = fs::read_to_string(&log_path).unwrap();
        assert!(!contains_account_secrets(&log));

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
