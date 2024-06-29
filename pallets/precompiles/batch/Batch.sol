// SPDX-License-Identifier: GPL-3.0-only
pragma solidity >=0.8.3;

/// Interface to the precompiled contract on Shibuya/Shiden/Astar
/// Predeployed at the address 0x0000000000000000000000000000000000005006
/// For better understanding check the source code:
/// repo: https://github.com/AstarNetwork/astar

/// @title Batch precompile
/// @dev Allows to perform multiple calls through one call to the precompile.
/// Can be used by EOA to do multiple calls in a single transaction.
interface Batch {
    /// @dev Batch multiple calls into a single transaction.
    /// All calls are performed from the address calling this precompile.
    ///
    /// In case of one subcall reverting following subcalls will still be attempted.
    ///
    /// @param to List of addresses to call.
    /// @param value List of values for each subcall. If array is shorter than `to` then additional
    /// calls will be performed with a value of 0.
    /// @param callData Call data for each `to` address. If array is shorter than `to` then
    /// additional calls will be performed with an empty call data.
    /// @param gasLimit Gas limit for each `to` address. Use 0 to forward all the remaining gas.
    /// If array is shorter than `to` then the remaining gas available will be used.
    function batchSome(
        address[] memory to,
        uint256[] memory value,
        bytes[] memory callData,
        uint64[] memory gasLimit
    ) external;

    /// @dev Batch multiple calls into a single transaction.
    /// All calls are performed from the address calling this precompile.
    ///
    /// In case of one subcall reverting, no more subcalls will be executed but
    /// the batch transaction will succeed. Use "batchAll" to revert on any subcall revert.
    ///
    /// @param to List of addresses to call.
    /// @param value List of values for each subcall. If array is shorter than `to` then additional
    /// calls will be performed with a value of 0.
    /// @param callData Call data for each `to` address. If array is shorter than `to` then
    /// additional calls will be performed with an empty call data.
    /// @param gasLimit Gas limit for each `to` address. Use 0 to forward all the remaining gas.
    /// If array is shorter than `to` then the remaining gas available will be used.
    function batchSomeUntilFailure(
        address[] memory to,
        uint256[] memory value,
        bytes[] memory callData,
        uint64[] memory gasLimit
    ) external;

    /// @dev Batch multiple calls into a single transaction.
    /// All calls are performed from the address calling this precompile.
    ///
    /// In case of one subcall reverting, the entire batch will revert.
    ///
    /// @param to List of addresses to call.
    /// @param value List of values for each subcall. If array is shorter than `to` then additional
    /// calls will be performed with a value of 0.
    /// @param callData Call data for each `to` address. If array is shorter than `to` then
    /// additional calls will be performed with an empty call data.
    /// @param gasLimit Gas limit for each `to` address. Use 0 to forward all the remaining gas.
    /// If array is shorter than `to` then the remaining gas available will be used.
    function batchAll(
        address[] memory to,
        uint256[] memory value,
        bytes[] memory callData,
        uint64[] memory gasLimit
    ) external;

    /// Emitted when a subcall succeeds.
    event SubcallSucceeded(uint256 index);

    /// Emitted when a subcall fails.
    event SubcallFailed(uint256 index);

}
