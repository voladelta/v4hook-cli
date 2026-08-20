# Local workflow

Use the installed command's `--help` output as the syntax authority. This reference defines local
project creation, inspection, and verification evidence.

## Create or inspect

- Initialize only into a path that does not exist, then read the generated project instructions and
  pinned dependency metadata.
- Resolve and retain one explicit CLI path. Preview scaffold updates and compare the CLI's embedded
  template with the project's pinned version before applying one; preserve the newer template.
- Derive the working configuration from the tracked example and replace every project-specific
  placeholder. The example does not establish the hook, tests, tokens, or target network.
- A tracked `.env.example` may document non-keyed public RPC URLs. Copy required values into ignored
  local state, probe chain ID and pinned-block availability, and replace rate-limited endpoints with
  dedicated archive-capable infrastructure before treating network evidence as launch evidence.
- Run `doctor` before expensive verification.

Local inspection is complete when the CLI accepts the active configuration, required tools are
accounted for, placeholders are resolved for the authorized stage, and no secret or authenticated
endpoint entered tracked state.

## Repair missing Slither tooling

When `doctor` reports that `slither` is missing, prefer an isolated uv tool installation:

```sh
uv tool install slither-analyzer
```

For a one-off independent run, use:

```sh
uvx --from slither-analyzer slither . --filter-paths 'vendor/' --fail-high
```

A one-off run does not satisfy a configured executable gate. Install the tool or intentionally
configure the full uvx invocation. Keep dependency filtering, failure severity, output handling, and
accepted-finding fingerprints in the structured Slither policy rather than adding those flags to the
base configured analyzer command. When uv itself is unavailable, use an existing trusted package
manager or report the blocker; installing it through a piped remote script requires explicit user
authorization. Preserve static analysis when the tool is unavailable: run the remaining gates
individually and report the exact blocker.

## Run the complete check

After focused tests, run the configured `check`. It must cover formatting, lint, structured Slither,
build, runtime and initcode size, the committed gas snapshot, unit tests, fuzz tests, and invariant
tests. Inspect the emitted evidence rather than relying on process success:

- Every configured Foundry filter executes at least one matching test and no required test skips.
- The workload floors and structured Slither policy in
  [integration-contract.md](integration-contract.md) are satisfied.
- The final effective Foundry settings and executed counts meet or exceed the pre-edit baseline.

The check stops at the first failed command. Classify each failure as implementation,
configuration, script, test, local tooling, launch input/network, live authority, or external
assurance. Record the violated contract, failing input, expected behavior, and observed behavior
before changing anything. Repair every locally actionable defect, rerun its narrow reproducer, then
rerun the complete configured check.

The complete check is green only when its intended workloads execute, every configured gate passes,
and no known locally actionable defect remains. Missing external assurance or inputs remain
blockers; they do not turn a local failure into residual risk.
