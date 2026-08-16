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
- Run `v4hook doctor --config <config>` before expensive checks.

## Check

Run `v4hook check --config <config>` after focused Foundry tests. It must cover formatting, lint,
static analysis, build, unit, fuzz, and invariant gates. A filter that executes no matching tests
is a failure, not a pass.

The check stops at the first failed command. When a configured tool is unavailable, do not rewrite
the config to bypass it: preserve the command, run later gates individually, and report the exact
blocker. If lint/cache state makes Foundry report no tests, force a rebuild and rerun the unchanged
test gate.

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

Deployment simulation exports `V4HOOK_PREDICTED_ADDRESS`; a later pool plan exports
`V4HOOK_HOOK_ADDRESS`. Ensure pool scripts accept the address for the lifecycle being exercised.
Quadrant and postcondition commands must observe the plan-deployed hook, not redeploy an unrelated
local fixture or use an ordinary unit test as a proxy.

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
