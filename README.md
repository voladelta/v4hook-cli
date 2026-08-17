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
  --version 1.2.0 \
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
```

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
