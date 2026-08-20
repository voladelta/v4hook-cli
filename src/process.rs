use std::{
    collections::BTreeMap,
    path::Path,
    process::{Command, Stdio},
    time::Instant,
};

use anyhow::{Context, Result, bail};
use regex::Regex;
use serde_json::Value;

use crate::model::FoundryTestSummary;

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

#[derive(Debug, Clone, Copy)]
pub struct FoundryTestRequirements {
    pub kind: FoundryTestKind,
    pub minimum_fuzz_runs: Option<u64>,
    pub minimum_invariant_runs: Option<u64>,
    pub minimum_invariant_depth: Option<u64>,
}

impl FoundryTestRequirements {
    pub const fn kind(kind: FoundryTestKind) -> Self {
        Self {
            kind,
            minimum_fuzz_runs: None,
            minimum_invariant_runs: None,
            minimum_invariant_depth: None,
        }
    }
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
    require_zero_exit(command, result)
}

fn require_zero_exit(command: &[String], result: CommandResult) -> Result<CommandResult> {
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

fn identical_foundry_snapshot_diff_count(result: &CommandResult) -> Option<usize> {
    if result.exit_code != 1 || !result.stderr.trim().is_empty() {
        return None;
    }

    let aggregate_summary = Regex::new(
        r"^Ran [0-9]+ test suites? .+: [0-9]+ tests? passed, ([0-9]+) failed, [0-9]+ skipped \([0-9]+ total tests\)$",
    )
    .expect("valid Foundry summary regex");
    let lines = result.stdout.lines().map(str::trim).collect::<Vec<_>>();
    if lines
        .iter()
        .any(|line| line.starts_with("[FAIL") || line.starts_with("Suite result: FAILED"))
    {
        return None;
    }
    let summaries = lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            aggregate_summary
                .captures(line)
                .map(|captures| (index, captures))
        })
        .collect::<Vec<_>>();
    let [(summary_index, summary)] = summaries.as_slice() else {
        return None;
    };
    if summary.get(1)?.as_str() != "0" {
        return None;
    }

    let diff =
        Regex::new(r#"^Diff in \"[^\"]+\": consumed \"([^\"]*)\" gas, expected \"([^\"]*)\" gas$"#)
            .expect("valid Foundry snapshot diff regex");
    let mut count = 0;
    for line in lines
        .iter()
        .skip(summary_index + 1)
        .filter(|line| !line.is_empty())
    {
        let captures = diff.captures(line)?;
        if captures.get(1)?.as_str() != captures.get(2)?.as_str() {
            return None;
        }
        count += 1;
    }
    (count > 0).then_some(count)
}

