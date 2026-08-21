# Delegated review, repair, and verification

Use this loop after first green when subagent delegation is available. The coordinator may itself
be a child agent; it still owns the parent contract, ledger, integration order, and terminal status.
Use `task-contracts` to write every child brief. Descendants receive only the authority and context
their role needs and do not inherit parent completion authority.

## Choose ownership

A coordinator may repair a trivial finding directly when the cause and production owner are
unambiguous and the change is isolated. A finding is material, and therefore gets a dedicated
fixer, when it touches Solidity callbacks, economic or accounting logic, custody, authorization,
deployment provenance, an external integration, a public API, or verification scope.

Keep reviewer, fixer, and verifier ownership serial within one project. The coordinator may advance
other frontier actions concurrently only when their repositories, mutation surfaces, inputs, and
proof are independent. Record the active child contract and owned surface in the convergence
ledger. Give one child sole write ownership during repair; reviewers never edit tracked source, and
verifiers never repair what they test.

## Write bounded child contracts

Unless the parent contract selects another execution profile, dispatch every reviewer, fixer, and
verifier with model `gpt-5.6-sol` and reasoning effort `high`. If
that exact profile is unavailable, record the mismatch and retain the work locally or escalate; do
not silently downgrade a security-critical delegated role.

Give a fresh reviewer the original parent contract, protected-invariant ledger, frozen workload,
baseline and candidate identities, and candidate diff—not the implementor's narrative. Its contract
must require:

```text
Destination: a classified report for the exact candidate commit.
Authority: tracked source is read-only; an ignored review report may be written.
Proof: each finding names the violated parent requirement, exact anchor, impact, and reproducer or
missing evidence; compare final commands, settings, executed names/counts, and production causality
with the frozen baseline.
Child gate: every material requirement is preserved, explicitly approved, or a must-fix. Never
claim parent completion.
```

Give a fixer only parent-accepted findings, their anchors and reproducers, the frozen verification
constraints, and explicit file ownership. Its contract must require:

```text
Destination: the accepted root causes are repaired in one clean candidate commit.
Authority: edit only the owned project surface; preserve the parent contract, frozen workload, and
external-action boundaries.
Proof: make the closest production-causal proof red for the expected reason, repair it, rerun the
focused gate, and report the diff and exact results.
Child gate: a clean commit ready for independent verification. Never mark the parent Complete.
```

Give a verifier the parent contract, frozen manifest, accepted findings and claimed repairs, exact
candidate identity, and retained installed CLI path. Its contract must require:

```text
Destination: independent evidence for the exact candidate and every accepted repair.
Authority: source, tests, configuration, and reports are read-only; running configured gates and
updating ignored CLI verification state are allowed.
Proof: run the closest repaired reproducers, then the unchanged configured full check; inspect exact
test names/counts, skips, settings, static-analysis policy, sizes, gas, source identity, and state.
Child gate: return verified evidence or a precise mismatch. Never repair or claim parent completion.
```

## Converge serially

1. Record first green for a clean candidate.
2. Dispatch a fresh reviewer and inspect its report as a claim.
3. For every accepted must-fix, assign one fixer or perform a qualifying trivial repair. Commit the
   repaired source, then dispatch a verifier. A changed source produces a new first-green candidate.
   A missing parent or verification requirement is not a source repair: the coordinator preserves
   the prior evidence, amends the tracked contract, and starts a new frozen lifecycle.
4. Repeat with a fresh reviewer for that new candidate. Do not reuse a report from the prior source.
5. When a fresh review has no must-fix, bind its ignored report with `v4hook verification review`,
   then dispatch a verifier for the unchanged second check.
6. The coordinator independently inspects the returned evidence, clean source identity, bound review
   digest, and lifecycle state. Only then may it classify the parent as Complete.

Return a mismatch to the ledger and frontier. Repeat only with a progress delta or changed
hypothesis. Classify the parent as Escalated or Blocked under the terminal rules in
[local-workflow.md](local-workflow.md) when no authorized evidence-producing action remains.
