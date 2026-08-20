# Agent instructions

This is a Foundry project for a Uniswap v4 hook managed by the `v4hook` CLI.

## Load current guidance

Before changing Solidity, tests, scripts, hook permissions or deployment configuration:

1. Read this repository's `README.md`, `foundry.toml` and v4hook configuration before deciding how the project works.
2. Use the first-party `v4hook-cli` skill when it is available. It owns the scaffold, configuration, checks, plan, simulation and live-action boundaries. Treat other generators as optional sources of implementation drafts.
3. Read [ETHSkills](https://ethskills.com/SKILL.md), then load only the topics relevant to the task. Hook or companion-contract work normally requires security and testing guidance. Robinhood Chain or other EVM L2 integration should also load the relevant L2 guidance; deployment preparation requires wallet and contract-address guidance; an off-chain application may require indexing or frontend guidance. Do not automatically load broad shipping, orchestration, audit-submission or feedback workflows.
4. Follow [Foundry's agent documentation](https://getfoundry.sh/introduction/agents). Read the narrow page-level Markdown relevant to the task and confirm version-sensitive commands against the installed tool's `--help` output.
5. Use the official [Uniswap AI skills](https://github.com/Uniswap/uniswap-ai) for v4 hook work. If the required skill is unavailable and project-local skill installation is supported, install only what the task needs:

   ```sh
   npx skills add Uniswap/uniswap-ai --skill v4-security-foundations
   npx skills add Uniswap/uniswap-ai --skill v4-hook-generator
   npx skills add Uniswap/uniswap-ai --skill viem-integration
   npx skills add Uniswap/uniswap-ai --skill v4-sdk-integration
   ```

   The security skill applies to every hook implementation or review. Use the generator only as a fallback design reference when the first-party skill is unavailable, and adapt its output to this project's pinned scaffold. Load `viem-integration` only for a TypeScript or JavaScript client, scenario runner, frontend, indexer, wallet connection or off-chain contract interaction. Also load `v4-sdk-integration` when constructing v4 pool identifiers, routes, actions, liquidity operations, Permit2 data or Universal Router calldata. Do not add viem or wagmi to a Solidity-only project.

6. If the task introduces Chainlink Data Feeds, VRF, CCIP or another Chainlink product, find the matching official skill in [Chainlink Agent Skills](https://github.com/smartcontractkit/chainlink-agent-skills) and install only that skill. For example:

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

For a completed hook implementation or material adaptation, replace the example verification
contract with a tracked contract that maps every protected invariant to exact configured test names
or one explicit external evidence gap. Commit that contract, the configuration, and the pre-edit
ledger before production edits, then use `v4hook verification freeze`, `check`, `review`, and the
same-source second `check`. A source change after review—including a tracked evidence-report
update—starts a new first-green cycle.
