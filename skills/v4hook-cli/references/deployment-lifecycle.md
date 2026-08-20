# Deployment lifecycle

Use the installed command's `--help` output as the syntax authority. This reference defines the
state transitions, bound evidence, and authority boundaries after local verification.

## Plan

Create a deployment plan only from a clean project worktree and a validated RPC setting. Preserve
unrelated changes; never discard or silently commit them to satisfy the clean-tree requirement.

The plan binds source identity, toolchain versions, configuration digest, artifact and constructor,
network chain and contract code, fork block, permissions, CREATE2 salt, and predicted address. A
change to any bound input invalidates the plan and requires a new one.

Planning is complete when the configured check is green, the worktree is clean, the target network
and dependencies are verified, and the CLI writes a bound plan for the exact source and
configuration.

## Simulate

Run simulation from the bound plan and require all four stages:

1. Deploy the exact planned hook to the pinned Anvil fork.
2. Create a representative pool and add liquidity.
3. Exercise zero-for-one and one-for-zero swaps with exact input and exact output.
4. Verify balances, deltas, permissions, accounting, and final state.

Require deployed runtime code and reported permissions to match the plan. Simulation scripts are
transactions: every broadcast stage uses a sender authorized for every call in the stage, and role
separation follows the declarations in
[integration-contract.md](integration-contract.md). Perform expected-revert diagnostics outside
broadcast or through read-only quoting.

Deployment simulation supplies `V4HOOK_PREDICTED_ADDRESS`; the later pool lifecycle supplies
`V4HOOK_HOOK_ADDRESS`. Shared tests resolve the address for the active stage. Quadrant and
postcondition commands observe the plan-deployed hook rather than redeploying a fixture or
substituting an ordinary unit test.

Simulation is complete when all four stages execute without unexpected skips and the evidence binds
the deployed runtime, permissions, transactions, and postconditions to the current plan.

## Deploy and verify live

Proceed only after explicit authorization for the named network and wallet action. Use the requested
account and public sender, the CLI confirmation derived from the plan, and the mainnet
acknowledgement when required. Keep private keys and authenticated RPC URLs out of process arguments.

Deployment reruns mandatory simulation before broadcast. After broadcast, verify live bytecode,
permissions, and pinned network dependencies. Create a fresh plan when fork evidence is stale or the
predicted address is occupied.

The deployment stage is complete only when the authorized broadcast has a successful receipt and
live verification matches the plan. Hook deployment does not authorize pool launch.

## Plan and launch a pool separately

After live hook verification:

1. Create a pool plan bound to the verified deployment plan.
2. Simulate pool creation, swap quadrants, and postconditions.
3. Launch only with separate explicit authorization and the exact pool confirmation.
4. Run the configured read-only live verification.

Use sorted currencies, bounded token approvals and liquidity, a dedicated testnet account for the
first launch, and small live swaps before increasing exposure. Pool launch is complete only when the
authorized launch receipt and read-only verification match the bound pool plan.

## Classify readiness

Run `readiness` with the active configuration and every available plan or simulation artifact. The
command validates digests and required gate/stage evidence. Its launch-stage requirements remain
external: local files and agent claims cannot certify an independent audit, monitoring, or live
authorization.

- **Locally ready:** formatting, build, lint, static analysis, unit, fuzz, invariant, integration,
  size, scripts, configuration, and documentation pass with no known locally actionable defect.
- **Testnet-ready:** local readiness plus finalized non-sentinel inputs, verified network contracts,
  a clean bound plan, and passing pinned-fork simulation.
- **Launch-ready:** testnet evidence plus the required independent security/economic review,
  operational monitoring, and explicit authorization for the named network and wallet actions.

Report external audit findings, missing RPC access, unfinished economic inputs, and live authority as
stage gates. Claim only the highest stage whose complete evidence is present.
