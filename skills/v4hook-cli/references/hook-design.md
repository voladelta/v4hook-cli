# Hook design

Use this reference to turn a product idea into an implementation specification. Confirm choices
against the project's pinned `vendor/uniswap-hooks/src` tree; available bases change by template
revision.

## Decide the behavior

Answer or infer these questions before coding:

1. What outcome must the hook produce?
2. Which pool lifecycle events must it intercept?
3. Does it observe swaps, modify fees, return deltas, hold liquidity, or move tokens?
4. Does it trust every router, allowlist routers, or authenticate a user through trusted hook data?
5. Which state is per pool, per user, global, persistent, or transaction-local?
6. Who may administer parameters, pause behavior, or recover assets?
7. Which chains and EVM features must it support?

For every asynchronous, timed, or permissionless state transition, also record:

- who can trigger it and what makes paying gas rational;
- the funds, state, and authority it consumes;
- the earliest and latest valid time, if any;
- its timeout, cancellation, or recovery path; and
- which state and balances must roll back when it fails.

A timestamp makes a transition eligible; it does not execute it. Prefer a permissionless caller with
a direct user benefit or bounded reward over an undocumented operator assumption.

## Select a base

Prefer the narrowest base present in the pinned tree:

| Goal | Candidate base or implementation |
| --- | --- |
| General callback logic | `BaseHook` |
| Delayed or async swaps | `BaseAsyncSwap` |
| Hook-owned liquidity or accounting | `BaseCustomAccounting` |
| Replacement pricing curve | `BaseCustomCurve` |
| Dynamic LP fee | `BaseDynamicFee` |
| Per-swap fee override | `BaseOverrideFee` |
| Fee collected after swaps | `BaseDynamicAfterFee` |
| Sandwich protection | `AntiSandwichHook` |
| JIT-liquidity protection | `LiquidityPenaltyHook` |
| Limit orders | `LimitOrderHook` |

Do not assume a candidate exists because an online generator advertises it. Fall back to
`BaseHook` only when implementing the missing behavior is justified and understood.

## Minimize permissions

Start with all 14 flags disabled. Enable a lifecycle callback only when implemented and exercised.
Enable a return-delta flag only with its corresponding callback and a written accounting
justification. Use permission names from the pinned `Hooks.sol` and v4hook configuration rather
than memorized bit positions.

Treat these as especially sensitive:

- `beforeRemoveLiquidity`: can trap liquidity.
- `beforeSwap`: can block or alter swaps.
- `beforeSwapReturnDelta`: can claim swap handling without fair output.
- Any other `*ReturnDelta`: can change user or LP settlement.

## Choose utilities and ownership

- Use the pinned currency-settlement helper when moving tokens to or from PoolManager.
- Use safe casts at every signed/unsigned or width boundary that can affect amounts or fees.
- Use transient storage only when the configured Solidity and EVM target support it.
- Issue shares only when the hook holds assets whose ownership must be represented.
- Prefer immutable configuration. Use `Ownable2Step`, role separation, or managed/timelocked
  authority only when privileged mutation is required.
- Treat proxies and upgradeable hooks as unsupported unless the project explicitly models proxy
  deployment, initialization, implementation verification, storage compatibility, and governance.
- Use the pinned standard implementation for an ordinary companion token. Specify only custom
  transfer observation, mint/burn authority, supply, decimal, fee, or accounting behavior instead
  of recreating inherited ERC-20 machinery.
- Make every fee denominator and rounding owner explicit. Multiply before dividing, carry a
  remainder when lifetime conservation requires it, and bound every signed/unsigned conversion.
- Apply checks-effects-interactions to claims and other external-value transfers. Bound public array
  sizes, reject duplicate identities where uniqueness matters, and define zero-value behavior.
- If an application needs historical activity, settle its event schema before implementation and
  route the indexing design through the EVM-integration reference.

## Produce the specification

Record the selected base, permissions, state, trusted actors, assets, constructor, administrative
surface, invariants, failure behavior, and test scenarios before implementation. Revisit the design
if required behavior cannot be tested through the v4hook simulation contract.
