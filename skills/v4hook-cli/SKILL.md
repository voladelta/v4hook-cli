---
name: v4hook-cli
description: Build and operate Uniswap v4 hook projects managed by the v4hook CLI. Use for hook design or implementation, project verification, deployment preparation, devnet operation, or explicitly authorized live deployment and pool launch.
---

# v4hook CLI

Turn a hook idea into a working `v4hook` project and drive its authorized lifecycle from chat. Treat
the pinned scaffold, installed CLI, and project configuration as sources of truth. Generated Solidity
is a draft until it compiles and passes the configured gates.

Use five leading words throughout the workflow: **authority** scopes mutation, **bound** ties
evidence to exact inputs, **frozen** preserves the pre-edit verifier's intended workload, **red**
reproduces a defect for the expected reason, and **green** means the intended configured workload
executed and passed.

## Bootstrap a fresh project

Skill mechanics may require this skill and other skills directly triggered by the user's product
requirements to be read before any task action. Treat those reads as preload only. Do not chase
optional references, choose an architecture, inspect external APIs, or begin design work before the
project-local inspection below.

When the authorized destination does not exist, the startup order is:

1. Resolve the retained global CLI, record its path and version, and read only the `init` help needed
   to create the scaffold.
2. Confirm that the exact destination is absent, then initialize it. This is the only project action
   allowed before local inspection.
3. Read the generated `AGENTS.md` first and follow its project map into `README.md`, metadata, lock,
   Foundry settings, remappings, active/example configuration, owned seed files, and only the pinned
   dependency paths needed to identify the actual APIs.
4. Only after that inspection is complete, activate preloaded security or protocol guidance and
   open the task-specific references routed by this skill.

Do not turn skill preload into startup research. Reading a mandatory skill early does not authorize
using its examples before the scaffold's pinned versions, ownership map, and integration surfaces
are known.

## Route the request

Classify the authorized outcome before changing state:

- **Explain or design:** inspect without editing. Read
  [hook-design.md](references/hook-design.md), return the architecture, permissions, trust and fund
  flows, invariants, open decisions, and test scenarios, then stop.
- **Review or verify:** inspect and run relevant local checks while leaving source and configuration
  unchanged unless the user also requests a fix. Read
  [integration-contract.md](references/integration-contract.md), apply
  `v4-security-foundations` to Solidity hooks, and use
  [final-review.md](references/final-review.md) for a complete implementation review. Stop after every
  in-scope invariant and configured workload has evidence or an explicit evidence gap.
- **Create, build, fix, or adapt:** make local project changes and run the implementation workflow
  below. Stop when the requested local behavior and applicable local gates pass, or when no locally
  actionable strategy remains.
- **Prepare deployment:** configure, check, plan, and fork-simulate when the required inputs and a
  clean worktree are available. Stop with bound evidence and remaining live actions; preparation
  does not authorize them.
- **Operate a devnet:** start from an immutable deployment plan and follow
  [devnet.md](references/devnet.md). Stop with every retained service identified, or with every
  temporary service stopped and its owned state handled as requested.
- **Perform a live action:** sign, broadcast, verify on a live explorer, access a wallet, deploy, or
  launch a pool only when the user explicitly authorizes that action and names the network. Stop
  after the authorized lifecycle stage; later live stages require separate authority.

Keep private keys, mnemonics, keystore passwords, authenticated RPC URLs, and explorer credentials
out of command arguments, tracked files, logs, and responses. Preserve the CLI's plan, pinned-fork
simulation, exact-confirmation, and separate pool-launch boundaries.

## Inspect the project

1. Resolve one CLI binary and retain its exact path for the task. Honor a user-supplied path; in a
   v4hook-cli source checkout prefer a freshly built `target/release/v4hook`; otherwise use the
   PATH-installed binary. Record its path and version, and read the relevant `--help` output.
2. For authorized creation, follow **Bootstrap a fresh project**, resolve a new path, and initialize
   there. Initialization is the only project action before reading the generated local instructions.