pub fn require_gas_snapshot(
    command: &[String],
    cwd: impl AsRef<Path>,
    env: Option<&BTreeMap<String, String>>,
) -> Result<(CommandResult, usize)> {
    let result = run(command, cwd, env, false)?;
    if result.exit_code == 0 {
        return Ok((result, 0));
    }
    if let Some(count) = identical_foundry_snapshot_diff_count(&result) {
        return Ok((result, count));
    }
    require_zero_exit(command, result).map(|result| (result, 0))
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

fn update_minimum(current: &mut Option<u64>, value: u64) {
    *current = Some(current.map_or(value, |minimum| minimum.min(value)));
}

fn record_foundry_kind(
    kind: &serde_json::Map<String, Value>,
    summary: &mut FoundryTestSummary,
    requirements: FoundryTestRequirements,
) -> Result<()> {
    summary.total += 1;
    if kind.contains_key("Unit") {
        summary.unit += 1;
        return Ok(());
    }
    if let Some(fuzz) = kind.get("Fuzz") {
        summary.fuzz += 1;
        let runs = fuzz
            .get("runs")
            .and_then(Value::as_u64)
            .context("fuzz test result is missing its run count")?;
        if let Some(minimum) = requirements.minimum_fuzz_runs
            && runs < minimum
        {
            bail!("fuzz test ran {runs} cases; configured minimum is {minimum}");
        }
        update_minimum(&mut summary.minimum_fuzz_runs, runs);
        return Ok(());
    }
    if let Some(invariant) = kind.get("Invariant") {
        summary.invariant += 1;
        let runs = invariant
            .get("runs")
            .and_then(Value::as_u64)
            .context("invariant test result is missing its run count")?;
        let calls = invariant
            .get("calls")
            .and_then(Value::as_u64)
            .context("invariant test result is missing its call count")?;
        let reverts = invariant
            .get("reverts")
            .and_then(Value::as_u64)
            .context("invariant test result is missing its revert count")?;
        if let Some(minimum) = requirements.minimum_invariant_runs
            && runs < minimum
        {
            bail!("invariant test ran {runs} campaigns; configured minimum is {minimum}");
        }
        if let Some(depth) = requirements.minimum_invariant_depth {
            let minimum_calls = runs
                .checked_mul(depth)
                .context("configured invariant workload overflows u64")?;
            if calls < minimum_calls {
                bail!(
                    "invariant test made {calls} calls across {runs} campaigns; configured minimum depth {depth} requires at least {minimum_calls} calls"
                );
            }
        }
        update_minimum(&mut summary.minimum_invariant_runs, runs);
        update_minimum(&mut summary.minimum_invariant_calls, calls);
        summary.invariant_reverts = summary
            .invariant_reverts
            .checked_add(reverts)
            .context("invariant revert count overflows u64")?;
        return Ok(());
    }
    bail!("forge test result contains an unknown test kind")
}

fn foundry_test_summary(
    stdout: &str,
    requirements: FoundryTestRequirements,
) -> Result<FoundryTestSummary> {
    if stdout.contains("No tests found") {
        bail!("forge test matched no tests");
    }
    let report: Value = serde_json::from_str(stdout)
        .context("decode forge test --json output; the command may have matched no tests")?;
    let suites = report
        .as_object()
        .context("forge test --json returned an unexpected report")?;
    let mut summary = FoundryTestSummary {
        total: 0,
        unit: 0,
        fuzz: 0,
        invariant: 0,
        minimum_fuzz_runs: None,
        minimum_invariant_runs: None,
        minimum_invariant_calls: None,
        invariant_reverts: 0,
    };
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
                bail!(
                    "forge test report contains a {status} test; skipped tests do not satisfy a gate"
                );
            }
            let kind = result
                .get("kind")
                .and_then(Value::as_object)
                .context("forge test result is missing its test kind")?;
            record_foundry_kind(kind, &mut summary, requirements)?;
        }
    }
    if summary.total == 0 {
        bail!("forge test matched no tests");
    }
    let matched_expected = match requirements.kind {
        FoundryTestKind::Any => true,
        FoundryTestKind::Unit => summary.unit > 0,
        FoundryTestKind::Fuzz => summary.fuzz > 0,
        FoundryTestKind::Invariant => summary.invariant > 0,
    };
    if !matched_expected {
        bail!(
            "{} gate did not execute any {} tests",
            requirements.kind.name(),
            requirements.kind.foundry_name().unwrap_or("matching")
        );
    }
    Ok(summary)
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
    requirements: FoundryTestRequirements,
) -> Result<(CommandResult, FoundryTestSummary)> {
    validate_foundry_test_command(command, requirements.kind.name())?;
    let mut report_command = command.to_vec();
    if !report_command.iter().any(|part| part == "--json") {
        let position = report_command
            .iter()
            .position(|part| part == "--")
            .unwrap_or(report_command.len());
        report_command.insert(position, "--json".to_owned());
    }
    let result = require_success(&report_command, cwd, env, false)?;
    let summary = foundry_test_summary(&result.stdout, requirements)?;
    Ok((result, summary))
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
            foundry_test_summary(
                "No tests found in project!",
                FoundryTestRequirements::kind(FoundryTestKind::Any),
            )
            .unwrap_err()
            .to_string()
            .contains("matched no tests")
        );
        assert!(
            foundry_test_summary("{}", FoundryTestRequirements::kind(FoundryTestKind::Any),)
                .unwrap_err()
                .to_string()
                .contains("matched no tests")
        );
    }

    #[test]
    fn reports_foundry_test_workloads() {
        let report = r#"{
            "test/Hook.t.sol:HookTest": {
                "test_results": {
                    "test_unit()": {"status": "Success", "kind": {"Unit": {"gas": 1}}},
                    "test_fuzz(uint256)": {"status": "Success", "kind": {"Fuzz": {"runs": 256}}},
                    "invariant_balances()": {"status": "Success", "kind": {"Invariant": {"runs": 256, "calls": 128000, "reverts": 7}}}
                }
            }
        }"#;
        let summary =
            foundry_test_summary(report, FoundryTestRequirements::kind(FoundryTestKind::Any))
                .unwrap();
        assert_eq!(summary.total, 3);
        assert_eq!(summary.unit, 1);
        assert_eq!(summary.fuzz, 1);
        assert_eq!(summary.invariant, 1);
        assert_eq!(summary.minimum_fuzz_runs, Some(256));
        assert_eq!(summary.minimum_invariant_runs, Some(256));
        assert_eq!(summary.minimum_invariant_calls, Some(128_000));
        assert_eq!(summary.invariant_reverts, 7);
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
            foundry_test_summary(report, FoundryTestRequirements::kind(FoundryTestKind::Any),)
                .unwrap_err()
                .to_string()
                .contains("Failure")
        );
    }

    #[test]
    fn rejects_skipped_tests() {
        let report = r#"{
            "test/Fork.t.sol:ForkTest": {
                "test_results": {
                    "setUp()": {"status": "Skipped", "kind": {"Unit": {"gas": 0}}}
                }
            }
        }"#;
        let error =
            foundry_test_summary(report, FoundryTestRequirements::kind(FoundryTestKind::Any))
                .unwrap_err()
                .to_string();
        assert!(error.contains("Skipped"));
        assert!(error.contains("do not satisfy a gate"));
    }

    #[test]
    fn rejects_weakened_fuzz_and_invariant_workloads() {
        let fuzz = r#"{
            "test/Fuzz.t.sol:FuzzTest": {"test_results": {
                "testFuzz_value(uint256)": {"status": "Success", "kind": {"Fuzz": {"runs": 999}}}
            }}
        }"#;
        let fuzz_error = foundry_test_summary(
            fuzz,
            FoundryTestRequirements {
                kind: FoundryTestKind::Fuzz,
                minimum_fuzz_runs: Some(1_000),
                minimum_invariant_runs: None,
                minimum_invariant_depth: None,
            },
        )
        .unwrap_err()
        .to_string();
        assert!(fuzz_error.contains("999"));
        assert!(fuzz_error.contains("1000"));

        let invariant = r#"{
            "test/Invariant.t.sol:InvariantTest": {"test_results": {
                "invariant_value()": {"status": "Success", "kind": {"Invariant": {
                    "runs": 256, "calls": 127999, "reverts": 0
                }}}
            }}
        }"#;
        let invariant_error = foundry_test_summary(
            invariant,
            FoundryTestRequirements {
                kind: FoundryTestKind::Invariant,
                minimum_fuzz_runs: None,
                minimum_invariant_runs: Some(256),
                minimum_invariant_depth: Some(500),
            },
        )
        .unwrap_err()
        .to_string();
        assert!(invariant_error.contains("127999"));
        assert!(invariant_error.contains("128000"));
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

    fn snapshot_result(exit_code: i32, stdout: &str, stderr: &str) -> CommandResult {
        CommandResult {
            command: command(&["forge", "snapshot", "--check"]),
            exit_code,
            stdout: stdout.to_owned(),
            stderr: stderr.to_owned(),
            duration_ms: 1,
        }
    }

    #[test]
    fn accepts_only_byte_identical_foundry_snapshot_false_diffs() {
        let output = r#"Ran 2 test suites in 1.00s (2.00s CPU time): 4 tests passed, 0 failed, 1 skipped (5 total tests)
Diff in "ForkTest::testRequiresFork()": consumed "(gas: 0)" gas, expected "(gas: 0)" gas
Diff in "InvariantTest::invariant_Solvent()": consumed "(runs: 256, calls: 128000, reverts: 0)" gas, expected "(runs: 256, calls: 128000, reverts: 0)" gas
"#;
        assert_eq!(
            identical_foundry_snapshot_diff_count(&snapshot_result(1, output, "")),
            Some(2)
        );

        let changed = output.replace("expected \"(gas: 0)\"", "expected \"(gas: 1)\"");
        assert_eq!(
            identical_foundry_snapshot_diff_count(&snapshot_result(1, &changed, "")),
            None
        );
        assert_eq!(
            identical_foundry_snapshot_diff_count(&snapshot_result(2, output, "")),
            None
        );
        assert_eq!(
            identical_foundry_snapshot_diff_count(&snapshot_result(1, output, "forge error")),
            None
        );
    }

    #[test]
    fn rejects_snapshot_false_diffs_without_a_clean_test_summary() {
        let output = r#"Ran 1 test suite in 1.00s: 0 tests passed, 1 failed, 0 skipped (1 total tests)
Diff in "HookTest::testGas()": consumed "(gas: 1)" gas, expected "(gas: 1)" gas
"#;
        assert_eq!(
            identical_foundry_snapshot_diff_count(&snapshot_result(1, output, "")),
            None
        );

        let spoofed = format!(
            "{output}Ran 1 test suite in 1.00s: 1 test passed, 0 failed, 0 skipped (1 total tests)\n{}",
            "Diff in \"HookTest::testGas()\": consumed \"(gas: 1)\" gas, expected \"(gas: 1)\" gas"
        );
        assert_eq!(
            identical_foundry_snapshot_diff_count(&snapshot_result(1, &spoofed, "")),
            None
        );
    }
}
