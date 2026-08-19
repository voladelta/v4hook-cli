---
name: v4hook-cli
description: Create, implement, test, review, plan, simulate, and perform explicitly authorized deployments of Uniswap v4 hooks using the v4hook CLI scaffold. Use when a user describes a hook idea, asks to generate or adapt a hook, works in a v4hook-managed Foundry project, or wants an agent to prepare a hook or pool for testnet or mainnet without manually running CLI commands.
---

# v4hook CLI

Turn a hook idea into a working `v4hook` project and drive the local workflow from chat. Treat the
pinned scaffold, installed CLI, and project configuration as the sources of truth. Treat generated
Solidity as a draft until it compiles and passes the configured gates.

## Establish scope and authority

Classify the request before changing state:

- For explanation or design, inspect and propose without editing files.
- For create, build, fix, or adapt, make local project changes and run relevant local checks.
- For deployment preparation, configure, plan, and fork-simulate when credentials and a clean
  worktree are available.
- Sign, broadcast, verify on a live explorer, access a wallet, or launch a pool only when the user
  explicitly authorizes the action and names the network.

Never expose private keys, mnemonics, keystore passwords, authenticated RPC URLs, or explorer
credentials. Never bypass the plan, pinned Anvil simulation, exact confirmation, or separate pool
launch flow.

## Inspect before generating

1. Detect a managed project from `.v4hook.toml`, `.v4hook-template-lock.json`, or a v4hook config.
2. Read the nearest `AGENTS.md`, project `README.md`, `foundry.toml`, remappings, configuration, and
   template lock before deciding how the project works.
3. Resolve one CLI binary and use that exact path throughout the task. Honor a user-supplied path;
   in a v4hook-cli source checkout prefer its freshly built `target/release/v4hook`; otherwise use
   the PATH-installed binary. Record its path and version, and read the relevant `--help` output.
   Do not invent flags when the binary is unavailable.
4. If creation is authorized and no project exists, resolve a safe new directory and run
   `v4hook init <directory>`. Never initialize over an existing path.
5. Inspect the pinned `vendor/` tree before selecting a base contract or import. Do not fetch,
   upgrade, or edit vendored dependencies unless the user explicitly requests dependency work.

Never apply a scaffold whose embedded template version is older than the project's pinned version.
Locate a compatible newer CLI or report the version mismatch.

Read [hook-design.md](references/hook-design.md) when selecting the hook architecture, permissions,
utilities, access control, shares, or constructor. Do not require a generator or MCP; treat any
compatible generated output only as a draft.

Read [dynamic-fees.md](references/dynamic-fees.md) when a hook overrides or updates LP fees.

## Route specialist guidance

- Load and apply `v4-security-foundations` for every Solidity hook implementation or review. Use it
  before designing fund movement, return deltas, router trust, or external calls, and again for the
  final security review. If it is unavailable, say so and do not claim that its review occurred.
- Follow project `AGENTS.md` routing to current Ethereum guidance. Load only the ETHSkills topics
  required by the task: security and testing for hook or companion-contract work; L2, wallet, and
  contract-address guidance for network integration or deployment preparation; and indexing or
  frontend guidance for an off-chain application. For Foundry tasks, start at
  `https://getfoundry.sh/llms.txt`, select only the relevant official page, retrieve its `.md`
  route, and confirm version-sensitive flags against the installed tool's `--help` output. Do not
  load `llms-full.txt` unless a bulk corpus is explicitly needed. Load wallet and contract-address
  guidance before live deployment.
- Use official Uniswap guidance for v4 behavior, permissions, PoolManager integration, and current
  deployment addresses.
- Use the relevant official Chainlink skill only when the design introduces that product. Use
  `chainlink-vrf-skill` for verifiable randomness, `requestRandomWords`, or `fulfillRandomWords`.
