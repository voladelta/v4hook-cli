# Persistent devnet

Use `v4hook devnet` only from an immutable deployment plan. A devnet is a persistent development
surface for browser apps and deterministic multi-wallet scenarios; it does not replace mandatory
one-shot deployment simulation evidence.

## Start from verified state

Complete the implementation's full check and adversarial review before entering a devnet harness
repair loop. For later harness repairs, run the narrow failing test, create a clean repair commit,
create a fresh plan, then start the devnet. Planning reruns configured checks and startup reruns all
four simulation stages; run standalone simulation when its separate evidence is itself required.

Startup is complete when it reuses the plan's pinned fork, executes all four simulation stages,
detaches Anvil across a real daemon boundary, and a separate process confirms RPC health and process
ownership.

## Preserve the local trust boundary

- Bind RPC to `127.0.0.1`. Export addresses, ABI, and disposable account addresses, never the Anvil
  mnemonic or private keys.
- Require secret-free runtime artifacts. Current releases start Anvil quietly and fail closed if a
  key or mnemonic banner reaches the log. With an older CLI, use quiet Anvil output, inspect the log
  without printing it, and remove any affected disposable log before continuing.
- Choose bootstrap authorities from the requested generated-account set. Reserve browser accounts
  before bootstrap, record their nonce and balances, and require the exported account count to match
  the request exactly.
- Treat unlocked local RPC accounts as disposable browser-accessible state. Stop the service when it
  is unused and narrow Anvil's allowed origin when the application has a stable origin.
- Verify process ownership and devnet chain identity before status, reset, export, run, or down
  operations. Stop only the verified PID owned by the recorded devnet state.

## Run deterministic scenarios

Use project-configured scenario commands for hook-specific Universal Router, Permit2, `hookData`,
and companion-contract behavior. Record a seed and deterministic ordering.

The scenario writes only `v4hook.devnet-scenario-report.v1` transaction hashes to
`V4HOOK_SCENARIO_REPORT`. Configure expected transaction and sender counts, allowed targets, required
hook events, and reserved account indices. Require CLI evidence v2 to prove every managed-account
transaction in the scenario block range was reported, mined successfully, and matched the policy.

For each write, simulate from the intended sender, apply only a documented bounded gas buffer when
concurrent state can invalidate an estimate, require a successful receipt, decode expected events,
and verify balances and hook state. A submitted hash or successful scenario process is not success
evidence.

Confirm dependent transactions sequentially and use bounded future deadlines for price-sensitive
calls. Use strict interior v4 price limits. For exact-output return-delta hooks, derive any gross
witness from an authoritative quote or the hook's atomic wrapped rejection. Use manual mining for
same-block ordering and interval mining only for browser confirmation behavior.

Scenario completion requires exact transaction completeness, successful receipts, expected senders
and targets, required hook events, reserved-account integrity, and declared state postconditions.

## Reset, stop, and hand off

Reset discards interactive state, restores the pinned fork, and reruns plan bootstrap. Purge generated
state only when debugging artifacts are no longer needed; remove only the manifest and runtime files
whose digests and ownership match the recorded devnet state.

The generated web manifest contains no secrets, but its default ignored location should normally
remain ephemeral. Commit it only when the project intentionally maintains a local-only fixture.

At handoff, identify the plan digest, listener, PID or launch-agent label, manifest, scenario evidence,
and every service intentionally retained. Otherwise stop the verified service, confirm the listener
closed, and remove or retain generated evidence according to the user's request.
