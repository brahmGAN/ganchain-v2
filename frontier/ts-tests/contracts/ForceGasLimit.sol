// SPDX-License-Identifier: MIT

pragma solidity ^0.8.0;

contract ForceGasLimit  {
    uint public number;
    function force_gas(uint require_gas) public returns (uint) {
        require(gasleft() > require_gas, "not enough gas");
        number++;
        return number;
    }
}