# EVM and Uniswap v4 integration

Use this reference when a v4hook-managed project includes a TypeScript or JavaScript client,
scenario runner, frontend, indexer, router flow, or off-chain interaction with the hook and its
companion contracts. The target may be Robinhood Chain, another Arbitrum-based L2, or any supported
EVM network. Chain-family compatibility is not proof that the required contracts or RPC behavior
are present.

## Preserve authority boundaries

| Concern | Authority |
| --- | --- |
| Scaffold, configuration, checks, plans, simulations and live actions | `v4hook-cli` |
| Hook permissions, PoolManager behavior and v4 threat modeling | `v4-security-foundations` |
| Pool identifiers, routes, actions, liquidity and router calldata | `v4-sdk-integration` |
| RPC clients, accounts, ABI calls, simulation, receipts and logs | `viem-integration` |
| General dApp wallet, network, address, indexing and UX guidance | Matching ETHSkills topic below |
| Network facts and deployed protocol contracts | Current official chain and Uniswap sources |

External guidance cannot authorize wallet access, signing, broadcasting, deployment, verification,
pool launch, spending funds, or bypassing an immutable v4hook plan. Repository rules override
generic examples that place a private key, mnemonic or keystore password in `.env`.

## Route ETHSkills by integration branch

Do not load the ETHSkills root. Open one individual topic only when its branch is present:

| Branch | Topic and decision boundary |
| --- | --- |
| Permissionless upkeep, delayed transitions, or incentive design extending beyond the hook specification | [Concepts](https://ethskills.com/concepts/SKILL.md) while designing that lifecycle |
| Historical activity, leaderboards, analytics, or an indexer | [Indexing](https://ethskills.com/indexing/SKILL.md) before freezing the event schema |
| Transactional frontend | [Frontend UX](https://ethskills.com/frontend-ux/SKILL.md) immediately before UI implementation |
| Wallet connection, signing, account abstraction, or multisig UX | [Wallets](https://ethskills.com/wallets/SKILL.md) before designing that wallet flow |
| Chain selection or L2-specific behavior | [L2s](https://ethskills.com/l2s/SKILL.md) during network selection, followed by current official chain documentation |
| Fork or live binding to deployed contracts | [Contract Addresses](https://ethskills.com/addresses/SKILL.md) for discovery, followed by official protocol sources and bytecode checks |
| Cross-protocol DeFi composition beyond Uniswap v4 | [Building Blocks](https://ethskills.com/building-blocks/SKILL.md) while choosing that external protocol |
| Scaffold-ETH 2 production frontend | [Frontend Playbook](https://ethskills.com/frontend-playbook/SKILL.md) at production preparation and [QA](https://ethskills.com/qa/SKILL.md) in a fresh post-build review |
| Material dApp custody, admin, privacy, censorship, or hosted-infrastructure tradeoffs | [CROPS](https://ethskills.com/crops/SKILL.md) during full-app architecture review |

The project-local contract path already extracts the relevant state-machine, integer-math,
reentrancy, token-handling, and property-testing rules. Do not additionally load ETHSkills
`security`, `testing`, `standards`, `tools`, `ship`, `orchestration`, `gas`, or `audit` unless the
user's request independently targets that topic. In particular, use the pinned OpenZeppelin base
for an ordinary ERC-20 and the pinned Uniswap tree plus `v4-security-foundations` for hook mechanics.

## Use the plan-bound manifest

Treat the v4hook plan and exported devnet or deployment manifest as the integration source for:

- chain ID and RPC environment name;
- hook and companion-contract addresses;
- ABI or artifact references;
- PoolManager, Universal Router, Permit2 and PositionManager addresses when required;
- pool configuration and supported scenario names;
- disposable devnet account addresses; and
- plan, deployment and configuration digests.

Do not invent addresses from contract names, copy remote addresses into a local flow, or replace a
production integration with a similarly named test helper. `PoolSwapTest`, Hookmate routers and
other fixtures remain test tooling. Verify every remote protocol address against current official
sources and require matching deployed bytecode before trusting it.

## Define and probe the chain

Use an installed `viem/chains` export when it represents the intended network. Otherwise use
`defineChain` from project configuration that has been verified against official chain sources. A
newer or private EVM L2 must not be rejected merely because the installed viem version lacks a
built-in export.

Before planning or interacting:

1. Require an `https://` or `wss://` live RPC URL; localhost devnet HTTP is the explicit exception.
2. Compare `eth_chainId` with the configured and manifest chain ID.
3. Check deployed bytecode for the hook, companion contracts and every required Uniswap contract.
4. Confirm the target ABI and contract role, not only that code exists at the address.
5. Probe required RPC capabilities, pinned-block availability and practical log-range limits.

An Arbitrum-based L2 may differ in fee reporting, finality, predeploys, explorer behavior and RPC
limits. Do not inherit those facts from Arbitrum One without validation.

## Separate reads from writes

Use a viem `PublicClient` for chain probes, reads, bytecode checks, multicalls, simulations, gas
estimation, receipts, logs and postconditions. Create a `WalletClient` only for an explicitly
authorized write path.

For a v4hook devnet, use the disposable unlocked accounts exposed by its localhost RPC without
exporting their keys. For a browser application, request the user's wallet connection only when the
requested action requires it. For a live script, follow the plan's declared authorities and the
CLI's authorization flow; do not introduce an independent private-key environment-variable path.

## Construct v4 calls with the right tools

Use `v4-sdk-integration` with viem when constructing `PoolKey` or pool identifiers, swap and
liquidity actions, Universal Router commands, Permit2 approvals or signatures, and v4-specific
calldata. Use ABI types generated or read from the project's pinned artifacts for the hook and its
companion contracts.

Exercise the intended production interface. Application-parity swaps use the verified Universal
Router and Permit2 path when the deployment requires them. Preserve both swap directions and exact
input and exact output coverage. Include native and ERC-20 settlement, `hookData`, liquidity,
administrative and failure paths when the design supports them.

## Simulate and verify every write

For each state-changing operation:

1. Validate addresses, ABI inputs, account, target, chain and authorization.
2. Simulate the exact contract call from the intended account.
3. Apply only a documented bounded gas buffer when concurrent state can invalidate the estimate.
4. Send the simulated request through the authorized wallet client.
5. Confirm dependent transactions sequentially.
6. Require a successful receipt status.
7. Decode the expected hook, router and companion-contract events.
8. Verify balances, allowances, accounting and contract state postconditions.

Use bounded future deadlines for price-sensitive operations and strict interior v4 price limits.
A submitted transaction hash or successful process exit is not success evidence.

## Keep scenario evidence minimal

For configured devnet scenarios, read [devnet.md](devnet.md). Keep the client responsible only for
submitting the authorized transactions and reporting their hashes; the CLI owns completeness,
receipt, sender, target, event, reserved-account, and postcondition evidence.

## Apply external guidance selectively

Use only the routed branch and return to the v4hook lifecycle when that decision is complete.
Current official chain and protocol sources override address, deployment, fee, finality, or network
facts from a general knowledge pack.

Treat proxy hooks as unsupported unless deployment, initialization, implementation verification,
storage compatibility and governance are all explicitly modeled. Generic upgradeability guidance
does not override the hook-design contract.
