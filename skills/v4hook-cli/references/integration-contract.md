# Scaffold integration contract

Use this reference when editing a v4hook-managed Foundry project.

## Preserve project ownership

- Preserve `.v4hook.toml` and `.v4hook-template-lock.json`.
- Do not edit `vendor/` to customize a hook.
- Preserve unrelated user changes in a dirty worktree.
- Preview scaffold updates before applying them when user files may conflict.
- Read remappings and actual base classes instead of relying on online import paths.
- Treat Hookmate as third-party scaffold/test tooling when it is present. Its address constants and
  `V4Router04` are not authoritative live deployment data and the router is not an ABI-compatible
  substitute for Uniswap Universal Router. The bundled first-party `v4hook-testkit` retains only
  attributed, pinned local fixture bytecode and Uniswap v4-core's `PoolSwapTest`. Use the official
  registry and plan-bound addresses for remote networks.
- Verify router identity by address, deployed code, and ABI. A helper with a router-like name or an
  overlapping swap method is not evidence that it exercises the production integration.

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

Treat signer identity as part of the integration contract. A broadcast script may group calls only
when one signer is authorized for all of them. Split registrar-only, treasury-only, owner-only, and
admin-only operations into separate stages when the roles differ. Declare the role/address pairs in
the step's `requiredAuthorities`; use `deployment.requiredAuthorities` and
`pool.launchAuthorities` for live scripts. An empty declaration means the CLI cannot validate
contract-specific signer compatibility, not that every signer is authorized.

Keep the active config and tracked `v4hook.config.example.json` aligned for non-secret contract
identity, constructor encoding, permissions, checks, scripts, and pool behavior. Put RPC values and
other secrets only in ignored local state or environment variables.

## Preserve verification integrity

- Never replace Slither or another configured gate with `forge lint` because the tool is missing.
  Preserve the gate, run the remaining commands individually, and report the missing tool.
- Filter pinned `vendor/` paths from Slither detector output, fail on high-severity project
  findings, and keep an explicit source-location triage for accepted lower-severity false
  positives. Put dependency directories and exact finding fingerprints in the structured Slither
  policy. High findings cannot be allowed; moved findings and stale allowances fail. Do not add
  output, fail, filter, or detector-exclusion flags to the base analyzer command. Do not ignore
  dependency pinning or compiler-known-bug review merely because pinned dependencies are filtered.
- Keep `.gas-snapshot` committed and configure only a failing `forge snapshot --check` command.
  Review any snapshot update as a behavioral budget change. Keep runtime and initcode ceilings at or
  below the protocol limits and prefer project-specific lower ceilings when architecture permits.
- Before editing, capture effective Foundry fuzz and invariant values from `forge config --json` and
  the configured gate commands. Never reduce runs, invariant depth, fail-on-revert behavior,
  matching filters, assertions, or executed test counts to make a check pass. Compare the final
  effective values and counts against that baseline. Keep the configured workload floors at or
  above 1,000 fuzz runs and 256 invariant campaigns at depth 500; inspect the actual counts emitted
  in check evidence.
- Never catch an expected revert inside `broadcast` or `startBroadcast`; Foundry records calls at
  that depth as transactions. Move the probe outside broadcast or use a read-only quoter.
- Confirm dependent broadcast receipts sequentially and use a bounded future deadline for
  price-sensitive calls; an exact `block.timestamp` deadline can expire during broadcast.
- Use strict interior v4 price limits. When an exact-output return-delta hook requires a gross
  witness, derive it from an authoritative quote or decode the hook's atomic wrapped rejection
  instead of guessing candidates or swallowing reverts.
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

Design coverage from the requirement and protected-invariant ledger, not only from the current
implementation or test suite. Exercise behavior through the intended public production interface
and keep each production decision in its declared owner. Mocks and fixtures may isolate external
boundaries, but must not replace or duplicate the business logic whose implementation is requested.

Cover at least:

- Every enabled callback and every material revert.
- Permission equality and correct address flags.
- Zero values, boundary values, signed amount modes, and both token orderings.
- Stateful fuzzing for amount, tick, fee, and hook-data boundaries.
- Invariants for solvency, balanced deltas, access control, and recoverable liquidity.
- Differential checks against a reference implementation or an independent calculation for any
  economic rule that has one, such as a published fee or curve formula.
- Both swap directions with exact input and exact output on the pinned fork.
- Postconditions for balances, hook state, PoolManager accounting, and permissions.

Add adversarial token and reentrancy cases when the hook transfers tokens or calls external code.
Increase fuzz depth for return deltas, custom curves, custody, or privileged fee changes.
If an invariant handler catches reverts, count attempted and successful actions and assert the
expected success relationship. A nonzero-call assertion alone can pass while every action fails.

For each load-bearing behavior whose evidence depends on tests, perform a targeted negative control:
temporarily disable or perturb the intended production implementation, confirm that the relevant
tests go red for the expected reason, restore the implementation, and make them green again. A test
that remains green when its intended production behavior is broken is not evidence for that
behavior. Leave no temporary mutation in the worktree.

## Wire simulation honestly

Deployment simulation supplies `V4HOOK_PREDICTED_ADDRESS`, `V4HOOK_HOOK_SALT`, and
`V4HOOK_CONSTRUCTOR_ARGS`. The separate pool lifecycle supplies `V4HOOK_HOOK_ADDRESS`. Pool scripts
must resolve the correct hook address for both stages without embedding an address.

Fork tests reused across stages must accept `V4HOOK_HOOK_ADDRESS` when present and otherwise use
`V4HOOK_PREDICTED_ADDRESS`. They may skip only when neither is present in an ordinary local run.
With either lifecycle address present, missing stage inputs must fail rather than skip. Verify the
configured simulation reports nonzero executed tests and zero unexpected skips.

Do not point swap-quadrant or postcondition steps at ordinary unit tests merely because the scaffold
contains placeholders. Those steps must inspect the hook deployed by the plan on the pinned Anvil
fork. If simulation is outside scope, leave it explicitly unvalidated and do not claim readiness.
