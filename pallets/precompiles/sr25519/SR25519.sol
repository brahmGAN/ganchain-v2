pragma solidity ^0.8.0;

/**
 * @title SR25519 signature interface.
 */
interface SR25519 {
    /**
     * @dev Verify signed message using SR25519 crypto.
     * @return A boolean confirming whether the public key is signer for the message. 
     */
    function verify(
        bytes32 public_key,
        bytes calldata signature,
        bytes calldata message
    ) external view returns (bool);
}