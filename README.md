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
- one flattened `vendor/` directory without Git history or submodules
- the official Foundry starter contracts, scripts and tests

Commit both metadata files. The CLI uses them to update the scaffold safely.

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
  --version 1.1.0 \
  --source Uniswap/v4-template \
  --reference main
```

The command resolves `main` to one commit. It records that commit and every dependency commit in the scaffold lock.

It downloads GitHub archives instead of cloning repositories. It then flattens the dependencies into `vendor/`.

The refresh preserves the v4hook deployment integration. It tests the prepared scaffold before replacing `assets/v4-template`.

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

Put the endpoint in `.env`. Keep the variable name from `network.rpcUrlEnv`; the example uses `BASE_SEPOLIA_RPC_URL`. Do not commit `.env` or share one provider key across generated projects.

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

The plan records the fork block and hashes the deployed Uniswap contracts. It also mines and checks the required CREATE2 address flags.

Any source, configuration, artifact or network change makes the plan invalid.

Interactive commands print a concise result and write progress to stderr. Redirected stdout is JSON, so scripts can parse it without progress messages. Pass `--json` anywhere in the command to request JSON explicitly:

```sh
v4hook doctor --json --config v4hook.config.json
```

## Deploy to Base Sepolia

Base Sepolia is the recommended first live network for this project. Its chain ID is `84532`.

Check the current addresses in the [Uniswap v4 deployment registry](https://developers.uniswap.org/docs/protocols/v4/deployments) before you create a plan. Do not assume that Uniswap uses the same address on each chain.

The example configuration uses these Base Sepolia addresses:

| Contract | Address |
| --- | --- |
| PoolManager | `0x05E73354cFDd6745C338b50BcFDfA3Aa6fA03408` |
| PositionManager | `0x4b2c77d209d3405f41a037ec6c77f7f5b8e2ca80` |
| Universal Router | `0x492e6456d9528771018deb9e87ef7750ef184104` |
| Quoter | `0x4a6513c898fe1b2d0e78d3b0e0a4a151589b1cba` |
| StateView | `0x571291b572ed32ce6751a2cb2486ebee8defb9b4` |
| Permit2 | `0x000000000022D473030F116dDEE9F6B43aC78BA3` |
| CREATE2 deployer | `0x4e59b44847b379578588920cA78FbF26c0B4956C` |

Use a separate testnet account. Import it into the Foundry keystore:

```sh
cast wallet import deployer --interactive
```

Get test ETH from a [Base Sepolia faucet](https://docs.base.org/base-chain/network-information/network-faucets). Copy the example environment file and fill in the RPC, explorer key and public deployer address:

```sh
cp .env.example .env
set -a
. ./.env
set +a
```

The RPC and explorer values are credentials. The deployer address is public. Never add a private key, mnemonic or keystore password to `.env`; import the key interactively into Foundry instead. For production mainnet deployments, prefer a hardware wallet after the CLI supports the required signer flow.

Create and simulate a fresh plan. Then deploy the hook:

```sh
v4hook deploy \
  --plan .v4hook/deployment-plan.json \
  --account deployer \
  --sender "$DEPLOYER_ADDRESS" \
  --confirm 'DEPLOY:84532:0xpredictedhook' \
  --verify
```

`plan` prints the predicted hook address. Use that address in the confirmation value.

The deploy command performs these checks before broadcasting:

- reruns all configured checks
- starts Anvil at the exact planned Base Sepolia block
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
  --confirm 'POOL:84532:0xhook:sha256:pool-plan-digest'
```

The launch command reruns the pool simulation. It also runs the configured read-only live verification after broadcasting.

Run small live swaps after launch. Cover both swap directions and both amount modes. Keep the amounts capped and check hook state after every transaction.

## Deploy to mainnet

Test the complete process on a supported testnet first.

Ethereum mainnet requires both the exact confirmation value and `--mainnet`. The CLI does not accept private keys on the command line or read them from `.env`.

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