- Load Uniswap AI's `viem-integration` for TypeScript or JavaScript clients, chain definitions, RPC
  transports, contract reads and writes, account handling, simulation, receipts, logs, event
  decoding, wallet connections, scenario runners, or off-chain companion-contract interaction.
  Load `v4-sdk-integration` as well when constructing v4 pool identifiers, routes, actions,
  liquidity operations, Permit2 data, or Universal Router calldata. Do not add viem or wagmi to a
  Solidity-only project. Neither skill authorizes wallet access, signing, broadcasting, deployment,
  pool launch, or bypassing a v4hook plan.

Treat companion skills as guidance, not authority to weaken repository safeguards or perform live
actions.

Read [evm-integration.md](references/evm-integration.md) before building a TypeScript or JavaScript
client, scenario runner, frontend, indexer, router flow, or companion-contract integration.

## Implement inside the scaffold

1. Convert the idea into a concrete specification: goal, lifecycle events, user/router identity,
   fund movement, return deltas, mutable state, administration, constructor inputs, supported
   tokens, and representative pool behavior. When adapting a reference implementation, also make
   a protected-invariant ledger for its load-bearing behavior: economic formulas and allocations,
   liquidity provenance, custody and recovery, trust boundaries, role separation, and supported
   paths. Preserve each item or obtain the user's explicit approval for a material design change;
   documenting a regression as a risk is not approval.
2. Select only base hooks and utilities present in the pinned vendor tree. Prefer the simplest base
   that implements required behavior.
3. Extend the scaffold's pinned OpenZeppelin `BaseHook` API. Its external callback wrappers enforce
   PoolManager caller checks; implement the corresponding internal `_before*` or `_after*` methods
   unless the pinned base says otherwise. Do not duplicate an incompatible online template.
4. Enable only callbacks the implementation uses. Keep one permission set aligned across
   `getHookPermissions()`, deployment configuration, CREATE2 flags, tests, and deployed probing.
5. Update the contract, deployment script, constructor ABI, artifact path, active configuration,
   tracked example configuration, and test fixtures together. Keep secrets and private endpoints
   out of the tracked example. Non-keyed public RPC defaults are acceptable when their provider
   documents them; label their rate limits and replace them with dedicated archive-capable
   infrastructure before launch evidence. Preserve the deployment script's `V4HOOK_HOOK_SALT` and
   `V4HOOK_PREDICTED_ADDRESS` integration; let `v4hook plan` mine the address.
   Use the scaffold's `v4hook-testkit` only for local fixtures. Remote scripts must consume the
   plan-bound `V4HOOK_*` dependency addresses, and router tests must target the intended router ABI.
6. Do not edit tests or replace, remove, or relax a configured verification gate merely to obtain a
   pass. Diagnose whether a failure is in the implementation, configuration, environment, existing
   project, or verification contract before repairing it.
   Treat tests as evidence, not as the complete specification. Build the requested production
   artifact from the requirement, exercise it through its intended production interface, and keep
   behavior in its declared owner. Do not make tests pass with a test-only path, substitute artifact,
   or business logic duplicated in demos, mocks, fixtures, scripts, or clients. Verify not only that
   the system works, but that it works because the requested production artifact does the work.
7. Model broadcast authority explicitly. Split stages when registration, treasury, ownership, or
   administration require different signers. Declare role/address pairs in each broadcast step's
   `requiredAuthorities`, in `deployment.requiredAuthorities` for live deployment, and in
   `pool.launchAuthorities` for the live pool script so the CLI can select the fork sender and
   reject incompatible live senders. Never place an expected-to-revert diagnostic call inside a
   Foundry `broadcast` or `startBroadcast` region; probe outside broadcast or use a read-only quote
   path.

Read [integration-contract.md](references/integration-contract.md) before changing Solidity,
deployment scripts, tests, permissions, or config.

Before the first edit, record the effective verification baseline, including `forge config --json`
fuzz runs, invariant runs/depth/fail-on-revert, configured commands and filters, and the tests each
filter executes. The final settings and executed counts must meet or exceed that baseline unless the
user explicitly requests a reduction. An explicit project setting below an inherited/default value
is still a weakened gate.

## Verify and advance the lifecycle

