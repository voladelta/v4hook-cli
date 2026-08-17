# v4hook CLI

`v4hook` checks, simulates and deploys Uniswap v4 hooks from Foundry projects.

The CLI binds your source, compiler output, constructor arguments, hook permissions and network contracts into a deployment plan. It then tests that exact plan on a pinned Anvil fork before it broadcasts a transaction.

A successful run is not a security audit. It shows that the configured checks passed for the planned code and network state.

## Install the CLI

You need:

- Rust 1.97.1
- Foundry with `forge`, `cast` and `anvil`
- Git
- a static analyser such as Slither

Install the locked, release-optimised binary in `~/.local/bin`:

```sh
./install.sh
```

Set `V4HOOK_INSTALL_ROOT` to use another installation root. Its `bin` directory must be in your `PATH`.

Remove the default installation with:

```sh
rm ~/.local/bin/v4hook
```

The release profile favours runtime speed. It uses optimisation level 3, fat link-time optimisation and one code generation unit.

## Install Slither with uv

[uv](https://github.com/astral-sh/uv) is the recommended way to install and run Slither without
mixing Python packages into the system interpreter.

Install uv when it is not already available:

```sh
curl -LsSf https://astral.sh/uv/install.sh | sh
```

Install Slither as a user-level tool so the default `v4hook check` configuration can invoke the
`slither` executable:

```sh
uv tool install slither-analyzer
```

For a one-off analysis without installing the tool, run:

```sh
uvx --from slither-analyzer slither . --filter-paths 'vendor/' --fail-high
```

Do not replace the configured Slither gate with `forge lint`. Run both: Foundry lint catches
compiler-aware style and safety warnings, while Slither provides a separate static-analysis pass.
Review and document lower-severity findings even when `--fail-high` exits successfully.

## Create a hook project

Create a project from the bundled Uniswap v4 scaffold:

```sh
v4hook init ../my-hook
```

The command does not make a network request. It copies one pinned scaffold and initialises a normal Git repository.

The generated project contains:

- `AGENTS.md` with current Uniswap, Foundry and Ethereum guidance for coding agents
- `.v4hook.toml` with the CLI and template versions
- `.v4hook-template-lock.json` with upstream revisions and file hashes
- `v4hook.config.example.json` with the deployment, checks and simulation schema
- one flattened `vendor/` directory without Git history or submodules
- the official Foundry starter contracts plus the first-party `v4hook-testkit`

`v4hook-testkit` deploys pinned Permit2, PoolManager and PositionManager fixtures locally and swaps
through Uniswap v4-core's `PoolSwapTest`. Its retained fixture bytecode records its Hookmate source
commit and license. The scaffold intentionally excludes Hookmate's network address table and custom
router: remote scripts receive verified contract addresses from the v4hook plan, and production
router integrations must target the intended official ABI.

Commit both metadata files. The CLI uses them to update the scaffold safely.

The first-party Codex workflow is maintained in `skills/v4hook-cli`. It lets an agent turn a hook idea into a scaffolded, tested and simulated project while preserving the same launch gates as the CLI.

## Update a hook project

Install the newer CLI before you update your project. The command uses the scaffold embedded in that numbered CLI release.

Commit or stash your work, then run:

```sh
v4hook scaffold update
```

Preview the update without changing files:

```sh
v4hook scaffold update --dry-run
```

The updater handles files as follows:

- adds new scaffold files
- updates files that still match the old scaffold
- removes obsolete files only when you have not changed them
- preserves files that only you changed
- reports files changed by both you and the template
- restores the project if Foundry checks fail

When a file conflicts, the interactive command asks whether to preserve or replace it. The safe default preserves your file.

Use an explicit policy in continuous integration:

```sh
v4hook scaffold update --conflicts preserve
v4hook scaffold update --conflicts overwrite
```

The command stops in a non-interactive shell if you do not provide a conflict policy.

If `.v4hook.toml` is missing, the updater compares the project with its known scaffold. It treats every uncertain difference as a conflict.

## Refresh the bundled template

This command is for maintainers of this repository:

```sh
v4hook template refresh \
  --version 2.0.0 \
  --source Uniswap/v4-template \
  --reference main
```

The command resolves `main` to one commit. It records that commit and every dependency commit in the scaffold lock.

It downloads GitHub archives instead of cloning repositories. It then flattens the dependencies into `vendor/`.

The refresh preserves the v4hook deployment integration. It tests the prepared scaffold before replacing `assets/v4-template`.

After changing a maintained scaffold overlay without downloading a new upstream snapshot, bump the
template version and reseal its file manifest:

```sh
v4hook template seal --repository .
```

The command does not commit or push changes. Review the diff and commit the updated scaffold with the new CLI release.

Use semantic versions for templates:

- increase the patch version for compatible fixes
- increase the minor version for compatible features
- increase the major version for changes that need manual migration

## Configure a deployment

Copy `v4hook.config.example.json` into your hook repository. Update every project-specific value.

The starter shows the file format. It does not know your contract name, permissions, test files or pool tokens.

The unit, fuzz, invariant, quadrant and postcondition gates must use `forge test`. Each gate fails if its filter matches no tests. The unit, fuzz and invariant commands must also execute at least one test of their named Foundry test type.

`checks.minimumFuzzRuns`, `checks.minimumInvariantRuns` and
`checks.minimumInvariantDepth` are fail-closed workload floors. They cannot be configured below
1,000 fuzz cases or 256 invariant campaigns at depth 500. Check and simulation evidence records
the actual executed test counts and workloads. A skipped Foundry test never satisfies a gate.

The CLI owns Slither's JSON and severity flags. Keep `checks.staticAnalysis` to the executable,
target and compiler arguments. Put pinned dependency directories in
`checks.slitherPolicy.dependencyPaths`; broad detector exclusions and caller-supplied output or
failure flags are rejected. High findings always fail. Low and medium findings require an exact
source-bound fingerprint and non-empty reason in `allowedFindings`; stale allowances fail when code
moves. The failing check prints the fingerprint needed for an independently reviewed triage.

`checks.codeSize` enforces limits no larger than the EVM runtime and initcode ceilings. The
configured `checks.gasSnapshot` must be a failing `forge snapshot --check` command against a
committed snapshot. Update that snapshot only after reviewing and explaining the gas change.

For a broadcast step that depends on contract roles, declare each role and address in the step's
`requiredAuthorities` object. Use `deployment.requiredAuthorities` for live hook deployment and
`pool.launchAuthorities` for the live pool script. One broadcast has one sender, so `doctor` and
`plan` reject a group whose declared roles resolve to different addresses. Fork simulation
impersonates the declared stage authority; live commands reject a different sender. Split the
operation into separate stages when registrar, treasury, owner or administrator roles differ.

Constructor arguments must be ABI encoded. For example:

```sh
cast abi-encode "constructor(address)" "$POOL_MANAGER"
```

The simulation must contain these 4 steps:

- `deploy` deploys the planned hook to the local Anvil fork
- `pool` creates a representative pool and adds liquidity
- `quadrants` tests both directions with exact input and exact output
- `postconditions` checks balances, accounting, permissions and final state

You cannot skip the Anvil fork. `deploy` runs a fresh simulation even when you ran `simulate` earlier.

## Check and simulate a hook

Set the RPC variable named by `network.rpcUrlEnv` in your configuration. The CLI reads the process environment first and then `<projectRoot>/.env`, so each developer can use a public or authenticated provider without changing committed configuration:

```sh
cp .env.example .env
```

The checked-in example already supplies rate-limited public Robinhood Chain and Ethereum endpoints.
Keep the variable name from `network.rpcUrlEnv`; the example uses
`ROBINHOOD_MAINNET_RPC_URL`. Before planning, verify the chain ID and access to the intended pinned
block. Replace the public URL with a dedicated archive-capable provider for repeatable launch
evidence. Do not commit authenticated URLs or share one provider key across generated projects.

Then run:

```sh
v4hook doctor --config v4hook.config.json
v4hook check --config v4hook.config.json

v4hook plan \
  --config v4hook.config.json \
  --output .v4hook/deployment-plan.json

v4hook simulate \
  --plan .v4hook/deployment-plan.json \
  --output .v4hook/deployment-evidence.json

v4hook readiness \
  --config v4hook.config.json \
  --plan .v4hook/deployment-plan.json \
  --simulation .v4hook/deployment-evidence.json
```

`readiness` validates bound evidence rather than accepting a self-attestation. It reports
configuration, local and testnet stages separately. Launch readiness remains false because an
independent security/economic review, production monitoring and explicit live authorization are
external facts the CLI cannot manufacture.

## Run a persistent local devnet

Use a devnet when a browser app or multi-wallet simulator needs to keep interacting with the exact
plan-deployed hook. Unlike `v4hook simulate`, a devnet remains available after its verification
steps finish:

```sh
v4hook devnet up --plan .v4hook/deployment-plan.json
v4hook devnet status
v4hook devnet reset
v4hook devnet down
```

`devnet up` starts a pinned Anvil fork on `127.0.0.1:8545`, reruns the plan's deploy, representative
pool, quadrant and postcondition steps, and provisions 100 deterministic unlocked development
accounts by default. It writes private process state to `.v4hook/devnet.json`, Anvil logs below
`.v4hook/devnet/`, and a web-safe manifest to `.v4hook/devnet-web.json`. The manifest contains the
local RPC URL, chain and fork identity, hook ABI and address, plan-bound Uniswap addresses, optional
pool configuration, scenario names and account addresses. It never contains private keys or the
Anvil mnemonic. The CLI starts Anvil in quiet mode even when `simulation.anvilArgs` omits the flag.
If Anvil still emits a private-key or mnemonic banner, `devnet up` stops it, removes the affected
log and fails instead of retaining sensitive account material.

`devnet up` already reruns the complete plan-bound simulation. Do not run `v4hook simulate`
immediately before it unless you also need a separate one-shot evidence file.

Persistent devnets require a Unix-like host. The CLI crosses a real daemon boundary before it
returns, so Anvil survives both an interactive shell exit and a one-shot coding-agent command.

The CLI verifies a process-command fingerprint, fork block hash, on-chain ownership marker and hook
runtime before status, reset, export or scenario operations. `devnet down` refuses to signal a PID
that does not match the recorded Anvil process. `reset` restores the pinned fork and repeats the
same plan-bound bootstrap; it does not preserve interactive changes.
`devnet down` retains the generated manifest and private log for debugging. Use
`devnet down --purge-generated` to remove only the digest-verified manifest and state-owned Anvil
log after shutdown. It refuses symlinks, unrelated manifests and logs outside the generated devnet
directory. The next `devnet up` may replace only a digest-valid v4hook manifest and creates a new
private log.

Override the development account count, port or interval mining when needed:

```sh
v4hook devnet up \
  --plan .v4hook/deployment-plan.json \
  --accounts 100 \
  --port 8545 \
  --block-time 1
```

For deterministic same-block ordering, leave interval mining unset and have the scenario switch
Anvil to manual mining through its documented RPC methods. Bind only to localhost. These unlocked
accounts are disposable local identities and must never receive or reuse public-network funds.
Anvil permits browser origins by default, so any page that can reach the local RPC can mutate this
disposable chain. Stop it when it is not in use, or add `--allow-origin` and the exact development
site origin to `simulation.anvilArgs` when only one web origin should have access.

Hook-specific traffic belongs in the hook project because only that project knows its router ABI,
Permit2 flow and `hookData` encoding. Declare commands in the optional `devnet` config:

Template 2.0 requires a `verification` policy on every configured scenario. When upgrading an
older project, add the exact transaction/sender counts, allowed targets, required common hook
events and any reserved browser-account indices before running the scenario again.

```json
{
  "devnet": {
    "accounts": 100,
    "scenarios": [
      {
        "name": "market",
        "command": ["pnpm", "devnet:market", "--", "--manifest", "{devnetManifest}", "--seed", "{seed}"],
        "verification": {
          "expectedTransactions": 198,
          "expectedSenders": 99,
          "allowedTargets": ["0x1000000000000000000000000000000000000000"],
          "requiredEvents": [
            {"address": "hook", "topic0": "0x0000000000000000000000000000000000000000000000000000000000000000"}
          ],
          "reservedAccountIndices": [0]
        }
      }
    ]
  }
}
```

Run a scenario and retain hashed evidence:

```sh
v4hook devnet run --scenario market --seed 42
```

Replace the target and event topic with the intended router and real hook event. Scenario commands
receive `V4HOOK_DEVNET_RPC_URL`, `V4HOOK_DEVNET_MANIFEST`, `V4HOOK_HOOK_ADDRESS`,
`V4HOOK_SCENARIO_SEED`, `V4HOOK_DEVNET_WALLET_COUNT` and `V4HOOK_SCENARIO_REPORT`. Available command
placeholders are `{devnetRpc}`, `{devnetManifest}`, `{hookAddress}`, `{projectRoot}`, `{seed}` and
`{walletCount}` and `{scenarioReport}`. The scenario must write only this untrusted report:

```json
{"schemaVersion":"v4hook.devnet-scenario-report.v1","transactions":["0x..."]}
```

The CLI independently scans every block in the scenario range, requires the report to cover every
managed-account transaction, fetches each successful receipt, checks unique senders and allowed
targets, requires configured hook events per receipt, and proves reserved accounts kept the same
nonce and native balance. Evidence v2 retains every verified transaction. A submitted hash or
zero exit code alone is never success. A scenario command that starts and exits nonzero still
writes failure evidence; an executable that cannot be started fails before an execution record
exists.

Use the Uniswap Universal Router and Permit2 for application-parity swaps; never treat the local
`PoolSwapTest` fixture as production-router evidence. Devnet runs are development evidence and do
not replace the immutable one-shot simulation or audit required for deployment.

`doctor` also checks configured script and test paths, obvious low-address placeholders, positive
pool allocations and declared broadcast-authority compatibility. These readiness issues do not
prevent local contract checks, but they must be resolved before `plan`.

Treat the first check as the start of a repair loop, not the final report. Fix every locally
actionable contract, script, configuration, test and analyzer finding, rerun the narrow failing
gate, then rerun `v4hook check`. Create a plan only after local checks are clean and every
project-specific placeholder has been replaced. A missing RPC, unfinalized launch input,
independent audit or live-action authorization is an external readiness gate; do not misreport it
as completed or bypass it to obtain a green result.

The plan records the fork block and hashes the deployed Uniswap contracts. It also mines and checks the required CREATE2 address flags.

Any source, configuration, artifact or network change makes the plan invalid.

Interactive commands print a concise result and write progress to stderr. Redirected stdout is JSON, so scripts can parse it without progress messages. Pass `--json` anywhere in the command to request JSON explicitly:

```sh
v4hook doctor --json --config v4hook.config.json
```

## Deploy to Robinhood Chain

The shipped live-network example targets Robinhood Chain mainnet, chain ID `4663`. The public RPC is
`https://rpc.mainnet.chain.robinhood.com`; Robinhood documents it as rate-limited and unsuitable for
production infrastructure. Use it for initial reads and local forks, then use a dedicated provider
for launch evidence and broadcast.

Check the current addresses in the [Uniswap v4 deployment registry](https://developers.uniswap.org/docs/protocols/v4/deployments) before you create a plan. Do not assume that Uniswap uses the same address on each chain.

The example configuration uses these Robinhood Chain addresses:

| Contract | Address |
| --- | --- |
| PoolManager | `0x8366a39cc670b4001a1121b8f6a443a643e40951` |
| PositionManager | `0x58daec3116aae6d93017baaea7749052e8a04fa7` |
| Universal Router | `0x8876789976decbfcbbbe364623c63652db8c0904` |
| Quoter | `0x8dc178efb8111bb0973dd9d722ebeff267c98f94` |
| StateView | `0xf3334192d15450cdd385c8b70e03f9a6bd9e673b` |
| Permit2 | `0x000000000022D473030F116dDEE9F6B43aC78BA3` |
| CREATE2 deployer | `0x4e59b44847b379578588920cA78FbF26c0B4956C` |

Use a separate testnet account. Import it into the Foundry keystore:

```sh
cast wallet import deployer --interactive
```

Robinhood Chain mainnet uses real ETH. Use a dedicated, minimally funded deployment account. Copy
the example environment file and replace the public RPC with dedicated infrastructure before any
broadcast:

```sh
cp .env.example .env
set -a
. ./.env
set +a
```

Non-keyed public RPC URLs and the deployer address are not secrets. Authenticated RPC URLs and
explorer keys are credentials. Never add a private key, mnemonic or keystore password to `.env`;
import the key interactively into Foundry instead. Prefer a hardware wallet after the CLI supports
the required signer flow.

Create and simulate a fresh plan. Then deploy the hook:

```sh
v4hook deploy \
  --plan .v4hook/deployment-plan.json \
  --account deployer \
  --sender "$DEPLOYER_ADDRESS" \
  --confirm 'DEPLOY:4663:0xpredictedhook' \
  --mainnet \
  --verify
```

`plan` prints the predicted hook address. Use that address in the confirmation value.

The deploy command performs these checks before broadcasting:

- reruns all configured checks
- starts Anvil at the exact planned Robinhood Chain block
- deploys the hook and representative pool on the fork
- runs all 4 simulation steps
- compares the simulated runtime code with the planned artifact
- checks every configured Uniswap contract again
- checks that the predicted hook address remains empty

Verify the live deployment:

```sh
v4hook verify --plan .v4hook/deployment-plan.json
```

The plan has a maximum block drift. Create a new plan if the CLI reports that the fork evidence is stale.

Uniswap v4 hook permissions are encoded in the hook address. See the [Uniswap hook deployment guide](https://developers.uniswap.org/docs/protocols/v4/guides/hooks/hook-deployment) for the address flag design.

## Launch a testnet pool

Deploy the hook before you create a pool plan. The pool planner checks the live hook code and permissions.

For a private test pool:

1. Deploy 2 mintable test tokens or choose dedicated testnet tokens.
2. Put the lower token address in `currency0`.
3. Set strict token and liquidity limits in the pool configuration.
4. Mint enough tokens to the deployment account.
5. Approve only the amounts required by your launch script.

Create and simulate the pool plan:

```sh
v4hook pool plan \
  --deployment-plan .v4hook/deployment-plan.json \
  --output .v4hook/pool-plan.json

v4hook pool simulate \
  --deployment-plan .v4hook/deployment-plan.json \
  --pool-plan .v4hook/pool-plan.json \
  --output .v4hook/pool-evidence.json
```

Launch the pool with the exact confirmation value printed by the CLI:

```sh
v4hook pool launch \
  --deployment-plan .v4hook/deployment-plan.json \
  --pool-plan .v4hook/pool-plan.json \
  --account deployer \
  --sender "$DEPLOYER_ADDRESS" \
  --confirm 'POOL:4663:0xhook:sha256:pool-plan-digest' \
  --mainnet
```

The launch command reruns the pool simulation. It also runs the configured read-only live verification after broadcasting.

Run small live swaps after launch. Cover both swap directions and both amount modes. Keep the amounts capped and check hook state after every transaction.

## Deploy to mainnet

Test the complete process on a supported testnet first.

Ethereum and Robinhood Chain mainnet require both the exact confirmation value and `--mainnet`.
The CLI does not accept private keys on the command line or read them from `.env`.

RPC endpoints are also kept out of child-process arguments. The CLI configures Anvil forks through the local Anvil JSON-RPC interface and passes live endpoints to Foundry through environment variables.

Review the hook independently before you deploy valuable assets. A passing CLI run does not prove that the hook is safe.

## Develop the CLI

Run the Rust checks:

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

Test the bundled Foundry scaffold:

```sh
forge fmt --check --root assets/v4-template
forge build --root assets/v4-template
forge test --root assets/v4-template
```

Rust 1.97.1 is pinned in `rust-toolchain.toml`. Cargo dependencies use exact versions.
