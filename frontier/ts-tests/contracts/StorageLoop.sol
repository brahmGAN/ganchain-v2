// SPDX-License-Identifier: GPL-3.0
pragma solidity >=0.8.2 <0.9.0;

contract StorageLoop {
    mapping(address => uint) public map;
    
    // n=1      30k
    // n=10     37k
    // n=100    100k
    // n=250    205k
    // n=500    380k
    // n=1000   745k
    function storageLoop(
        uint16 n,
        address _to,
        uint _amount
    ) public {
        for (uint16 i = 0; i < n; i++) {
            map[_to] += _amount;
        }
    }
}