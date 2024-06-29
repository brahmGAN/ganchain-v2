pragma solidity ^0.5.0;

import '@openzeppelin/contracts/token/ERC20/ERC20.sol';

// This ERC-20 contract mints the maximum amount of tokens to the contract creator.
contract MyToken is ERC20 {
  constructor() public {
    _mint(msg.sender, 2**256 - 1);
  }
}