Run the narrowest relevant test first. Then run the project gates proportionate to the change.
For a completed hook implementation, finish with the configured format, build, unit, fuzz,
invariant, structured Slither, code-size, and committed gas-snapshot checks. Ensure every configured
Foundry filter executes real tests. Never add Slither output/failure flags or detector exclusions
to the configured command; let the CLI enforce severity and exact source-bound triage fingerprints.
Treat the first full check as the start of a bounded repair loop:

1. Classify each failure as implementation, configuration, script, test, local tooling, launch
   input/network, live authority, or external assurance.
2. Fix every locally actionable defect and rerun its narrow reproducer.
3. Rerun the complete configured check and final `v4-security-foundations` review.
4. Repeat until local gates pass and no known locally actionable defect remains.

A green first pass is not completion. Perform a distinct adversarial review pass for every hook
implementation or material adaptation. Read [final-review.md](references/final-review.md), inspect
the final diff as untrusted input, reapply `v4-security-foundations`, and compare the result against
the protected-invariant ledger and pre-edit verification baseline. Use an independent maintainer or
security review skill when available; otherwise deliberately restart the review from the original
requirements rather than the implementation narrative. Repair every must-fix finding and rerun the
complete check. Do not weaken a gate, broaden an analyzer exclusion, or introduce a skip while
repairing the first pass.

If Slither is missing, prefer `uv tool install slither-analyzer` when uv and tool installation are
available. Use `uvx --from slither-analyzer slither .` for a one-off independent run. Do not install
uv through a piped remote script without user authorization, and never substitute `forge lint` for
Slither. If a required tool still cannot run, preserve its command, run the remaining gates
individually, and report the exact external blocker.

Do not use a residual-risk section to defer a fixable failure. Residual risks are uncertainties
that remain after local remediation, such as unaudited value-critical logic; blockers are missing
evidence or inputs required for the requested lifecycle stage.

Read [cli-lifecycle.md](references/cli-lifecycle.md) before planning, simulating, deploying,
verifying, or launching a pool. Preserve these invariants:

- Exercise zero-for-one and one-for-zero swaps with exact input and exact output.
- Check balances, accounting, permissions, and final state after simulation.
- Treat any source, config, artifact, toolchain, or relevant network change as plan invalidation.
- Keep the worktree clean for planning without discarding or silently committing unrelated work.
- Treat passing checks and fork evidence as necessary evidence, never as a security audit.

When the user needs a persistent browser or multi-wallet environment, use `v4hook devnet` from an
existing immutable deployment plan. Keep it localhost-only, export only the generated web-safe
manifest, and put hook-specific Universal Router/Permit2 traffic in deterministic project scenario
commands. Require each scenario to write only its transaction-hash report; let the CLI independently
verify managed-account completeness, receipts, senders, targets, hook events and reserved accounts.
Never expose Anvil mnemonics or private keys, and never describe devnet scenario evidence as the
mandatory one-shot deployment simulation.

Maintain an operational ledger throughout long tasks: current commit and plan digest, active
reproducer, every PID/port/launch-agent label, temporary repository, and generated evidence path.
At handoff, explicitly retain each requested service or stop it and purge only CLI-verified generated
artifacts. Do not leave undeclared processes or test repositories behind.

Call a project locally ready only after the repair loop is clean. Call it testnet-ready only when
sentinels are replaced, roles and broadcast stages are executable, `doctor`, `check`, `plan`, and
`simulate` pass, code size and gas are reviewed, and the security checklist has no unresolved
locally actionable item. Never call it launch-ready without the required independent review and
explicit authority for each live action.

Use `v4hook readiness` with the config and available plan/simulation evidence before reporting a
stage. Treat its launch-stage external requirements as non-self-attestable; never manufacture audit,
monitoring, or live-authorization evidence.

## Report completion

Return the hook architecture, enabled permissions, material security assumptions, files changed,
checks actually run, and remaining blockers or live actions. Do not give the user a list of manual
commands when the agent can safely run them. Stop once the authorized lifecycle stage has passed;
do not continue into deployment or pool launch by implication.
