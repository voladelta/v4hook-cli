# Dynamic fee hooks

Use this reference when a hook changes an LP fee.

## Choose the fee mechanism

- Use a per-swap override when the fee is derived for each swap. A narrow `BaseHook` implementation
  may need only `beforeSwap`; return the fee with `LPFeeLibrary.OVERRIDE_FEE_FLAG` and zero delta.
- Use the pinned dynamic-fee base only when its additional lifecycle callbacks and cached fee update
  behavior are required. Inspect its actual permissions before selecting it.
- Reject a static-fee `PoolKey` when the selected mechanism requires a dynamic-fee pool.

Do not enable `afterInitialize` merely because an available base enables it. Choose the narrowest
pinned base that matches the required lifecycle.

## Keep pool configuration aligned

- Use `LPFeeLibrary.DYNAMIC_FEE_FLAG` in Solidity pool scripts and the equivalent numeric value in
  v4hook configuration.
- Keep the contract's override units, constructor bounds, pool fee mode, deployment arguments, and
  tests consistent. Uniswap LP fees use hundredths of a basis point and must not exceed the pinned
  library's maximum.
- Test static-pool rejection, fee flagging, lower and upper bounds, rounding, and every time or state
  boundary that changes the fee.
- Exercise all four swap direction and exact-input/exact-output quadrants through PoolManager.

For time-based fees, validate schedule overflow only when the implementation materializes an end
timestamp. Use full-precision multiplication and division for interpolation.
