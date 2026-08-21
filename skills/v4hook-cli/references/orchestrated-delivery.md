# Chief-led delivery

Use this workflow for a complete hook implementation or material adaptation when multi-agent
delegation is available. The chief owns the parent task contract, workflow-convergence ledger,
architecture, integration order, accepted evidence, and terminal status. Children receive bounded
task contracts and never inherit parent completion authority.

For ordinary one-agent changes, keep the direct `inspect → implement → verify` path.

## Use role profiles

When the runtime permits model selection, use these defaults:

| Role | Model | Reasoning | Responsibility |
| --- | --- | --- | --- |
| Chief | `gpt-5.6-sol` | high | Parent contract, architecture, ledger, integration, terminal decision |
| Scout | `gpt-5.6-luna` | xhigh | Narrow documentation or pinned-API research; read-only |
| Implementor | `gpt-5.6-sol` | medium | Bounded production slices and focused proof |
| Reviewer | `gpt-5.6-sol` | high | Fresh adversarial review; read-only |
| Fixer | `gpt-5.6-sol` | medium | Accepted root causes and focused regression proof |
| Verifier | `gpt-5.6-sol` | high | Independent unchanged gates; read-only |

Record the actual profile when a default is unavailable. Profile availability does not weaken the
parent gate, and it does not justify restarting verified work.

## Keep one chief

After project-local inspection, the chief reads `task-contracts` and `workflow-convergence`, writes
the smallest sufficient parent contract, and creates the ignored ledger from
[local-workflow.md](local-workflow.md). The chief crafts every child contract, inspects each returned
claim, integrates accepted work, and alone classifies the parent as Complete, Escalated, or
Blocked.

Use one writer at a time in a shared worktree. Parallel writers require isolated worktrees. Every
writer contract explicitly grants or withholds Git working-tree, index, and commit authority. When
it grants none of those, the child returns an uncommitted patch or findings and the chief alone
integrates, stages, and commits them.

Route each child to only the domain skill and reference needed for its owned boundary. The chief
already encodes orchestration in the child contract, so children do not reload `task-contracts` or
`workflow-convergence` unless one is explicitly appointed as a coordinator.

Children do not spawn descendants unless their child contract grants that authority for one named,
independent frontier action. Otherwise they return the newly discovered need to the chief.

Every dispatch uses the smallest task-contract envelope that makes ownership and proof unambiguous:

```text
Destination:
Anchors and supplied context:
Owned files or read-only surface:
Dependencies and invariants:
Authority:
Git working-tree, index, and commit authority:
Proof and evidence to return:
Handoff point and child gate:
```

Omit fields that do not change execution. Each child gate advances one named parent requirement and
ends before parent completion.

## Scout unresolved boundaries

Dispatch one or two scouts in parallel only for questions that project inspection did not answer.
Split by independent boundary, such as pinned PoolManager settlement and an external oracle
lifecycle. A scout owns no tracked files and returns:

```text
Question and parent requirement:
Exact source anchors:
Constraints and traps:
Recommended decision:
Remaining uncertainty:
```

The chief accepts or rejects each recommendation and records the result before it becomes an
implementation assumption. Do not dispatch a scout for facts available in the generated project
map, owned source, configuration, installed `--help`, or one pinned symbol lookup.

## Implement vertical slices

Use one implementor by default. Add a second writer only when interfaces are frozen, owned files do
not overlap, and each workstream has independent proof and a defined integration order. Hook
callbacks, settlement, token accounting, and shared custody normally stay with one owner.

The implementor works in vertical slices:

```text
parent requirement → failing behavioral proof → smallest production change → focused green
```

Its child contract names the accepted architecture, owned files, external interfaces, frozen
verification constraints, and integration handoff. It authorizes only that production and test
surface plus non-destructive local checks, and explicitly states its Git working-tree, index, and
commit authority.

Start with compiling interfaces and one real production-path slice. Add its rollback or hostile
case before moving to the next slice. Run formatting, compilation, focused tests, structured static
analysis, and small fuzz or invariant smoke workloads as soon as each becomes relevant. Do not use
the complete configured check as a development loop; smoke workloads supplement rather than replace
the frozen full counts.

The implementation handoff is a clean candidate commit whose production path, focused integration
suite, static policy, and smoke properties are green. The implementor reports exact commands,
counts, changed files, assumptions, and gaps; it does not claim parent completion.

## Review before the expensive gate

Dispatch a fresh reviewer for the exact focused-green candidate before full configured fuzz and
invariant workloads. Give it the parent contract, protected-invariant ledger, frozen workload,
baseline and candidate identities, and candidate diff—not the implementor's narrative.

The reviewer treats tracked source as read-only but may write its ignored preliminary findings to
`.v4hook/adversarial-findings.json`:

```text
Destination: classified preliminary findings for the exact candidate commit.
Proof: every finding names the violated parent requirement, exact anchor, impact, and reproducer or
missing evidence; compare verification scope and production causality with the frozen baseline.
Child gate: every material requirement is preserved, explicitly approved, or a must-fix. Never
claim parent completion.
```

The chief treats the findings as claims. Accepted must-fix findings move to a fresh fixer. Rejected
findings retain a rationale in the ledger. The reviewer does not adjudicate on the chief's behalf or
guess the `candidateSource.treeDigest` that the first complete check will expose.

## Repair accepted findings

Give one fresh fixer only the accepted findings, anchors and reproducers, frozen verification
constraints, and exclusive file ownership:

```text
Destination: repair the accepted root causes in one clean candidate commit.
Authority: edit only the owned project surface; preserve the parent contract, frozen workload, and
external-action boundaries.
Proof: make the closest production-causal proof red for the expected reason, repair it, rerun the
focused gate, and report the diff and exact results.
Child gate: a clean candidate ready for fresh review. Never mark the parent Complete.
```

Every source change invalidates the prior review. Dispatch a fresh reviewer for the repaired commit
and repeat `review → accepted repair → focused proof` until the exact candidate has no accepted
must-fix finding. A trivial non-production correction may remain with the chief only when its owner,
cause, and proof are unambiguous.

## Verify the reviewed candidate

Give a fresh verifier the parent contract, frozen manifest, reviewer-clean preliminary findings,
exact candidate identity, and retained installed CLI path. Source, tests, and configuration are
read-only; running gates and updating ignored CLI verification state are allowed.

The verifier runs the closest accepted reproducers, then follows the exact command sequence in
[local-workflow.md](local-workflow.md). After first green exposes `candidateSource`, the chief
inspects that identity, the final reviewer evidence, and the verification state and authors the
strict v1 JSON adjudication. The verifier treats that report as read-only, binds it, and runs the
unchanged second check.

It inspects exact test names and counts, skips, settings, static-analysis policy, sizes, gas, source
identity, report digest, and lifecycle state. It returns verified evidence or one precise mismatch;
it never repairs the source or claims parent completion.

A verifier mismatch returns to the chief. Any source repair goes through a fixer, focused proof,
fresh review, and fresh verification. Repeat an action only with a progress delta or changed
hypothesis. Run the expensive complete gate only when every cheaper relevant gate is green.

## Finish at the parent

The chief independently inspects the verifier's evidence, clean source identity, bound review
digest, and lifecycle state. Only the chief may classify:

- **Complete:** every parent deliverable is integrated and the required lifecycle is green.
- **Escalated:** a user decision, approval, or grant of new authority can unblock the work. This
  classification takes precedence over Blocked.
- **Blocked:** a technical, environmental, access, or external-dependency limit that user input or
  authority cannot resolve leaves no autonomous next action.

Local green, a child report, or an unbound review is never parent green.
