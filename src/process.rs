use std::{
    collections::BTreeMap,
    path::Path,
    process::{Command, Stdio},
    time::Instant,
};

use anyhow::{Context, Result, bail};

#[derive(Debug, Clone)]
pub struct CommandResult {
    pub command: Vec<String>,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

pub fn run(
    command: &[String],
    cwd: impl AsRef<Path>,
    env: Option<&BTreeMap<String, String>>,
    inherit: bool,
) -> Result<CommandResult> {
    let (executable, args) = command
        .split_first()
        .context("cannot run an empty command")?;
    let started = Instant::now();
    let mut process = Command::new(executable);
    process.args(args).current_dir(cwd.as_ref());
    if let Some(env) = env {
        process.envs(env);
    }
    if inherit {
        let status = process
            .status()
            .with_context(|| format!("start {executable}"))?;
        Ok(CommandResult {
            command: command.to_vec(),
            exit_code: status.code().unwrap_or(1),
            stdout: String::new(),
            stderr: String::new(),
            duration_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
        })
    } else {
        process.stdin(Stdio::null());
        let output = process
            .output()
            .with_context(|| format!("start {executable}"))?;
        Ok(CommandResult {
            command: command.to_vec(),
            exit_code: output.status.code().unwrap_or(1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            duration_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
        })
    }
}

pub fn require_success(
    command: &[String],
    cwd: impl AsRef<Path>,
    env: Option<&BTreeMap<String, String>>,
    inherit: bool,
) -> Result<CommandResult> {
    let result = run(command, cwd, env, inherit)?;
    if result.exit_code != 0 {
        let detail = if result.stderr.trim().is_empty() {
            result.stdout.trim()
        } else {
            result.stderr.trim()
        };
        let executable = command.first().map_or("command", String::as_str);
        if detail.is_empty() {
            bail!("{executable} failed with exit code {}", result.exit_code);
        }
        bail!(
            "{executable} failed with exit code {}: {detail}",
            result.exit_code
        );
    }
    Ok(result)
}

pub fn command_exists(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub fn redact_command(command: &[String]) -> Vec<String> {
    const SECRET_FLAGS: [&str; 4] = [
        "--private-key",
        "--password",
        "--rpc-url",
        "--verifier-api-key",
    ];
    command
        .iter()
        .enumerate()
        .map(|(index, part)| {
            if index > 0 && SECRET_FLAGS.contains(&command[index - 1].as_str()) {
                return "[REDACTED]".to_owned();
            }
            if let Some((flag, _)) = part.split_once('=')
                && SECRET_FLAGS.contains(&flag)
            {
                return format!("{flag}=[REDACTED]");
            }
            part.clone()
        })
        .collect()
}

pub fn command(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_separate_and_equals_secrets() {
        assert_eq!(
            redact_command(&command(&[
                "forge",
                "script",
                "--rpc-url",
                "secret",
                "--password=x"
            ])),
            command(&[
                "forge",
                "script",
                "--rpc-url",
                "[REDACTED]",
                "--password=[REDACTED]"
            ])
        );
    }
}
