use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use anyhow::{Context, Result, bail};
use regex::escape;
use serde_json::{Value, json};

use crate::{
    model::{LoadedConfig, SlitherFinding, SlitherImpact, SlitherSummary},
    process::{CommandResult, run},
    util::{sha256_bytes, stable_json},
};

fn parse_impact(value: &str) -> Result<SlitherImpact> {
    match value.to_ascii_lowercase().as_str() {
        "informational" => Ok(SlitherImpact::Informational),
        "low" => Ok(SlitherImpact::Low),
        "medium" => Ok(SlitherImpact::Medium),
        "high" => Ok(SlitherImpact::High),
        _ => bail!("unsupported Slither impact: {value}"),
    }
}

fn fail_flag(impact: SlitherImpact) -> &'static str {
    match impact {
        SlitherImpact::Informational => "--fail-pedantic",
        SlitherImpact::Low => "--fail-low",
        SlitherImpact::Medium => "--fail-medium",
        SlitherImpact::High => "--fail-high",
    }
}

fn source_locations(detector: &Value) -> Vec<(String, Vec<u64>)> {
    let mut locations = detector
        .get("elements")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|element| element.get("source_mapping"))
        .map(|mapping| {
            let path = mapping
                .get("filename_relative")
                .or_else(|| mapping.get("filename_short"))
                .or_else(|| mapping.get("filename_absolute"))
                .and_then(Value::as_str)
                .unwrap_or("<unknown>")
                .replace('\\', "/");
            let lines = mapping
                .get("lines")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_u64)
                .collect::<Vec<_>>();
            (path, lines)
        })
        .collect::<Vec<_>>();
    locations.sort();
    locations.dedup();
    if locations.is_empty() {
        locations.push(("<unknown>".to_owned(), Vec::new()));
    }
    locations
}

fn parse_findings(output: &str, allowed: &BTreeSet<String>) -> Result<Vec<SlitherFinding>> {
    let payload: Value = serde_json::from_str(output).context("parse Slither JSON output")?;
    if payload.get("success").and_then(Value::as_bool) != Some(true) {
        bail!("Slither reported an unsuccessful analysis");
    }
    let detectors = payload
        .pointer("/results/detectors")
        .and_then(Value::as_array)
        .context("Slither JSON is missing results.detectors")?;
    let mut findings = BTreeMap::new();
    for detector in detectors {
        let check = detector
            .get("check")
            .and_then(Value::as_str)
            .context("Slither finding is missing check")?
            .to_owned();
        let impact = parse_impact(
            detector
                .get("impact")
                .and_then(Value::as_str)
                .context("Slither finding is missing impact")?,
        )?;
        let confidence = detector
            .get("confidence")
            .and_then(Value::as_str)
            .unwrap_or("Unknown")
            .to_owned();
        let locations = source_locations(detector);
        let fingerprint = sha256_bytes(stable_json(&json!({
            "check": check,
            "impact": impact.as_str(),
            "locations": locations,
        }))?);
        let (path, lines) = locations[0].clone();
        findings.insert(
            fingerprint.clone(),
            SlitherFinding {
                allowed: allowed.contains(&fingerprint),
                fingerprint,
                check,
                impact,
                confidence,
                path,
                lines,
            },
        );
    }
    Ok(findings.into_values().collect())
}

pub fn run_slither(config: &LoadedConfig) -> Result<(CommandResult, SlitherSummary)> {
    let policy = &config.value.checks.slither_policy;
    let allowed = policy
        .allowed_findings
        .iter()
        .map(|finding| finding.fingerprint.clone())
        .collect::<BTreeSet<_>>();
    let mut command = config.value.checks.static_analysis.clone();
    if !policy.dependency_paths.is_empty() {
        let filter = policy
            .dependency_paths
            .iter()
            .map(|path| escape(path))
            .collect::<Vec<_>>()
            .join("|");
        command.extend(["--filter-paths".to_owned(), format!("({filter})")]);
    }
    command.extend([
        "--json".to_owned(),
        "-".to_owned(),
        "--show-ignored-findings".to_owned(),
        fail_flag(policy.fail_on).to_owned(),
    ]);
    let result = run(&command, Path::new(&config.project_root), None, false)?;
    let findings = parse_findings(&result.stdout, &allowed)?;
    let observed = findings
        .iter()
        .map(|finding| finding.fingerprint.clone())
        .collect::<BTreeSet<_>>();
    let stale = allowed.difference(&observed).cloned().collect::<Vec<_>>();
    let rejected = findings
        .iter()
        .filter(|finding| {
            finding.impact >= policy.fail_on
                || (finding.impact >= policy.require_triage_on && !finding.allowed)
        })
        .collect::<Vec<_>>();
    if !rejected.is_empty() || !stale.is_empty() {
        let details = rejected
            .iter()
            .take(8)
            .map(|finding| {
                format!(
                    "{} {} {}:{} {}",
                    finding.impact.as_str(),
                    finding.check,
                    finding.path,
                    finding.lines.first().copied().unwrap_or(0),
                    finding.fingerprint
                )
            })
            .chain(
                stale
                    .iter()
                    .take(8)
                    .map(|fingerprint| format!("stale allowance {fingerprint}")),
            )
            .collect::<Vec<_>>()
            .join("; ");
        bail!("Slither policy rejected findings: {details}");
    }
    if result.exit_code != 0 {
        bail!(
            "Slither exited with code {} without a policy-rejected finding",
            result.exit_code
        );
    }
    let allowed_findings = u64::try_from(findings.iter().filter(|item| item.allowed).count())?;
    Ok((
        result,
        SlitherSummary {
            findings,
            allowed_findings,
            untriaged_findings: 0,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_include_detector_and_every_source_location() {
        let payload = json!({
            "success": true,
            "results": {"detectors": [{
                "check": "timestamp",
                "impact": "Low",
                "confidence": "Medium",
                "elements": [{"source_mapping": {
                    "filename_relative": "src/Hook.sol",
                    "lines": [12, 13]
                }}]
            }]}
        });
        let findings = parse_findings(&payload.to_string(), &BTreeSet::new()).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].check, "timestamp");
        assert_eq!(findings[0].path, "src/Hook.sol");
        assert_eq!(findings[0].lines, [12, 13]);
        assert!(findings[0].fingerprint.starts_with("sha256:"));
        assert!(!findings[0].allowed);
    }
}