3. Detect the managed project from `.v4hook.toml`, `.v4hook-template-lock.json`, or its active
   v4hook configuration.
4. Read that project's `AGENTS.md`, `README.md`, `.v4hook.toml`, template lock, `foundry.toml`,
   remappings, and active and example configuration in their instructed order.
5. Inspect only the pinned `vendor/` paths needed to choose a base contract or import. After local
   inspection, read [local-workflow.md](references/local-workflow.md) before running `doctor` or
   `check`, updating a scaffold, or repairing a local verification tool.

The inspection is complete when project ownership, the exact CLI path/version, template
compatibility, active configuration, relevant help, and pinned contract APIs are known. Keep the
worktree unchanged until that criterion is met. Use a compatible newer CLI rather than applying a
scaffold whose embedded template predates the project's pinned version. Complete this local
inspection before loading security, protocol-integration, deployment, or verification-loop guidance.

Read [hook-design.md](references/hook-design.md) when selecting architecture, permissions,
utilities, access control, shares, or constructor behavior. Read
[dynamic-fees.md](references/dynamic-fees.md) when the hook overrides or updates LP fees. Treat any
generator output as a draft against the pinned APIs.

## Prepare the implementation

Complete these gates before the first edit:

1. Read [integration-contract.md](references/integration-contract.md) for every change to Solidity,
   scripts, tests, permissions, configuration, or simulation wiring.
2. Convert the request into a specification covering lifecycle events, user and router identity,
   fund movement, return deltas, mutable state, administration, constructor inputs, supported
   tokens, failure behavior, and representative pool behavior.
3. When adapting an existing or reference implementation, record a protected-invariant ledger for
   its economic formulas and allocations, liquidity provenance, custody and recovery, trust
   boundaries, role separation, and supported paths. Preserve every item unless the user approves a
   material design change.
4. Record the exact pre-edit gate commands, filters, effective settings, and executed counts as the
   frozen verification manifest required by the integration contract.
5. Replace the scaffold verification-contract example with a tracked contract mapping every
   protected invariant to exact test names in the configured unit, fuzz, or invariant gate. Give an
   invariant without local proof one explicit external evidence gap instead of a plausible test
   claim. Commit the specification, ledger, active configuration, verification contract, and
   scaffold baseline before editing production code, then run `v4hook verification freeze`.
6. Load and apply `v4-security-foundations` for Solidity hook implementation or review. If it is
   unavailable, say so and do not claim that review occurred. Follow project `AGENTS.md` routing and
   use current official Foundry and Uniswap guidance for version-sensitive behavior and addresses;
   confirm CLI and Foundry flags against installed `--help` output.

Preparation is complete when the specification is checkable, every protected invariant is mapped,
the effective workload is frozen from a clean baseline commit, and each selected base, import, and
permission exists in the pinned tree.

For TypeScript or JavaScript clients, scenario runners, frontends, indexers, router flows,
companion-contract interaction, or cross-protocol DeFi composition, read
[evm-integration.md](references/evm-integration.md) before that integration. It routes
`viem-integration`, `v4-sdk-integration`, and current network guidance by the integration actually
being built. Load `chainlink-vrf-skill` only when the design uses Chainlink VRF.

Do not load the ETHSkills root for a Solidity-only hook or an ordinary companion ERC-20. The pinned
project APIs, [hook-design.md](references/hook-design.md),
[integration-contract.md](references/integration-contract.md), and `v4-security-foundations` own
that path. The EVM-integration reference routes individual ETHSkills topics only for a matching app,
network, wallet, indexing, remote-address, or cross-protocol branch.

## Implement inside the scaffold

- Use the narrowest pinned base and extend its actual `BaseHook` API. Preserve inherited
  PoolManager-caller checks and implement the corresponding internal callbacks.
- Enable only callbacks the implementation exercises. Keep one permission set aligned across
  `getHookPermissions()`, deployment configuration, CREATE2 flags, tests, and deployed probing.
