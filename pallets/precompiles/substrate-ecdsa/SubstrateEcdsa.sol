pragma solidity ^0.8.0;

/**
 * @title SubstrateEcdsa signature interface.
 */
interface ISubstrateEcdsa {
    /**
     * @dev Verify signed message using Substrate version of ECDSA crypto.
     * @return A boolean confirming whether the public key is signer for the message. 
     */
    function verify(
        bytes32 public_key,
        bytes calldata signature,
        bytes calldata message
    ) external view returns (bool);
}
