use std::{
    collections::BTreeMap,
    path::Path,
    process::{Command, Stdio},
    time::Instant,
};

use anyhow::{Context, Result, bail};
use serde_json::Value;

const SECRET_FLAGS: [&str; 5] = [
    "--private-key",
    "--password",
    "--rpc-url",
    "--fork-url",
    "--verifier-api-key",
];

#[derive(Debug, Clone)]
pub struct CommandResult {
    pub command: Vec<String>,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoundryTestKind {
    Any,
    Unit,
    Fuzz,
    Invariant,
}

impl FoundryTestKind {
    const fn name(self) -> &'static str {
        match self {
            Self::Any => "test",
            Self::Unit => "unit test",
            Self::Fuzz => "fuzz test",
            Self::Invariant => "invariant test",
        }
    }

    const fn foundry_name(self) -> Option<&'static str> {
        match self {
            Self::Any => None,
            Self::Unit => Some("Unit"),
            Self::Fuzz => Some("Fuzz"),
            Self::Invariant => Some("Invariant"),
        }
    }
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
        let detail = redact_command_output(command, detail);
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

pub fn validate_foundry_test_command(command: &[String], label: &str) -> Result<()> {
    let executable = command
        .first()
        .and_then(|value| Path::new(value).file_name())
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if executable != "forge" || command.get(1).map(String::as_str) != Some("test") {
        bail!("{label} must run `forge test`");
    }
    if command.iter().any(|part| part == "--allow-failure") {
        bail!("{label} cannot use forge test --allow-failure");
    }
    if command.iter().any(|part| part == "--md") {
        bail!("{label} cannot use forge test --md");
    }
    Ok(())
}

fn foundry_test_kinds(stdout: &str) -> Result<BTreeMap<String, usize>> {
    if stdout.contains("No tests found") {
        bail!("forge test matched no tests");
    }
    let report: Value = serde_json::from_str(stdout)
        .context("decode forge test --json output; the command may have matched no tests")?;
    let suites = report
        .as_object()
        .context("forge test --json returned an unexpected report")?;
    let mut kinds = BTreeMap::new();
    for suite in suites.values() {
        let Some(results) = suite.get("test_results").and_then(Value::as_object) else {
            continue;
        };
        for result in results.values() {
            let status = result
                .get("status")
                .and_then(Value::as_str)
                .context("forge test result is missing its status")?;
            if status != "Success" {
                bail!("forge test report contains a {status} test");
            }
            let kind = result
                .get("kind")
                .and_then(Value::as_object)
                .and_then(|value| value.keys().next())
                .context("forge test result is missing its test kind")?;
            *kinds.entry(kind.clone()).or_insert(0) += 1;
        }
    }
    if kinds.values().sum::<usize>() == 0 {
        bail!("forge test matched no tests");
    }
    Ok(kinds)
}

fn redact_command_output(command: &[String], output: &str) -> String {
    let mut redacted = output.to_owned();
    for (index, part) in command.iter().enumerate() {
        if SECRET_FLAGS.contains(&part.as_str()) {
            if let Some(secret) = command.get(index + 1)
                && !secret.is_empty()
            {
                redacted = redacted.replace(secret, "[REDACTED]");
            }
        } else if let Some((flag, secret)) = part.split_once('=')
            && SECRET_FLAGS.contains(&flag)
            && !secret.is_empty()
        {
            redacted = redacted.replace(secret, "[REDACTED]");
        }
    }
    redacted
}

pub fn require_foundry_tests(
    command: &[String],
    cwd: impl AsRef<Path>,
    env: Option<&BTreeMap<String, String>>,
    kind: FoundryTestKind,
) -> Result<CommandResult> {
    validate_foundry_test_command(command, kind.name())?;
    let mut report_command = command.to_vec();
    if !report_command.iter().any(|part| part == "--json") {
        let position = report_command
            .iter()
            .position(|part| part == "--")
            .unwrap_or(report_command.len());
        report_command.insert(position, "--json".to_owned());
    }
    let result = require_success(&report_command, cwd, env, false)?;
    let kinds = foundry_test_kinds(&result.stdout)?;
    if let Some(expected) = kind.foundry_name()
        && !kinds.contains_key(expected)
    {
        bail!("{} gate did not execute any {expected} tests", kind.name());
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
        assert_eq!(
            redact_command_output(
                &command(&["forge", "script", "--rpc-url", "https://secret"]),
                "request to https://secret failed"
            ),
            "request to [REDACTED] failed"
        );
    }

    #[test]
    fn rejects_empty_foundry_test_reports() {
        assert!(
            foundry_test_kinds("No tests found in project!")
                .unwrap_err()
                .to_string()
                .contains("matched no tests")
        );
        assert!(
            foundry_test_kinds("{}")
                .unwrap_err()
                .to_string()
                .contains("matched no tests")
        );
    }

    #[test]
    fn counts_foundry_test_kinds() {
        let report = r#"{
            "test/Hook.t.sol:HookTest": {
                "test_results": {
                    "test_unit()": {"status": "Success", "kind": {"Unit": {"gas": 1}}},
                    "test_fuzz(uint256)": {"status": "Success", "kind": {"Fuzz": {"runs": 256}}},
                    "invariant_balances()": {"status": "Success", "kind": {"Invariant": {"runs": 256}}}
                }
            }
        }"#;
        let kinds = foundry_test_kinds(report).unwrap();
        assert_eq!(kinds.get("Unit"), Some(&1));
        assert_eq!(kinds.get("Fuzz"), Some(&1));
        assert_eq!(kinds.get("Invariant"), Some(&1));
    }

    #[test]
    fn rejects_failed_tests_even_if_forge_allows_failure() {
        let report = r#"{
            "test/Hook.t.sol:HookTest": {
                "test_results": {
                    "test_failure()": {"status": "Failure", "kind": {"Unit": {"gas": 1}}}
                }
            }
        }"#;
        assert!(
            foundry_test_kinds(report)
                .unwrap_err()
                .to_string()
                .contains("Failure")
        );
    }

    #[test]
    fn only_accepts_strict_forge_test_commands() {
        validate_foundry_test_command(&command(&["forge", "test"]), "test").unwrap();
        assert!(validate_foundry_test_command(&command(&["true"]), "test").is_err());
        assert!(
            validate_foundry_test_command(&command(&["forge", "test", "--allow-failure"]), "test")
                .is_err()
        );
    }
}
