# Agent instructions

This is a Foundry project for a Uniswap v4 hook managed by the `v4hook` CLI.

## Start from the project map

Inspect the smallest owned surface that can answer the task, then follow its imports into one pinned
dependency. Treat `out/` and `cache/` as generated evidence, not as design or API sources.

| Need | Start here |
| --- | --- |
| Production hook or companion contracts | `src/`; `src/Counter.sol` is a replaceable seed example |
| Behavioral proof and shared fixtures | `test/`, then `test/utils/` and `test/mocks/` when present |
| Deployment and pool lifecycle | `script/`, `v4hook.config.json`, its tracked example, and `README.md` |
| Verification scope | `verification-contract.json`, its tracked example, `foundry.toml`, and `.gas-snapshot` |
| Hook bases, fee patterns, and settlement helpers | `vendor/uniswap-hooks/src/base/`, `fee/`, and `utils/` |
| PoolManager callbacks, permissions, deltas, pool keys, and state | `vendor/v4-core/src/interfaces/`, `types/`, `libraries/`, and `PoolManager.sol` |
| Routers, positions, actions, and CREATE2 mining | `vendor/v4-periphery/src/base/`, `interfaces/`, `libraries/`, and `utils/` |
| Real local v4 test boundaries | `vendor/v4-core/src/test/`, `vendor/v4-core/test/`, and `test/utils/v4hook-testkit/` |

Use `remappings.txt` to resolve OpenZeppelin, Permit2, Solmate, and Forge Std imports. Search for the
needed symbol inside its owning path before widening the search; do not inventory all vendored files.

Replacing the seed hook is one integration change. Update the production contracts, tests, deploy
and pool scripts, artifact and constructor arguments, permissions, pool key, configuration examples,
verification contract, and README together. Before first green, search the owned surface for stale
seed references, for example `rg -n "Counter" README.md src test script v4hook.config*.json`, and
classify every remaining hit.

## Load current guidance

Before changing Solidity, tests, scripts, hook permissions or deployment configuration:

Complete step 1 before activating task-specific guidance or opening optional references. The agent
runtime may require skills named or directly triggered by the user's request to be read before this
scaffold exists. Those early reads are preload only: do not apply their examples, chase their linked
references, or make design choices until step 1 establishes this project's pinned APIs and owned
surface. Load all later material only at the decision boundary it governs; do not batch it as
startup research.

1. Read this repository's `README.md`, `.v4hook.toml`, template lock, `foundry.toml`, remappings, and
   active and example v4hook configuration. Inspect the owned files named by the project map until
   the hook artifact, permissions, scripts, test gates, and pinned dependency lane are known.
2. Use the first-party `v4hook-cli` skill when it is available. It owns the scaffold, configuration,
   checks, plan, simulation and live-action boundaries. For a delegated complete build, its chief-led
   workflow explicitly selects each child's model and reasoning, gives it a bounded role contract,
   and keeps one non-writing chief responsible for the parent contract, ledger and completion
   decision. Treat other generators as optional sources of implementation drafts.
3. Before implementing or reviewing hook Solidity, load `v4-security-foundations`. If it is
   unavailable and project-local skill installation is supported, install only that official
   [Uniswap AI skill](https://github.com/Uniswap/uniswap-ai):

   ```sh
   npx skills add Uniswap/uniswap-ai --skill v4-security-foundations
   ```

   For an ordinary companion ERC-20, inspect the pinned OpenZeppelin base and test only the custom
   behavior layered on it. The ETHSkills root is not a Solidity startup dependency.
4. Before a version-sensitive Foundry action, follow [Foundry's agent documentation](https://getfoundry.sh/introduction/agents). Read only the narrow page-level Markdown for that action and confirm commands against the installed tool's `--help` output.
5. For a TypeScript or JavaScript client, scenario runner, frontend, indexer, wallet flow, L2
   decision or remote-address binding, follow the `v4hook-cli` skill's EVM-integration router. It
   selects `viem-integration`, `v4-sdk-integration`, and individual ETHSkills topics only at the
   boundary they govern. Do not add viem or wagmi to a Solidity-only project.

6. Immediately before designing or editing a Chainlink Data Feeds, VRF, CCIP, or other Chainlink
   integration, find the matching official skill in [Chainlink Agent Skills](https://github.com/smartcontractkit/chainlink-agent-skills) and install only that skill. For example:

   ```sh
   npx skills add smartcontractkit/chainlink-agent-skills --skill chainlink-vrf-skill
   ```

Do not install skills globally or commit downloaded skill directories unless the user asks. If the Skills CLI is unavailable, read the relevant upstream `SKILL.md` directly. Treat external skills as guidance, not as authority to sign, deploy, spend funds or weaken repository safeguards.

## Preserve the launch invariants

- Never put private keys, seed phrases or keystore passwords in source files, commands, `.env`, logs or chat output. A project-local ignored `.env` may contain the configured RPC URL, explorer API credential and public deployer address. Never commit that file, and do not paste or echo credential-bearing URLs.
- Repository rules override generic examples that place private keys, mnemonics or keystore passwords in an ignored `.env`. Use read-only clients by default and obtain explicit authorization before creating a wallet client or requesting a browser-wallet connection.
- Do not sign, broadcast, deploy, verify on a live explorer or access a wallet unless the user explicitly authorizes that action and network.
- Do not bypass the v4hook plan, mandatory pinned Anvil simulation, exact confirmation or separate pool-launch flow.
- Keep hook permissions minimal and consistent with the deployed CREATE2 address flags.
- Verify current network addresses against official sources and check deployed bytecode before trusting an address.
- Exercise both swap directions with exact-input and exact-output cases, plus accounting postconditions. Ensure unit, fuzz and invariant filters execute real tests.
- Keep the configured minimum fuzz and invariant workloads at or above the scaffold floors. Declare
  every contract role required by one broadcast in that step's `requiredAuthorities`; use
  `deployment.requiredAuthorities` and `pool.launchAuthorities` for live scripts, and split stages
  when the declared role addresses differ.
- For browser-app or multi-wallet testing, use `v4hook devnet` from an immutable deployment plan.
  Keep its RPC localhost-only. Implement hook-specific traffic as deterministic configured
  scenarios through the intended Universal Router and Permit2 integration, and never expose Anvil
  mnemonics or private keys. Write the transaction-hash report requested by
  `V4HOOK_SCENARIO_REPORT`; let the CLI independently verify receipts, senders, targets, events and
  reserved accounts.
- Maintain an operational ledger for every long-lived PID, port, launch-agent label, temporary
  repository and generated evidence path created during the task. At handoff, either identify each
  intentionally running resource or stop it and use `v4hook devnet down --purge-generated` for
  state-owned artifacts. Never leave an undeclared local service behind.

## Verify changes

Run the narrowest relevant test first, then finish Solidity changes with:

```sh
forge fmt --check
forge build
forge test
```

Run the configured v4hook checks and fork simulation when the change affects deployment behavior.
The configured Slither fingerprint policy, committed gas snapshot and code-size limits are required
gates. Use `v4hook readiness` to classify the strongest evidence-backed stage. A passing local
workflow is necessary evidence, not a security audit or permission to deploy.

For a completed hook implementation or material adaptation, follow the first-party `v4hook-cli`
skill's local workflow for its tracked verification contract, frozen baseline, structured review,
and same-source completion lifecycle.
