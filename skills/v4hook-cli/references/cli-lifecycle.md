# CLI lifecycle

Use the installed command's `--help` output as the syntax authority. This reference defines the
required state transitions and evidence.

## Create or inspect

- Run `v4hook init <new-directory>` only for a path that does not exist.
- Read the generated project instructions and pinned dependency metadata.
- Resolve and retain one explicit CLI path. Before a scaffold update, preview it and compare the
  CLI's embedded template with the project's pinned version; never downgrade the project.
- Copy `v4hook.config.example.json` to the working config and replace every project-specific
  placeholder. Do not assume the example knows the hook, tests, tokens, or network.
- A tracked `.env.example` may contain documented, non-keyed public RPC URLs. Copy it to the
  ignored `.env` for initial reads and local forks. Probe the endpoint's chain ID and intended
  pinned block before planning; public rate-limited endpoints are not launch evidence. Replace
  them with dedicated archive-capable infrastructure, and never commit authenticated URLs.
- Run `v4hook doctor --config <config>` before expensive checks.

When doctor reports a missing `slither` executable, prefer an isolated uv tool installation:

```sh
uv tool install slither-analyzer
```

Use `uvx --from slither-analyzer slither . --filter-paths 'vendor/' --fail-high` for a one-off run.
The one-off command does not satisfy a configured `slither` executable gate; install the tool or
intentionally configure the full uvx command. Filter pinned vendor paths to keep project findings
actionable, fail on high-severity findings, and triage lower-severity output. Never replace static
analysis with Foundry lint.

## Check

Run `v4hook check --config <config>` after focused Foundry tests. It must cover formatting, lint,
static analysis, build, unit, fuzz, and invariant gates. A filter that executes no matching tests
is a failure, not a pass. A skipped test is also a failure. Inspect the emitted test summaries and
require at least 1,000 cases for every fuzz test and 256 campaigns at depth 500 for every invariant.

The check stops at the first failed command. When a configured tool is unavailable, do not rewrite
the config to bypass it: preserve the command, run later gates individually, and report the exact
blocker. If lint/cache state makes Foundry report no tests, force a rebuild and rerun the unchanged
test gate.

After any failure, repair every locally actionable implementation, config, script, test, or tool
issue and rerun the narrow gate followed by the complete check. Continue until the local check is
clean; do not stop after merely collecting a blocker list.

## Plan

Run `v4hook plan --config <config> --output <plan>` only with a clean project worktree and a valid
RPC setting. Do not discard or silently commit unrelated changes to satisfy this requirement.

The plan binds source identity, toolchain versions, config digest, artifact and constructor,
network chain and contract code, fork block, permissions, CREATE2 salt, and predicted address.
Changing a bound input requires a new plan.

## Simulate

Run `v4hook simulate --plan <plan> --output <evidence>`. Require all four stages:

1. Deploy the exact planned hook to the pinned Anvil fork.
2. Create a representative pool and add liquidity.
3. Exercise both directions with exact input and exact output.
4. Verify balances, deltas, permissions, and final state.

Require the deployed runtime code and reported permissions to match the plan.

Inspect simulation scripts as transactions, not ordinary test helpers. Each broadcast stage must
use an address authorized for every call in that stage. Split registrar, treasury, owner, and admin
operations when their signers differ. Declare role/address pairs in `requiredAuthorities` and use
`deployment.requiredAuthorities` and `pool.launchAuthorities` for live scripts. Never broadcast a
call that is expected to revert; perform diagnostic probes outside broadcast or through read-only
quoting.

Deployment simulation exports `V4HOOK_PREDICTED_ADDRESS`; a later pool plan exports
`V4HOOK_HOOK_ADDRESS`. Ensure pool scripts accept the address for the lifecycle being exercised.
Quadrant and postcondition commands must observe the plan-deployed hook, not redeploy an unrelated
local fixture or use an ordinary unit test as a proxy.

## Keep an interactive devnet

Use `v4hook devnet` only after a deployment plan exists. It is a persistent development surface for
browser apps and deterministic multi-wallet scenarios, not an alternative deployment proof.

- `devnet up` must reuse the plan's exact pinned fork and all four simulation stages before it
  detaches Anvil.
- Persistent Anvil must cross a real daemon boundary on Unix. Dropping a Rust `Child` is not enough:
  it can survive an interactive shell but be killed when a one-shot agent command exits. Verify
  persistence from a separate command process.
- Keep the RPC bound to `127.0.0.1`. Export addresses, ABI and disposable account addresses, never
  the mnemonic or private keys.
- Treat the unlocked local RPC as browser-accessible disposable state. Stop it when unused and
  narrow Anvil's allowed origin in `simulation.anvilArgs` when the app has a stable origin.
- Use project-configured scenario commands for hook-specific Universal Router, Permit2 and
  `hookData` behavior. Record a seed and evidence so failures reproduce.
- Use Anvil manual mining when exact same-block batching and ordering matter; use interval mining
  for browser confirmation behavior.
- `devnet status`, `reset`, `export`, `run` and `down` must verify process ownership and devnet chain
  identity before operating. `down` must never signal an unverified PID.
- `devnet reset` discards interactive state, restores the pinned fork and reruns the plan bootstrap.

The generated web manifest is safe to commit only if the project intentionally wants a local-only
fixture, but the default `.v4hook/` location is ignored and should normally remain ephemeral.

## Deploy and verify

Proceed only after explicit authorization for the named network and wallet action. Use the account
and public sender requested by the user, the exact CLI confirmation derived from the plan, and the
mainnet acknowledgement flag when required. Never pass a private key or authenticated RPC URL as a
process argument.

Deployment reruns the mandatory simulation before broadcast. After broadcast, verify live bytecode,
permissions, and pinned network dependencies. Create a fresh plan when fork evidence is stale or
the predicted address is occupied.

## Plan and launch a pool separately

Do not merge hook deployment with pool launch. After live hook verification:

1. Create a pool plan bound to the deployment plan.
2. Simulate pool creation, swap quadrants, and postconditions.
3. Launch only with explicit authorization and the exact pool confirmation.
4. Run the configured read-only live verification.

Use sorted currencies, bounded token approvals and liquidity, a dedicated testnet account for the
first launch, and small live swaps before increasing exposure.

## Readiness closure

Classify completion precisely:

- **Locally ready:** format, build, lint, static analysis, unit, fuzz, invariant, integration, code
  size, scripts, config and documentation pass with no known locally actionable defect.
- **Testnet-ready:** local readiness plus finalized non-sentinel inputs, verified network contracts,
  clean immutable plan and passing pinned-fork simulation.
- **Launch-ready:** testnet evidence plus the required independent security/economic review,
  operational monitoring and explicit authorization for the named network and wallet actions.

External audit findings, RPC access, finalized economic inputs and live authorization cannot be
manufactured by an implementation loop. Report them as stage gates without claiming readiness.
