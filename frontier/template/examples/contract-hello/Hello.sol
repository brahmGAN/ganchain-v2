pragma solidity =0.5.15;

contract Hello {
	event Said(string message);
	address public owner;

	constructor() public {
		owner = msg.sender;
		emit Said("Hello, world!");
	}
}
