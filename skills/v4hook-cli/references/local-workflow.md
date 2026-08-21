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

## Converge on parent green

After freezing the verification contract for an implementation or material adaptation, keep one
authoritative ignored ledger at `.v4hook/convergence-ledger.md`:

```text
Parent gate status:
Accepted and rejected evidence:
Open assumptions:
Workstream dependencies:
Approval gates and blockers:
Frontier:
Latest failure and hypothesis:
Selected next action and verifier:
Active child contract and ownership:
Latest verified checkpoint:
```

Update it after each material observation and accepted checkpoint. Recover from the ledger plus the
actual Git and verification state, not from a conversational summary. Keep only authorized,
prerequisite-complete actions on the frontier.

Cycle `diagnose → focused proof → integrate → review → full gate`. A focused pass advances
evidence; only the configured full check advances the CLI verification lifecycle. In chief-led
delivery, obtain a fresh reviewer-clean candidate before spending the full configured workload. A
source change after review invalidates that report and adds fresh review, first green, bound review,
and the unchanged second check back to the frontier. Repeat only when new evidence changes the
hypothesis or verifier. Classify the parent outcome directly:

- **Complete:** the requested local behavior and required verification lifecycle are green.
- **Escalated:** an autonomous next action exists only after a user decision or grant of new
  authority.
- **Blocked:** a technical, environmental, access, or external-dependency limit leaves no
  autonomous next action under current authority.

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

## Bind a completed implementation

For a completed hook or material adaptation, use the CLI verification lifecycle rather than
reporting a raw `check` result as completion:

1. Before production edits, replace `verification-contract.example.json` with a tracked contract
   mapping every protected invariant to exact configured test names or one explicit `externalGap`.
   Commit the prepared scaffold, ledger, configuration, and contract with a clean worktree.
2. Run `v4hook verification freeze --config v4hook.config.json --contract
   verification-contract.json`. The state freezes the tracked baseline, check configuration,
   effective `forge config`, and verification-contract digest.
3. Implement and commit the candidate. Review that exact commit and write the v1 JSON report below
   under `.v4hook/`; repair and re-review changed source until the chief adjudicates the candidate
   reviewer-clean.
4. Run `v4hook verification check --config v4hook.config.json`; this is first-green evidence only.
   Bind the existing exact-candidate report with `v4hook verification review --report
   .v4hook/adversarial-review.json`.
5. Run the same verification check again. Only the `complete` state proves that the reviewed source
   digest passed twice. Any committed source change replaces the candidate with a new first-green
   cycle; fresh review, bound review, and second green must then be repeated.

The contract validates exact names emitted by the configured unit, fuzz, and invariant gates. A
general file or suite reference is not a mapping. Keep the state file and review report together;
the second check verifies the report digest before advancing. Completion binds the entire tracked
tree, including documentation. Report results in the final response without editing tracked files;
a required tracked report is a source change and therefore starts a new verification cycle.

The report is strict JSON. Copy `candidateSource` from the exact clean candidate identity and copy
the frozen baseline and three digests from `.v4hook/verification-state.json`:

```json
{
  "schemaVersion": "v4hook.verification-review.v1",
  "candidateSource": {
    "commit": "<candidate commit>",
    "dirty": false,
    "treeDigest": "sha256:<candidate tree and submodules digest>"
  },
  "frozenBaseline": {
    "commit": "<baseline commit>",
    "dirty": false,
    "treeDigest": "sha256:<baseline tree and submodules digest>"
  },
  "verificationContractDigest": "sha256:<frozen contract digest>",
  "checksDigest": "sha256:<frozen checks digest>",
  "foundryConfigDigest": "sha256:<frozen Foundry config digest>",
  "chiefAdjudication": {
    "decision": "reviewerClean",
    "rationale": "All findings are resolved or rejected with the evidence recorded below."
  },
  "findings": [
    {
      "id": "review-1",
      "summary": "<finding summary>",
      "disposition": "resolved",
      "rationale": "<non-empty chief disposition rationale>"
    }
  ]
}
```

`findings` may be empty. Each listed finding needs a unique non-empty `id`, non-empty `summary`, and
exactly one supported disposition (`resolved` or `rejected`) with a non-empty rationale. The CLI
rejects a different candidate, baseline, contract digest, checks digest, Foundry-config digest,
decision, unknown field, or changed report before advancing to `reviewed`.
