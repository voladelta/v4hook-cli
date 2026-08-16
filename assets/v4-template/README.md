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

## Test with Anvil

Start a local node:

```sh
anvil
```

Deploy the example hook in another terminal:

```sh
forge script script/00_DeployHook.s.sol:DeployHookScript \
  --rpc-url http://127.0.0.1:8545 \
  --account deployer \
  --sender 0xYourAddress \
  --broadcast
```

Import the account into the Foundry keystore before you run the script:

```sh
cast wallet import deployer --interactive
```

Do not put a private key in the command or repository.

## Configure a remote RPC

Copy the safe example before using v4hook against a testnet or mainnet:

```sh
cp .env.example .env
```

Set the RPC variable named by `network.rpcUrlEnv` in `.env`. Authenticated Alchemy or other paid endpoints are supported. The v4hook CLI reads this project-local file without putting the endpoint in Anvil, Forge or Cast process arguments.

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

## Deploy to a testnet

Use a dedicated account with testnet funds. Base Sepolia is a suitable first network.

Check the current contract addresses in the [Uniswap v4 deployment registry](https://developers.uniswap.org/docs/protocols/v4/deployments). Then follow the deployment and pool commands in the v4hook CLI README.

Hook permissions are encoded in the deployment address. Read the [Uniswap hook deployment guide](https://developers.uniswap.org/docs/protocols/v4/guides/hooks/hook-deployment) before changing permission flags.

## Upstream source

This project derives from the [Uniswap v4 hook template](https://github.com/Uniswap/v4-template). The scaffold lock records the exact upstream and dependency commits.
