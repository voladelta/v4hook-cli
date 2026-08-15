use std::{
    io::Read,
    net::TcpListener,
    path::Path,
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
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for AnvilHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

fn reject_reserved_args(args: &[String]) -> Result<()> {
    const RESERVED: [&str; 5] = [
        "--fork-url",
        "--fork-block-number",
        "--host",
        "--port",
        "--chain-id",
    ];
    for argument in args {
        if let Some(flag) = RESERVED
            .iter()
            .find(|flag| argument == **flag || argument.starts_with(&format!("{flag}=")))
        {
            bail!("anvilArgs cannot override CLI-controlled {flag}");
        }
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
    reject_reserved_args(extra_args)?;
    let listener = TcpListener::bind("127.0.0.1:0").context("allocate local Anvil port")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    let args = [
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
    let mut child = Command::new("anvil")
        .args(&args)
        .current_dir(cwd)
        .env_remove(target_rpc_env_name)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("start Anvil")?;
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
    let rpc_url = format!("http://127.0.0.1:{port}");
    if let Err(error) = wait_for_rpc(&rpc_url, Duration::from_secs(20)) {
        let _ = child.kill();
        let _ = child.wait();
        let output = output
            .lock()
            .map(|value| value.trim().to_owned())
            .unwrap_or_default();
        let output = output.replace(target_rpc_url, "[REDACTED RPC URL]");
        if output.is_empty() {
            bail!("failed to start Anvil: {error}");
        }
        bail!("failed to start Anvil: {error}: {output}");
    }
    let configure_fork = || -> Result<()> {
        reset_fork(&rpc_url, target_rpc_url, fork_block_number)?;
        if chain_id(&rpc_url)? != expected_chain_id {
            bail!("local Anvil chain ID does not match the target network");
        }
        if block_number(&rpc_url)? != fork_block_number {
            bail!("local Anvil did not reset to the pinned fork block");
        }
        if block_hash(&rpc_url, fork_block_number)?
            != block_hash(target_rpc_url, fork_block_number)?
        {
            bail!("local Anvil fork block hash does not match the target network");
        }
        Ok(())
    };
    if let Err(error) = configure_fork() {
        let _ = child.kill();
        let _ = child.wait();
        let error = error
            .to_string()
            .replace(target_rpc_url, "[REDACTED RPC URL]");
        bail!("failed to configure Anvil fork: {error}");
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
