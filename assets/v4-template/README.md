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

## Prepare your hook

Replace the example contract and tests with your implementation. Update your deployment configuration with:

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