- Update the contract, deployment scripts, constructor ABI, artifact path, active configuration,
  tracked example configuration, and fixtures as one integration change. Keep secrets and private
  endpoints out of tracked examples.
- Preserve the deployment script's plan-bound salt and predicted-address integration. Remote scripts
  consume plan-bound dependency addresses; local-only fixtures remain local.
- Model every broadcast signer through the integration contract's authority declarations. Split
  stages when one signer is not authorized for every call.
- Build from the requirement and exercise behavior through its intended production interface. Tests,
  mocks, fixtures, demos, and clients provide evidence; they do not become substitute owners for the
  requested production behavior.
- Preserve tests and configured gates while diagnosing failures. Repair the implementation,
  configuration, environment, or verification contract that actually violated the requirement.

Implementation is ready for full verification when the aligned artifacts compile, focused tests
exercise the changed production path, and the focused gate is green.

## Verify the implementation

A completed hook implementation or material adaptation uses the ignored v4hook ledger defined in
[local-workflow.md](references/local-workflow.md). Create it after freezing the parent verification
contract, then resume from that ledger and actual repository state until the parent gate is
Complete, Escalated, or Blocked.

When subagent delegation is available, read
[delegated-review-loop.md](references/delegated-review-loop.md) before first-green review. Use
`task-contracts` to bound each fresh reviewer, fixer, and verifier; the coordinator retains parent
contract ownership and alone decides terminal status.

Make the narrowest relevant reproducer red for the expected reason, repair the defect, then make that
gate green. Run the configured gates proportionate to the change. For a completed hook
implementation, read [local-workflow.md](references/local-workflow.md) and finish its complete check.

Keep the frozen workload scope through every repair. Repair a verifier or tooling defect by making
the same intended workload reliable; a replacement command, narrower filter, reduced count, weaker
severity, or removed assertion cannot turn red into green. When the original gate remains
nondeterministic or incompatible after evidence-changing attempts, preserve its failure and report
the exact blocker instead of claiming completion.

A green first pass is not completion for a hook implementation or material adaptation. Commit the
candidate and use `v4hook verification check` so the CLI binds first-green evidence to that clean
source digest and validates every exact test mapping. Read [final-review.md](references/final-review.md),
inspect that candidate diff as untrusted input, reapply `v4-security-foundations`, and compare it
with the original requirements, protected-invariant ledger, and pre-edit verification baseline.
Write the review report under ignored `.v4hook/` state and bind it with `v4hook verification review`.
Repair every must-fix finding, commit the repair, and run `v4hook verification check`; a changed
source becomes a new first-green candidate and therefore requires a new review. Completion requires
the lifecycle state `complete`, produced only by a second check of the unchanged reviewed source and
unchanged report. Keep the tracked tree unchanged after completion; if a tracked evidence document
must change, commit it and repeat first-green, review, and second-green for that new source. Passing
gates and fork evidence are necessary evidence, not a security audit.

## Advance the lifecycle

Read [deployment-lifecycle.md](references/deployment-lifecycle.md) before planning, simulating,
deploying, live verification, readiness classification, or pool planning and launch. It defines the
bound evidence and separate authority boundaries for those stages.

Read [devnet.md](references/devnet.md) before `v4hook devnet up`, `status`, `reset`, `export`, `run`,
or `down`, or when a persistent browser or multi-wallet environment is requested.

Maintain an operational ledger during long-running lifecycle work: current commit and plan digest,
active reproducer, every PID/port/launch-agent label, temporary repository, and generated evidence
path. At handoff, identify each intentionally retained service or stop it and handle only
CLI-verified generated artifacts.

## Report completion

Report the authorized outcome, hook architecture and permissions when applicable, material security
assumptions, files changed, checks actually run with meaningful counts, immutable evidence produced,
the completed verification-state digest and source commit, and remaining blockers or live actions. Run safe in-scope commands instead of handing them back to
the user. Describe only the lifecycle stage whose completion criterion passed.
