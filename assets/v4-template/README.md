# Uniswap v4 hook project

This Foundry project contains a pinned Uniswap v4 hook scaffold. `v4hook init` copied it without Git history or submodules.

## Build and test the example

Run:

```sh
forge build
forge test
```

`src/Counter.sol` shows the `beforeSwap`, `afterSwap`, `beforeAddLiquidity` and `beforeRemoveLiquidity` callbacks.

`test/Counter.t.sol` deploys local v4 contracts, creates a pool and checks the hook counters.

## Run static analysis

The deployment checks expect Slither. Use [uv](https://github.com/astral-sh/uv) to keep its Python
environment isolated from the system interpreter.

Install uv when needed, then install Slither as a user-level tool:

```sh
curl -LsSf https://astral.sh/uv/install.sh | sh
uv tool install slither-analyzer
```

Run the configured executable:

```sh
slither . --filter-paths 'vendor/' --fail-high
```

Or run a one-off analysis without installing Slither:

```sh
uvx --from slither-analyzer slither . --filter-paths 'vendor/' --fail-high
```

Do not replace Slither with `forge lint`; run both when configured. Fix actionable findings and
triage accepted false positives. Rerun the full project gates before describing the hook as locally
ready.

## Use a coding agent

The project includes `AGENTS.md` for Codex, ChatGPT and other coding agents. It routes agents to current Uniswap, Foundry and Ethereum guidance, and tells them how to install relevant project-level skills when supported.

Keep `AGENTS.md` in the repository root so agents can discover it. Review any third-party skill before using it, and do not treat agent output or a passing test suite as a security audit.

## Update the scaffold

Commit your work before an update. Install the newer v4hook CLI, then preview its bundled template:

```sh
v4hook scaffold update --dry-run
```

Apply the update:

```sh
v4hook scaffold update
```

The updater uses `.v4hook.toml` and `.v4hook-template-lock.json`. It updates unchanged scaffold files and preserves your changes.

If you changed the same file as the template, the CLI asks whether to preserve or replace your version.

## Test locally

Run `forge test`. The first-party `v4hook-testkit` deploys pinned Permit2, PoolManager and
PositionManager fixtures on Foundry's local chain and supplies Uniswap v4-core's `PoolSwapTest`.

For a remote deployment or fork, use the v4hook CLI lifecycle. It binds scripts to contract
addresses and code hashes from the deployment plan; scripts do not guess addresses or deploy a
custom router.

Do not put a private key in the command or repository.

## Configure a remote RPC

Copy the safe example before using v4hook against a testnet or mainnet:

```sh
cp .env.example .env
```

The example supplies public Robinhood Chain and Ethereum endpoints. They are convenience defaults,
not launch-grade infrastructure. Set the variable named by `network.rpcUrlEnv` in `.env`, verify its
chain ID and pinned-block access, and replace it with a dedicated archive-capable endpoint before
producing launch evidence. Keep authenticated provider URLs out of Git. The v4hook CLI reads this
project-local file without putting the endpoint in Anvil, Forge or Cast process arguments.

`DEPLOYER_ADDRESS` is public and may be stored in `.env`. Keep private keys, mnemonics and passwords out of `.env`; use `cast wallet import deployer --interactive` and pass `--account deployer` to v4hook.

## Prepare your hook

Replace the example contract and tests with your implementation. Update your deployment configuration with:

```sh
cp v4hook.config.example.json v4hook.config.json
```

- the compiled artifact path
- ABI-encoded constructor arguments
- every hook permission
- the target network contracts
- unit, fuzz and invariant test commands
- deployment, pool, quadrant and postcondition simulation steps

The v4hook deployment flow always runs a pinned Anvil fork before it broadcasts.

Treat `v4hook check` as a repair loop. Resolve every locally actionable source, script,
configuration, test and analyzer failure, then rerun the complete check. Testnet readiness also
requires finalized addresses and launch inputs, a clean plan, and passing pinned-fork simulation.
An external audit and explicit live authorization remain separate gates.

## Target Robinhood Chain

The supplied remote example targets Robinhood Chain mainnet, chain ID `4663`, because it has an
official Uniswap v4 deployment. The CLI requires `--mainnet` in addition to its exact confirmation
string before it can broadcast a hook or pool launch there. Use a dedicated, minimally funded
account and complete all checks and pinned-fork simulations before requesting live authorization.

Check the current contract addresses in the [Uniswap v4 deployment registry](https://developers.uniswap.org/docs/protocols/v4/deployments). Then follow the deployment and pool commands in the v4hook CLI README.

Hook permissions are encoded in the deployment address. Read the [Uniswap hook deployment guide](https://developers.uniswap.org/docs/protocols/v4/guides/hooks/hook-deployment) before changing permission flags.

## Upstream source

This project derives from the [Uniswap v4 hook template](https://github.com/Uniswap/v4-template). The scaffold lock records the exact upstream and dependency commits.

The local `v4hook-testkit` retains only pinned fixture deployment bytecode derived from
[Hookmate](https://github.com/akshatmittal/hookmate), with exact provenance recorded beside the
fixtures. It deliberately excludes Hookmate's network address table and custom router. Verify live
addresses against Uniswap's deployment registry; the CLI binds those addresses from the verified
plan into remote scripts.
