use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

fn default_project_root() -> String {
    ".".to_owned()
}

fn default_hex() -> String {
    "0x".to_owned()
}

fn default_max_fork_block_drift() -> u64 {
    64
}

pub const fn default_devnet_accounts() -> u16 {
    100
}

pub const fn default_minimum_fuzz_runs() -> u64 {
    1_000
}

pub const fn default_minimum_invariant_runs() -> u64 {
    256
}

pub const fn default_minimum_invariant_depth() -> u64 {
    500
}

pub const fn default_max_runtime_code_size() -> u64 {
    24_576
}

pub const fn default_max_init_code_size() -> u64 {
    49_152
}

fn default_slither_fail_on() -> SlitherImpact {
    SlitherImpact::High
}

fn default_slither_require_triage_on() -> SlitherImpact {
    SlitherImpact::Low
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SimulationKind {
    Deploy,
    Pool,
    Quadrants,
    Postconditions,
}

impl SimulationKind {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Deploy => "deploy",
            Self::Pool => "pool",
            Self::Quadrants => "quadrants",
            Self::Postconditions => "postconditions",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SimulationStep {
    pub kind: SimulationKind,
    pub command: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub required_authorities: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContractConfig {
    pub artifact: String,
    #[serde(default = "default_hex")]
    pub constructor_args: String,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NetworkConfig {
    pub chain_id: u64,
    pub rpc_url_env: String,
    pub pool_manager: String,
    pub position_manager: String,
    pub universal_router: String,
    pub quoter: String,
    pub state_view: String,
    pub permit2: String,
    pub create2_deployer: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum SlitherImpact {
    Informational,
    Low,
    Medium,
    High,
}

impl SlitherImpact {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Informational => "informational",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlitherFindingAllowance {
    pub fingerprint: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlitherPolicy {
    #[serde(default = "default_slither_fail_on")]
    pub fail_on: SlitherImpact,
    #[serde(default = "default_slither_require_triage_on")]
    pub require_triage_on: SlitherImpact,
    #[serde(default)]
    pub dependency_paths: Vec<String>,
    #[serde(default)]
    pub allowed_findings: Vec<SlitherFindingAllowance>,
}

impl Default for SlitherPolicy {
    fn default() -> Self {
        Self {
            fail_on: default_slither_fail_on(),
            require_triage_on: default_slither_require_triage_on(),
            dependency_paths: Vec::new(),
            allowed_findings: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeSizePolicy {
    #[serde(default = "default_max_runtime_code_size")]
    pub max_runtime_bytes: u64,
    #[serde(default = "default_max_init_code_size")]
    pub max_init_code_bytes: u64,
}

impl Default for CodeSizePolicy {
    fn default() -> Self {
        Self {
            max_runtime_bytes: default_max_runtime_code_size(),
            max_init_code_bytes: default_max_init_code_size(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChecksConfig {
    pub unit: Vec<String>,
    pub fuzz: Vec<String>,
    pub invariant: Vec<String>,
    pub static_analysis: Vec<String>,
    #[serde(default)]
    pub slither_policy: SlitherPolicy,
    #[serde(default)]
    pub gas_snapshot: Vec<String>,
    #[serde(default)]
    pub code_size: CodeSizePolicy,
    #[serde(default = "default_minimum_fuzz_runs")]
    pub minimum_fuzz_runs: u64,
    #[serde(default = "default_minimum_invariant_runs")]
    pub minimum_invariant_runs: u64,
    #[serde(default = "default_minimum_invariant_depth")]
    pub minimum_invariant_depth: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SimulationConfig {
    #[serde(default = "default_max_fork_block_drift")]
    pub max_fork_block_drift: u64,
    #[serde(default)]
    pub anvil_args: Vec<String>,
    pub steps: Vec<SimulationStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeploymentConfig {
    pub script: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub required_authorities: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevnetScenario {
    pub name: String,
    pub command: Vec<String>,
    pub verification: DevnetScenarioVerification,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevnetRequiredEvent {
    pub address: String,
    pub topic0: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevnetScenarioVerification {
    pub expected_transactions: u64,
    pub expected_senders: u64,
    pub allowed_targets: Vec<String>,
    #[serde(default)]
    pub required_events: Vec<DevnetRequiredEvent>,
    #[serde(default)]
    pub reserved_account_indices: Vec<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevnetConfig {
    #[serde(default = "default_devnet_accounts")]
    pub accounts: u16,
    #[serde(default)]
    pub block_time_seconds: Option<u64>,
    #[serde(default)]
    pub scenarios: Vec<DevnetScenario>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PoolConfig {
    pub currency0: String,
    pub currency1: String,
    pub fee: u32,
    pub tick_spacing: i32,
    pub sqrt_price_x96: String,
    pub tick_lower: i32,
    pub tick_upper: i32,
    pub liquidity: String,
    pub amount0_max: String,
    pub amount1_max: String,
    pub recipient: String,
    #[serde(default = "default_hex")]
    pub hook_data: String,
    pub launch_script: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub launch_authorities: BTreeMap<String, String>,
    pub simulation_steps: Vec<SimulationStep>,
    pub live_verify: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct V4HookConfig {
    pub schema_version: String,
    #[serde(default = "default_project_root")]
    pub project_root: String,
    pub contract: ContractConfig,
    pub network: NetworkConfig,
    pub checks: ChecksConfig,
    pub simulation: SimulationConfig,
    pub deployment: DeploymentConfig,
    #[serde(default)]
    pub devnet: Option<DevnetConfig>,
    #[serde(default)]
    pub pool: Option<PoolConfig>,
}

#[derive(Debug, Clone)]
pub struct LoadedConfig {
    pub path: String,
    pub project_root: String,
    pub value: V4HookConfig,
    pub raw: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckEvidence {
    pub name: String,
    pub command: Vec<String>,
    pub duration_ms: u64,
    pub stdout_hash: String,
    pub stderr_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_summary: Option<FoundryTestSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slither_summary: Option<SlitherSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_size_summary: Option<CodeSizeSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlitherFinding {
    pub fingerprint: String,
    pub check: String,
    pub impact: SlitherImpact,
    pub confidence: String,
    pub path: String,
    pub lines: Vec<u64>,
    pub allowed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlitherSummary {
    pub findings: Vec<SlitherFinding>,
    pub allowed_findings: u64,
    pub untriaged_findings: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeSizeSummary {
    pub unit: String,
    pub runtime: u64,
    pub runtime_limit: u64,
    pub init_code: u64,
    pub init_code_limit: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FoundryTestSummary {
    pub total: u64,
    pub unit: u64,
    pub fuzz: u64,
    pub invariant: u64,
    pub minimum_fuzz_runs: Option<u64>,
    pub minimum_invariant_runs: Option<u64>,
    pub minimum_invariant_calls: Option<u64>,
    pub invariant_reverts: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceIdentity {
    pub commit: String,
    pub dirty: bool,
    pub tree_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolchainIdentity {
    /// Kept as `node` to preserve the v1 plan schema; now identifies the Rust CLI build.
    pub node: String,
    pub forge: String,
    pub cast: String,
    pub anvil: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContractIdentity {
    pub address: String,
    pub code_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NetworkIdentity {
    pub chain_id: u64,
    pub rpc_url_env: String,
    pub fork_block_number: u64,
    pub fork_block_hash: String,
    pub contracts: BTreeMap<String, ContractIdentity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImmutableReference {
    pub start: usize,
    pub length: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactIdentity {
    pub path: String,
    pub file_digest: String,
    pub creation_bytecode_hash: String,
    pub runtime_bytecode_hash: String,
    pub constructor_args: String,
    pub init_code_hash: String,
    pub immutable_references: Vec<ImmutableReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HookIdentity {
    pub permissions: Vec<String>,
    pub flags: String,
    pub salt: String,
    pub predicted_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanSimulation {
    pub max_fork_block_drift: u64,
    pub anvil_args: Vec<String>,
    pub steps: Vec<SimulationStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanDeployment {
    pub script: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub required_authorities: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeploymentPlan {
    pub schema_version: String,
    pub created_at: String,
    pub config_path: String,
    pub config_digest: String,
    pub project_root: String,
    pub source: SourceIdentity,
    pub toolchain: ToolchainIdentity,
    pub network: NetworkIdentity,
    pub artifact: ArtifactIdentity,
    pub hook: HookIdentity,
    pub checks: Vec<CheckEvidence>,
    pub simulation: PlanSimulation,
    pub deployment: PlanDeployment,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandEvidence {
    pub kind: SimulationKind,
    pub command: Vec<String>,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub stdout_hash: String,
    pub stderr_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_summary: Option<FoundryTestSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SimulationEvidence {
    pub schema_version: String,
    pub created_at: String,
    pub plan_digest: String,
    pub fork_block_number: u64,
    pub fork_block_hash: String,
    pub anvil_version: String,
    pub anvil_rpc_url: String,
    pub commands: Vec<CommandEvidence>,
    pub deployed_runtime_code_hash: String,
    pub passed: bool,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevnetState {
    pub schema_version: String,
    pub created_at: String,
    pub pid: u32,
    pub port: u16,
    pub rpc_url: String,
    pub log_path: String,
    pub plan_path: String,
    pub plan_digest: String,
    pub project_root: String,
    pub chain_id: u64,
    pub fork_block_number: u64,
    pub fork_block_hash: String,
    pub hook_address: String,
    pub deployed_runtime_code_hash: String,
    pub marker_address: String,
    pub marker_code: String,
    pub accounts: Vec<String>,
    pub manifest_path: String,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevnetHookManifest {
    pub address: String,
    pub abi: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevnetPoolManifest {
    pub currency0: String,
    pub currency1: String,
    pub fee: u32,
    pub tick_spacing: i32,
    pub sqrt_price_x96: String,
    pub tick_lower: i32,
    pub tick_upper: i32,
    pub liquidity: String,
    pub amount0_max: String,
    pub amount1_max: String,
    pub recipient: String,
    pub hook_data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevnetManifest {
    pub schema_version: String,
    pub created_at: String,
    pub warning: String,
    pub rpc_url: String,
    pub chain_id: u64,
    pub fork_block_number: u64,
    pub fork_block_hash: String,
    pub plan_digest: String,
    pub hook: DevnetHookManifest,
    pub contracts: BTreeMap<String, String>,
    pub accounts: Vec<String>,
    pub pool: Option<DevnetPoolManifest>,
    pub scenarios: Vec<String>,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevnetStatus {
    pub running: bool,
    pub rpc_url: String,
    pub chain_id: u64,
    pub block_number: u64,
    pub fork_block_number: u64,
    pub hook_address: String,
    pub accounts: usize,
    pub plan_digest: String,
    pub manifest_path: String,
    pub log_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevnetScenarioEvidence {
    pub schema_version: String,
    pub created_at: String,
    pub plan_digest: String,
    pub scenario: String,
    pub seed: u64,
    pub accounts: usize,
    pub start_block: u64,
    pub end_block: Option<u64>,
    pub command: Vec<String>,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub stdout_hash: String,
    pub stderr_hash: String,
    pub integrity_passed: bool,
    pub verification: DevnetScenarioVerificationEvidence,
    pub passed: bool,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevnetScenarioReport {
    pub schema_version: String,
    pub transactions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevnetTransactionEvidence {
    pub hash: String,
    pub sender: String,
    pub target: String,
    pub block_number: u64,
    pub gas_used: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevnetReservedAccountEvidence {
    pub index: u16,
    pub address: String,
    pub nonce_before: String,
    pub nonce_after: String,
    pub balance_before: String,
    pub balance_after: String,
    pub unchanged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevnetScenarioVerificationEvidence {
    pub expected_transactions: u64,
    pub observed_transactions: u64,
    pub expected_senders: u64,
    pub observed_senders: u64,
    pub assertions: Vec<DevnetScenarioAssertion>,
    pub transactions: Vec<DevnetTransactionEvidence>,
    pub reserved_accounts: Vec<DevnetReservedAccountEvidence>,
    pub issues: Vec<String>,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevnetScenarioAssertion {
    pub name: String,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadinessStage {
    pub ready: bool,
    pub evidence: Vec<String>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadinessReport {
    pub schema_version: String,
    pub highest_stage: String,
    pub configuration: ReadinessStage,
    pub local: ReadinessStage,
    pub testnet: ReadinessStage,
    pub launch: ReadinessStage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DevnetDownResult {
    pub stopped: bool,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PoolPlan {
    pub schema_version: String,
    pub created_at: String,
    pub deployment_plan_digest: String,
    pub hook_address: String,
    pub chain_id: u64,
    pub fork_block_number: u64,
    pub fork_block_hash: String,
    pub max_fork_block_drift: u64,
    pub pool: PoolConfig,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PoolSimulationEvidence {
    pub schema_version: String,
    pub created_at: String,
    pub pool_plan_digest: String,
    pub fork_block_number: u64,
    pub fork_block_hash: String,
    pub commands: Vec<CommandEvidence>,
    pub passed: bool,
    pub digest: String,
}
