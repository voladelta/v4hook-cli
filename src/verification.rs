use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    checks::run_check_suite,
    config::load_config,
    model::{CheckEvidence, LoadedConfig, SourceIdentity, ToolchainIdentity},
    plan::{current_toolchain, source_identity},
    process::{command, require_success},
    util::{
        assert_digest, calculate_digest, now_iso, read_json, resolve_from, sha256_bytes,
        sha256_file, stable_json, write_json,
    },
};

pub const DEFAULT_STATE_PATH: &str = ".v4hook/verification-state.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum VerificationGate {
    Unit,
    Fuzz,
    Invariant,
}

impl VerificationGate {
    const fn as_str(&self) -> &'static str {
        match self {
            Self::Unit => "unit",
            Self::Fuzz => "fuzz",
            Self::Invariant => "invariant",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationTest {
    pub gate: VerificationGate,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationInvariant {
    pub id: String,
    pub requirement: String,
    #[serde(default)]
    pub tests: Vec<VerificationTest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_gap: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationContract {
    pub schema_version: String,
    pub invariants: Vec<VerificationInvariant>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum VerificationStage {
    Frozen,
    FirstGreen,
    Reviewed,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationRun {
    pub created_at: String,
    pub checks: Vec<CheckEvidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationReview {
    pub created_at: String,
    pub report_path: String,
    pub report_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ReviewDecision {
    ReviewerClean,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChiefAdjudication {
    pub decision: ReviewDecision,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ReviewFindingDisposition {
    Resolved,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationReviewFinding {
    pub id: String,
    pub summary: String,
    pub disposition: ReviewFindingDisposition,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationReviewReport {
    pub schema_version: String,
    pub candidate_source: SourceIdentity,
    pub frozen_baseline: SourceIdentity,
    pub verification_contract_digest: String,
    pub checks_digest: String,
    pub foundry_config_digest: String,
    pub chief_adjudication: ChiefAdjudication,
    pub findings: Vec<VerificationReviewFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationCandidate {
    pub source: SourceIdentity,
    pub first_green: VerificationRun,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<VerificationReview>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub second_green: Option<VerificationRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationState {
    pub schema_version: String,
    pub created_at: String,
    pub updated_at: String,
    pub project_root: String,
    pub cli_path: String,
    pub cli_digest: String,
    pub toolchain: ToolchainIdentity,
    pub config_path: String,
    pub contract_path: String,
    pub contract_digest: String,
    pub checks_digest: String,
    pub foundry_config_digest: String,
    pub baseline: SourceIdentity,
    pub stage: VerificationStage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate: Option<VerificationCandidate>,
    pub digest: String,
}

fn validate_contract(contract: &VerificationContract) -> Result<()> {
    if contract.schema_version != "v4hook.verification-contract.v1" {
        bail!(
            "unsupported verification contract schemaVersion: {}",
            contract.schema_version
        );
    }
    if contract.invariants.is_empty() {
        bail!("verification contract must contain at least one invariant");
    }
    let mut ids = BTreeSet::new();
    for invariant in &contract.invariants {
        if invariant.id.trim().is_empty() || invariant.requirement.trim().is_empty() {
            bail!("verification invariant id and requirement must be non-empty");
        }
        if !ids.insert(invariant.id.clone()) {
            bail!("duplicate verification invariant id: {}", invariant.id);
        }
        let external_gap = invariant
            .external_gap
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
        if invariant.tests.is_empty() != external_gap {
            bail!(
                "verification invariant {} must have tests or one explicit externalGap, but not both",
                invariant.id
            );
        }
        let mut tests = BTreeSet::new();
        for test in &invariant.tests {
            if test.name.trim().is_empty() {
                bail!(
                    "verification invariant {} has an empty test name",
                    invariant.id
                );
            }
            if !tests.insert((test.gate.clone(), test.name.clone())) {
                bail!(
                    "verification invariant {} duplicates test mapping: {} {}",
                    invariant.id,
                    test.gate.as_str(),
                    test.name
                );
            }
        }
    }
    Ok(())
}

fn foundry_config_digest(project_root: &Path) -> Result<String> {
    let result = require_success(
        &command(&["forge", "config", "--json"]),
        project_root,
        None,
        false,
    )?;
    let value: Value = serde_json::from_str(&result.stdout).context("parse forge config --json")?;
    Ok(sha256_bytes(stable_json(&value)?))
}

fn current_cli() -> Result<(String, String)> {
    let path = fs::canonicalize(std::env::current_exe().context("resolve current v4hook binary")?)
        .context("resolve current v4hook binary path")?;
    Ok((path.to_string_lossy().into_owned(), sha256_file(&path)?))
}

fn require_frozen_toolchain(project_root: &Path, state: &VerificationState) -> Result<()> {
    let (cli_path, cli_digest) = current_cli()?;
    if cli_path != state.cli_path || cli_digest != state.cli_digest {
        bail!("verification lifecycle must use the exact v4hook binary frozen at baseline");
    }
    if current_toolchain(project_root)? != state.toolchain {
        bail!("verification toolchain changed after freeze");
    }
    Ok(())
}

fn checks_digest(config: &LoadedConfig) -> Result<String> {
    Ok(sha256_bytes(stable_json(&config.value.checks)?))
}

fn canonical_project_root(config: &LoadedConfig) -> Result<PathBuf> {
    fs::canonicalize(&config.project_root)
        .with_context(|| format!("resolve project root {}", config.project_root))
}

fn repository_relative_path(project_root: &Path, path: &Path, label: &str) -> Result<String> {
    let path =
        fs::canonicalize(path).with_context(|| format!("resolve {label} {}", path.display()))?;
    let relative = path.strip_prefix(project_root).with_context(|| {
        format!(
            "{label} must be inside project root {}",
            project_root.display()
        )
    })?;
    relative
        .to_str()
        .map(str::to_owned)
        .context("project paths must be UTF-8")
}

fn require_repository_root(project_root: &Path) -> Result<()> {
    let result = require_success(
        &command(&["git", "rev-parse", "--show-toplevel"]),
        project_root,
        None,
        false,
    )?;
    let actual = fs::canonicalize(result.stdout.trim()).context("resolve Git worktree root")?;
    if actual != project_root {
        bail!(
            "project root {} must be the Git worktree root {}",
            project_root.display(),
            actual.display()
        );
    }
    Ok(())
}

fn require_tracked(project_root: &Path, relative: &str, label: &str) -> Result<()> {
    require_success(
        &[
            "git".to_owned(),
            "ls-files".to_owned(),
            "--error-unmatch".to_owned(),
            "--".to_owned(),
            relative.to_owned(),
        ],
        project_root,
        None,
        false,
    )
    .with_context(|| format!("{label} must be tracked by Git before verification freeze"))?;
    Ok(())
}

fn write_state(path: &Path, state: &mut VerificationState) -> Result<()> {
    state.updated_at = now_iso();
    state.digest = calculate_digest(state)?;
    write_json(path, state)
}

fn load_state(path: &Path) -> Result<VerificationState> {
    let state: VerificationState = read_json(path)?;
    if state.schema_version != "v4hook.verification-state.v1" {
        bail!(
            "unsupported verification state schemaVersion: {}",
            state.schema_version
        );
    }
    assert_digest(&state, &state.digest, "verification state")?;
    Ok(state)
}

fn load_contract(project_root: &Path, state: &VerificationState) -> Result<VerificationContract> {
    let path = project_root.join(&state.contract_path);
    if sha256_file(&path)? != state.contract_digest {
        bail!("verification contract changed after freeze");
    }
    let contract: VerificationContract = read_json(path)?;
    validate_contract(&contract)?;
    Ok(contract)
}

fn require_frozen_inputs(
    config: &LoadedConfig,
    state: &VerificationState,
) -> Result<(PathBuf, VerificationContract)> {
    let project_root = canonical_project_root(config)?;
    if project_root.to_string_lossy() != state.project_root {
        bail!("verification state belongs to a different project root");
    }
    require_frozen_toolchain(&project_root, state)?;
    let config_relative =
        repository_relative_path(&project_root, Path::new(&config.path), "configuration")?;
    if config_relative != state.config_path {
        bail!("verification state is bound to a different configuration file");
    }
    if checks_digest(config)? != state.checks_digest {
        bail!("configured verification workload changed after freeze");
    }
    if foundry_config_digest(&project_root)? != state.foundry_config_digest {
        bail!("effective Foundry configuration changed after freeze");
    }
    let contract = load_contract(&project_root, state)?;
    Ok((project_root, contract))
}

fn require_contract_evidence(
    contract: &VerificationContract,
    checks: &[CheckEvidence],
) -> Result<()> {
    let executed = checks
        .iter()
        .filter_map(|check| {
            check.test_summary.as_ref().map(|summary| {
                (
                    check.name.as_str(),
                    summary.tests.iter().collect::<BTreeSet<_>>(),
                )
            })
        })
        .collect::<BTreeMap<_, _>>();
    for invariant in &contract.invariants {
        for test in &invariant.tests {
            let gate = test.gate.as_str();
            let names = executed
                .get(gate)
                .with_context(|| format!("verification contract references missing {gate} gate"))?;
            if !names.contains(&test.name) {
                bail!(
                    "verification invariant {} expected {} test not executed by the frozen gate: {}",
                    invariant.id,
                    gate,
                    test.name
                );
            }
        }
    }
    Ok(())
}

fn record_green(state: &mut VerificationState, source: SourceIdentity, run: VerificationRun) {
    let candidate_matches = state
        .candidate
        .as_ref()
        .is_some_and(|candidate| candidate.source == source);
    match (&mut state.candidate, candidate_matches, &state.stage) {
        (Some(candidate), true, VerificationStage::Reviewed) => {
            candidate.second_green = Some(run);
            state.stage = VerificationStage::Complete;
        }
        (Some(candidate), true, VerificationStage::Complete) => {
            candidate.second_green = Some(run);
        }
        (Some(candidate), true, VerificationStage::FirstGreen) => {
            candidate.first_green = run;
        }
        _ => {
            state.candidate = Some(VerificationCandidate {
                source,
                first_green: run,
                review: None,
                second_green: None,
            });
            state.stage = VerificationStage::FirstGreen;
        }
    }
}

fn require_review_unchanged(
    state: &VerificationState,
    project_root: &Path,
    source: &SourceIdentity,
) -> Result<()> {
    let Some(candidate) = state
        .candidate
        .as_ref()
        .filter(|candidate| candidate.source == *source)
    else {
        return Ok(());
    };
    if !matches!(
        state.stage,
        VerificationStage::Reviewed | VerificationStage::Complete
    ) {
        return Ok(());
    }
    let review = candidate
        .review
        .as_ref()
        .context("reviewed verification state is missing review evidence")?;
    if sha256_file(resolve_from(project_root, &review.report_path))? != review.report_digest {
        bail!("structured review report changed after it was recorded");
    }
    Ok(())
}

fn validate_review_report(
    state: &VerificationState,
    candidate: &VerificationCandidate,
    report: &VerificationReviewReport,
) -> Result<()> {
    if report.schema_version != "v4hook.verification-review.v1" {
        bail!(
            "unsupported verification review schemaVersion: {}",
            report.schema_version
        );
    }
    if report.candidate_source != candidate.source {
        bail!("verification review candidateSource does not match the first-green candidate");
    }
    if report.frozen_baseline != state.baseline {
        bail!("verification review frozenBaseline does not match the frozen baseline");
    }
    for (label, actual, expected) in [
        (
            "verificationContractDigest",
            &report.verification_contract_digest,
            &state.contract_digest,
        ),
        ("checksDigest", &report.checks_digest, &state.checks_digest),
        (
            "foundryConfigDigest",
            &report.foundry_config_digest,
            &state.foundry_config_digest,
        ),
    ] {
        if actual != expected {
            bail!("verification review {label} does not match frozen verification state");
        }
    }
    if report.chief_adjudication.rationale.trim().is_empty() {
        bail!("verification review chiefAdjudication rationale must be non-empty");
    }
    let mut finding_ids = BTreeSet::new();
    for finding in &report.findings {
        if finding.id.trim().is_empty()
            || finding.summary.trim().is_empty()
            || finding.rationale.trim().is_empty()
        {
            bail!(
                "verification review findings require non-empty id, summary, disposition, and rationale"
            );
        }
        if !finding_ids.insert(&finding.id) {
            bail!("duplicate verification review finding id: {}", finding.id);
        }
    }
    Ok(())
}

fn bind_review(
    state: &mut VerificationState,
    source: &SourceIdentity,
    report: &VerificationReviewReport,
    report_path: String,
    report_digest: String,
) -> Result<()> {
    if state.stage != VerificationStage::FirstGreen {
        bail!("structured review requires a first-green candidate");
    }
    let candidate = state
        .candidate
        .as_ref()
        .context("first-green verification state is missing its candidate")?;
    if candidate.source != *source {
        bail!(
            "project source changed after first green; run verification check again before review"
        );
    }
    validate_review_report(state, candidate, report)?;
    let candidate = state
        .candidate
        .as_mut()
        .context("first-green verification state is missing its candidate")?;
    candidate.review = Some(VerificationReview {
        created_at: now_iso(),
        report_path,
        report_digest,
    });
    candidate.second_green = None;
    state.stage = VerificationStage::Reviewed;
    Ok(())
}

pub fn freeze(
    config: &LoadedConfig,
    contract_path: &Path,
    output_path: &Path,
) -> Result<VerificationState> {
    let project_root = canonical_project_root(config)?;
    require_repository_root(&project_root)?;
    let baseline = source_identity(&project_root)?;
    let config_relative =
        repository_relative_path(&project_root, Path::new(&config.path), "configuration")?;
    let contract_path = resolve_from(&project_root, contract_path);
    let contract_relative =
        repository_relative_path(&project_root, &contract_path, "verification contract")?;
    require_tracked(&project_root, &config_relative, "configuration")?;
    require_tracked(&project_root, &contract_relative, "verification contract")?;
    let contract: VerificationContract = read_json(&contract_path)?;
    validate_contract(&contract)?;
    let output_path = resolve_from(&project_root, output_path);
    if output_path.exists() {
        bail!(
            "verification state already exists at {}; preserve it or remove it only when intentionally abandoning that lifecycle",
            output_path.display()
        );
    }
    let timestamp = now_iso();
    let (cli_path, cli_digest) = current_cli()?;
    let mut state = VerificationState {
        schema_version: "v4hook.verification-state.v1".to_owned(),
        created_at: timestamp.clone(),
        updated_at: timestamp,
        project_root: project_root.to_string_lossy().into_owned(),
        cli_path,
        cli_digest,
        toolchain: current_toolchain(&project_root)?,
        config_path: config_relative,
        contract_path: contract_relative,
        contract_digest: sha256_file(contract_path)?,
        checks_digest: checks_digest(config)?,
        foundry_config_digest: foundry_config_digest(&project_root)?,
        baseline,
        stage: VerificationStage::Frozen,
        candidate: None,
        digest: String::new(),
    };
    write_state(&output_path, &mut state)?;
    Ok(state)
}

pub fn check(config: &LoadedConfig, state_file: &Path) -> Result<VerificationState> {
    let project_root = canonical_project_root(config)?;
    let state_file = resolve_from(&project_root, state_file);
    let mut state = load_state(&state_file)?;
    let loaded_state_digest = state.digest.clone();
    let (project_root, contract) = require_frozen_inputs(config, &state)?;
    require_repository_root(&project_root)?;
    let source = source_identity(&project_root)?;
    require_review_unchanged(&state, &project_root, &source)?;

    let checks = run_check_suite(config)?;
    require_contract_evidence(&contract, &checks)?;
    let refreshed_config = load_config(project_root.join(&state.config_path))?;
    require_frozen_inputs(&refreshed_config, &state)?;
    let final_source = source_identity(&project_root)?;
    if final_source != source {
        bail!("project source changed while the verification check was running");
    }
    require_review_unchanged(&state, &project_root, &final_source)?;
    if load_state(&state_file)?.digest != loaded_state_digest {
        bail!("verification state changed while the check was running");
    }
    let run = VerificationRun {
        created_at: now_iso(),
        checks,
    };
    record_green(&mut state, source, run);
    write_state(&state_file, &mut state)?;
    Ok(state)
}

pub fn review(state_file: &Path, report_file: &Path) -> Result<VerificationState> {
    let state_file = fs::canonicalize(state_file)
        .with_context(|| format!("resolve verification state {}", state_file.display()))?;
    let mut state = load_state(&state_file)?;
    let loaded_state_digest = state.digest.clone();
    if state.stage != VerificationStage::FirstGreen {
        bail!("structured review requires a first-green candidate");
    }
    let project_root = PathBuf::from(&state.project_root);
    require_repository_root(&project_root)?;
    require_frozen_toolchain(&project_root, &state)?;
    let source = source_identity(&project_root)?;
    let report_file = resolve_from(&project_root, report_file);
    let report_bytes = fs::read(&report_file)
        .with_context(|| format!("read structured review report {}", report_file.display()))?;
    let report: VerificationReviewReport = serde_json::from_slice(&report_bytes)
        .with_context(|| format!("parse structured review report {}", report_file.display()))?;
    let report_path = report_file
        .strip_prefix(&project_root)
        .unwrap_or(&report_file)
        .to_string_lossy()
        .into_owned();
    bind_review(
        &mut state,
        &source,
        &report,
        report_path,
        sha256_bytes(&report_bytes),
    )?;
    if load_state(&state_file)?.digest != loaded_state_digest {
        bail!("verification state changed while the review was being recorded");
    }
    write_state(&state_file, &mut state)?;
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FoundryTestSummary;

    fn contract() -> VerificationContract {
        VerificationContract {
            schema_version: "v4hook.verification-contract.v1".to_owned(),
            invariants: vec![VerificationInvariant {
                id: "permission-boundary".to_owned(),
                requirement: "Only the PoolManager can call hook callbacks.".to_owned(),
                tests: vec![VerificationTest {
                    gate: VerificationGate::Unit,
                    name: "test/Hook.t.sol:HookTest::testOnlyPoolManager()".to_owned(),
                }],
                external_gap: None,
            }],
        }
    }

    #[test]
    fn contract_requires_test_or_external_gap() {
        let mut value = contract();
        value.invariants[0].tests.clear();
        assert!(validate_contract(&value).is_err());
        value.invariants[0].external_gap = Some("requires a deployed hook".to_owned());
        assert!(validate_contract(&value).is_ok());
    }

    #[test]
    fn contract_rejects_test_and_external_gap_together() {
        let mut value = contract();
        value.invariants[0].external_gap = Some("not actually external".to_owned());
        assert!(validate_contract(&value).is_err());
    }

    #[test]
    fn contract_evidence_requires_the_exact_configured_test_name() {
        let checks = vec![CheckEvidence {
            name: "unit".to_owned(),
            command: Vec::new(),
            duration_ms: 0,
            stdout_hash: String::new(),
            stderr_hash: String::new(),
            test_summary: Some(FoundryTestSummary {
                total: 1,
                unit: 1,
                fuzz: 0,
                invariant: 0,
                minimum_fuzz_runs: None,
                minimum_invariant_runs: None,
                minimum_invariant_calls: None,
                invariant_reverts: 0,
                tests: vec!["test/Hook.t.sol:HookTest::testOnlyPoolManager()".to_owned()],
            }),
            slither_summary: None,
            code_size_summary: None,
        }];
        assert!(require_contract_evidence(&contract(), &checks).is_ok());
        let mut wrong = contract();
        wrong.invariants[0].tests[0].name = "HookTest::testOnlyPoolManager()".to_owned();
        assert!(require_contract_evidence(&wrong, &checks).is_err());
    }

    fn source(commit: &str) -> SourceIdentity {
        SourceIdentity {
            commit: commit.to_owned(),
            dirty: false,
            tree_digest: format!("sha256:{commit}"),
        }
    }

    fn run() -> VerificationRun {
        VerificationRun {
            created_at: "2026-08-20T00:00:00.000Z".to_owned(),
            checks: Vec::new(),
        }
    }

    fn report(
        state: &VerificationState,
        candidate_source: SourceIdentity,
    ) -> VerificationReviewReport {
        VerificationReviewReport {
            schema_version: "v4hook.verification-review.v1".to_owned(),
            candidate_source,
            frozen_baseline: state.baseline.clone(),
            verification_contract_digest: state.contract_digest.clone(),
            checks_digest: state.checks_digest.clone(),
            foundry_config_digest: state.foundry_config_digest.clone(),
            chief_adjudication: ChiefAdjudication {
                decision: ReviewDecision::ReviewerClean,
                rationale: "Every listed finding is resolved or rejected with evidence.".to_owned(),
            },
            findings: vec![VerificationReviewFinding {
                id: "review-1".to_owned(),
                summary: "Candidate initially lacked an exact-output regression.".to_owned(),
                disposition: ReviewFindingDisposition::Resolved,
                rationale: "The candidate now includes the focused regression.".to_owned(),
            }],
        }
    }

    fn state(stage: VerificationStage, candidate: VerificationCandidate) -> VerificationState {
        VerificationState {
            schema_version: "v4hook.verification-state.v1".to_owned(),
            created_at: String::new(),
            updated_at: String::new(),
            project_root: String::new(),
            cli_path: String::new(),
            cli_digest: String::new(),
            toolchain: ToolchainIdentity {
                node: String::new(),
                forge: String::new(),
                cast: String::new(),
                anvil: String::new(),
            },
            config_path: String::new(),
            contract_path: String::new(),
            contract_digest: String::new(),
            checks_digest: String::new(),
            foundry_config_digest: String::new(),
            baseline: source("baseline"),
            stage,
            candidate: Some(candidate),
            digest: String::new(),
        }
    }

    #[test]
    fn reviewed_source_completes_only_on_same_source_green() {
        let candidate = VerificationCandidate {
            source: source("candidate"),
            first_green: run(),
            review: Some(VerificationReview {
                created_at: String::new(),
                report_path: String::new(),
                report_digest: String::new(),
            }),
            second_green: None,
        };
        let mut value = state(VerificationStage::Reviewed, candidate);
        record_green(&mut value, source("candidate"), run());
        assert_eq!(value.stage, VerificationStage::Complete);
        assert!(value.candidate.unwrap().second_green.is_some());
    }

    #[test]
    fn structured_review_rejects_candidate_and_frozen_digest_mismatches() {
        let candidate = VerificationCandidate {
            source: source("candidate"),
            first_green: run(),
            review: None,
            second_green: None,
        };
        let value = state(VerificationStage::FirstGreen, candidate.clone());

        let mut wrong_candidate = report(&value, source("other"));
        assert!(validate_review_report(&value, &candidate, &wrong_candidate).is_err());
        wrong_candidate.candidate_source = candidate.source.clone();

        let mut wrong_baseline = wrong_candidate.clone();
        wrong_baseline.frozen_baseline = source("other-baseline");
        assert!(validate_review_report(&value, &candidate, &wrong_baseline).is_err());

        for field in ["contract", "checks", "foundry"] {
            let mut mismatched = wrong_candidate.clone();
            match field {
                "contract" => mismatched.verification_contract_digest = "sha256:other".to_owned(),
                "checks" => mismatched.checks_digest = "sha256:other".to_owned(),
                "foundry" => mismatched.foundry_config_digest = "sha256:other".to_owned(),
                _ => unreachable!(),
            }
            assert!(validate_review_report(&value, &candidate, &mismatched).is_err());
        }
    }

    #[test]
    fn structured_review_requires_finding_disposition_and_rationale() {
        let candidate = VerificationCandidate {
            source: source("candidate"),
            first_green: run(),
            review: None,
            second_green: None,
        };
        let value = state(VerificationStage::FirstGreen, candidate.clone());
        let mut missing_rationale = report(&value, candidate.source.clone());
        missing_rationale.findings[0].rationale = "  ".to_owned();
        assert!(validate_review_report(&value, &candidate, &missing_rationale).is_err());

        let raw = serde_json::to_string(&report(&value, candidate.source)).unwrap();
        let raw = raw.replace("\"disposition\":\"resolved\"", "\"disposition\":\"\"");
        assert!(serde_json::from_str::<VerificationReviewReport>(&raw).is_err());

        let raw = serde_json::to_string(&report(&value, source("candidate"))).unwrap();
        let raw = raw.replace(
            "\"decision\":\"reviewerClean\"",
            "\"decision\":\"needsFix\"",
        );
        assert!(serde_json::from_str::<VerificationReviewReport>(&raw).is_err());
    }

    #[test]
    fn preauthored_report_binds_after_first_green_then_same_source_completes() {
        let future_source = source("candidate");
        let placeholder = VerificationCandidate {
            source: source("placeholder"),
            first_green: run(),
            review: None,
            second_green: None,
        };
        let mut value = state(VerificationStage::Frozen, placeholder);
        value.candidate = None;
        let preauthored = report(&value, future_source.clone());

        record_green(&mut value, future_source.clone(), run());
        assert_eq!(value.stage, VerificationStage::FirstGreen);
        bind_review(
            &mut value,
            &future_source,
            &preauthored,
            ".v4hook/adversarial-review.json".to_owned(),
            "sha256:report".to_owned(),
        )
        .unwrap();
        assert_eq!(value.stage, VerificationStage::Reviewed);
        record_green(&mut value, future_source, run());
        assert_eq!(value.stage, VerificationStage::Complete);
    }

    #[test]
    fn changed_source_resets_review_to_first_green() {
        let candidate = VerificationCandidate {
            source: source("candidate"),
            first_green: run(),
            review: Some(VerificationReview {
                created_at: String::new(),
                report_path: String::new(),
                report_digest: String::new(),
            }),
            second_green: None,
        };
        let mut value = state(VerificationStage::Reviewed, candidate);
        record_green(&mut value, source("repaired"), run());
        assert_eq!(value.stage, VerificationStage::FirstGreen);
        let candidate = value.candidate.unwrap();
        assert_eq!(candidate.source, source("repaired"));
        assert!(candidate.review.is_none());
        assert!(candidate.second_green.is_none());
    }

    #[test]
    fn complete_state_still_requires_its_bound_review_report() {
        let candidate_source = source("candidate");
        let candidate = VerificationCandidate {
            source: candidate_source.clone(),
            first_green: run(),
            review: Some(VerificationReview {
                created_at: String::new(),
                report_path: "missing-review.md".to_owned(),
                report_digest: "sha256:missing".to_owned(),
            }),
            second_green: Some(run()),
        };
        let value = state(VerificationStage::Complete, candidate);
        assert!(
            require_review_unchanged(&value, Path::new("/does-not-exist"), &candidate_source)
                .is_err()
        );
    }
}
