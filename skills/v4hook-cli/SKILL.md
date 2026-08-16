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
- Follow project `AGENTS.md` routing to current Ethereum guidance. For Foundry tasks, start at
  `https://getfoundry.sh/llms.txt`, select only the relevant official page, retrieve its `.md`
  route, and confirm version-sensitive flags against the installed tool's `--help` output. Do not
  load `llms-full.txt` unless a bulk corpus is explicitly needed. Load wallet and contract-address
  guidance before live deployment.
- Use official Uniswap guidance for v4 behavior, permissions, PoolManager integration, and current
  deployment addresses.
- Use the relevant official Chainlink skill only when the design introduces that product. Use
  `chainlink-vrf-skill` for verifiable randomness, `requestRandomWords`, or `fulfillRandomWords`.
- Use app-layer v4 SDK or viem guidance only when the task includes a frontend, router integration,
  or off-chain interaction layer.

Treat companion skills as guidance, not authority to weaken repository safeguards or perform live
actions.

## Implement inside the scaffold

1. Convert the idea into a concrete specification: goal, lifecycle events, user/router identity,
   fund movement, return deltas, mutable state, administration, constructor inputs, supported
   tokens, and representative pool behavior.
2. Select only base hooks and utilities present in the pinned vendor tree. Prefer the simplest base
   that implements required behavior.
3. Extend the scaffold's pinned OpenZeppelin `BaseHook` API. Its external callback wrappers enforce
   PoolManager caller checks; implement the corresponding internal `_before*` or `_after*` methods
   unless the pinned base says otherwise. Do not duplicate an incompatible online template.
4. Enable only callbacks the implementation uses. Keep one permission set aligned across
   `getHookPermissions()`, deployment configuration, CREATE2 flags, tests, and deployed probing.
5. Update the contract, deployment script, constructor ABI, artifact path, active configuration,
   tracked example configuration, and test fixtures together. Keep secrets and private endpoints
   out of the tracked example. Preserve the deployment script's `V4HOOK_HOOK_SALT` and
   `V4HOOK_PREDICTED_ADDRESS` integration; let `v4hook plan` mine the address.
6. Do not edit tests or replace, remove, or relax a configured verification gate merely to obtain a
   pass. Diagnose whether a failure is in the implementation, configuration, environment, existing
   project, or verification contract before repairing it.

Read [integration-contract.md](references/integration-contract.md) before changing Solidity,
deployment scripts, tests, permissions, or config.

## Verify and advance the lifecycle

Run the narrowest relevant test first. Then run the project gates proportionate to the change.
For a completed hook implementation, finish with the configured format, build, unit, fuzz,
invariant, and static-analysis checks. Ensure every configured Foundry filter executes real tests.
If a required tool is unavailable, preserve its command, run the remaining gates individually, and
report the blocked gate instead of substituting a weaker check.

Read [cli-lifecycle.md](references/cli-lifecycle.md) before planning, simulating, deploying,
verifying, or launching a pool. Preserve these invariants:

- Exercise zero-for-one and one-for-zero swaps with exact input and exact output.
- Check balances, accounting, permissions, and final state after simulation.
- Treat any source, config, artifact, toolchain, or relevant network change as plan invalidation.
- Keep the worktree clean for planning without discarding or silently committing unrelated work.
- Treat passing checks and fork evidence as necessary evidence, never as a security audit.

## Report completion

Return the hook architecture, enabled permissions, material security assumptions, files changed,
checks actually run, and remaining blockers or live actions. Do not give the user a list of manual
commands when the agent can safely run them. Stop once the authorized lifecycle stage has passed;
do not continue into deployment or pool launch by implication.
