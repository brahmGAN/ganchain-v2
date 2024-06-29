pragma solidity >=0.8.0;

contract InvalidOpcode {
    uint public number;

    function call() public  {
        number = 1;
        if (gasleft() < 40000 ){
            assembly {
                // Calls the INVALID opcode (0xFE)
                invalid()
            }
        }
    }
}
