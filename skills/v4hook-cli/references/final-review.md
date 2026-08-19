# Final adversarial review

Run this review after the first complete green check. Treat the implementation, tests, scripts,
configuration, and reported results as untrusted evidence. A passing command is insufficient when
its workload changed, its filters matched nothing, or its tests skipped the lifecycle under review.

When delegating this review, hand the reviewer the original requirements, the protected-invariant
ledger, the pre-edit verification baseline, and the final diff. Require findings against those
artifacts rather than a summary of the implementation, and treat the returned report as evidence
to re-verify, not as completion.

## Reconstruct the intended system

Re-read the user's request and any reference README, architecture, economics, and security files.
Compare the final design against the protected-invariant ledger made before implementation.

For every material difference, classify it as:

- preserved with direct code and test evidence;
- explicitly changed with user approval; or
- a must-fix regression.

Pay particular attention to liquidity provenance and manipulation resistance, fee allocation,
custody and recovery, role separation, supported swap quadrants, exact-output behavior, and native
currency paths. Do not relabel a missing load-bearing precursor as an accepted residual risk.

## Prove verification was not weakened

Compare the pre-edit and final effective `forge config --json` values and configured commands.
Record at least:

| Gate | Evidence to compare |
| --- | --- |
| Unit/integration | filters and executed/pass/skip counts |
| Implementation causality | production interface and owner exercised; targeted negative control fails as expected |
| Fuzz | runs per test and matched test count |
| Invariant | runs, depth, fail-on-revert, calls, reverts and discards |
| Static analysis | detector set, project-path filters and finding triage |
| Size/gas | runtime and initcode ceilings, committed snapshot and reviewed deltas |
| Fork simulation | stage environment, executed test count and postconditions |

Reject any unapproved reduction, including adding explicit values below inherited defaults,
narrowing filters, deleting assertions, swallowing reverts, accepting only one successful handler
call, broad detector exclusions, or converting a required failure into a skip.

If an invariant handler catches expected reverts, require exact accounting for attempted,
successful, expected-revert and unexpected-failure actions. Assert the expected relationship for
every action class, not merely that some call succeeded.

## Exercise lifecycle boundaries honestly

Deployment simulation exposes `V4HOOK_PREDICTED_ADDRESS`; pool simulation exposes
`V4HOOK_HOOK_ADDRESS`. Tests shared between stages must resolve the address supplied by that stage.
They may skip in an ordinary local suite only when neither lifecycle variable is present. Once a
simulation-stage variable exists, missing calldata, actors, balances, or state must fail visibly.

Run the exact configured stage command in its real environment and confirm that the intended tests
executed without skips. A compiling fork test or a local-fixture substitute is not fork evidence.

Build a signer matrix from constructor/config roles and every broadcasted call. One broadcast may
combine calls only when the configured signer is authorized for all of them. Distinct registrar,
treasury, owner, deployer, and administrator roles require distinct stages unless the user
explicitly chose the same address and the active configuration proves that choice.

## Review static-analysis evidence

Keep project detectors visible. Filter pinned dependency paths, then triage project findings by
detector, exact source location, impact, and rationale. Do not silence a detector class globally to
hide a known project finding. Require the structured fingerprint to match the current source and
reject stale allowances. Preserve fail-closed handling for the severities required by project
policy. Compare the final code-size evidence and committed gas snapshot against the original design;
an unexplained budget increase is a must-fix review finding.

## Finish the second pass

Repair every must-fix item, rerun its narrow reproducer, rerun the complete configured check, and
repeat the comparison above. Report before/after gate settings and actual executed/pass/skip counts.
Only residual external assurance, unavailable launch inputs, or actions outside the user's granted
authority may remain; locally actionable defects are not residual risks.
