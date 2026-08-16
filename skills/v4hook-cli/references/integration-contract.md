# Scaffold integration contract

Use this reference when editing a v4hook-managed Foundry project.

## Preserve project ownership

- Preserve `.v4hook.toml` and `.v4hook-template-lock.json`.
- Do not edit `vendor/` to customize a hook.
- Preserve unrelated user changes in a dirty worktree.
- Preview scaffold updates before applying them when user files may conflict.
- Read remappings and actual base classes instead of relying on online import paths.

## Keep these artifacts aligned

| Concern | Required locations |
| --- | --- |
| Contract identity | Solidity name, artifact path, deployment script |
| Constructor | Solidity signature, ABI-encoded config value, script arguments |
| Permissions | `getHookPermissions`, config names, CREATE2 flags, tests |
| Network | Chain ID, RPC environment name, official deployed contracts |
| Test gates | Unit, fuzz, invariant commands that execute matching test types |
| Simulation | Deploy, pool, swap quadrants, postconditions |

Return-delta permissions require their parent callback. Let configuration validation and
`v4hook plan` derive address flags; never hardcode a salt or predicted address as a shortcut.

Keep the active config and tracked `v4hook.config.example.json` aligned for non-secret contract
identity, constructor encoding, permissions, checks, scripts, and pool behavior. Put RPC values and
other secrets only in ignored local state or environment variables.

## Preserve verification integrity

- Never replace Slither or another configured gate with `forge lint` because the tool is missing.
  Preserve the gate, run the remaining commands individually, and report the missing tool.
- Never reduce fuzz runs, invariant depth, matching filters, or assertions to make a check pass.
- If `forge lint` leaves ABI-only artifacts and a later Foundry test reports no tests, run
  `forge build --force` and rerun the original gate. Add `--force` to a configured test command only
  when the cache failure is reproduced; still verify that matching tests executed.

## Implement against pinned BaseHook

Inspect the exact base contract. In the bundled OpenZeppelin pattern, external callbacks apply
`onlyPoolManager` and dispatch to internal implementation methods. Override the internal method and
verify the inherited wrapper remains intact. If another pinned base changes this pattern, follow
that base and document the caller path.

Treat the callback `sender` as router context, not automatically as the end user. Authenticate
hook data only through a router whose encoding behavior is trusted and tested.

## Build the tests

Cover at least:

- Every enabled callback and every material revert.
- Permission equality and correct address flags.
- Zero values, boundary values, signed amount modes, and both token orderings.
- Stateful fuzzing for amount, tick, fee, and hook-data boundaries.
- Invariants for solvency, balanced deltas, access control, and recoverable liquidity.
- Both swap directions with exact input and exact output on the pinned fork.
- Postconditions for balances, hook state, PoolManager accounting, and permissions.

Add adversarial token and reentrancy cases when the hook transfers tokens or calls external code.
Increase fuzz depth for return deltas, custom curves, custody, or privileged fee changes.

## Wire simulation honestly

Deployment simulation supplies `V4HOOK_PREDICTED_ADDRESS`, `V4HOOK_HOOK_SALT`, and
`V4HOOK_CONSTRUCTOR_ARGS`. The separate pool lifecycle supplies `V4HOOK_HOOK_ADDRESS`. Pool scripts
must resolve the correct hook address for both stages without embedding an address.

Do not point swap-quadrant or postcondition steps at ordinary unit tests merely because the scaffold
contains placeholders. Those steps must inspect the hook deployed by the plan on the pinned Anvil
fork. If simulation is outside scope, leave it explicitly unvalidated and do not claim readiness.

## Route external guidance

Apply project `AGENTS.md` instructions. Use `v4-security-foundations` for hook threat modeling and
review. Load official Ethereum and Foundry guidance for Solidity and testing. Load official
Uniswap guidance for current v4 behavior and deployments. Load a Chainlink skill only when its
product enters the design; use the VRF skill for verifiable randomness.
