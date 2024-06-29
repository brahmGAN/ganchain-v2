import { expect } from "chai";
import { step } from "mocha-steps";

import { createAndFinalizeBlock, describeWithFrontier } from "./util";

describeWithFrontier("Frontier RPC (State root hash)", (context) => {
	let block;
	step("should calculate a valid intermediate state root hash", async function () {
		await createAndFinalizeBlock(context.web3);
		block = await context.web3.eth.getBlock(1);
		expect(block.stateRoot.length).to.be.equal(66); // 0x prefixed
		expect(block.stateRoot).to.not.be.equal("0x0000000000000000000000000000000000000000000000000000000000000000");
	});

	step("hash should be unique between blocks", async function () {
		await createAndFinalizeBlock(context.web3);
		const anotherBlock = await context.web3.eth.getBlock(2);
		expect(block.stateRoot).to.not.be.equal(anotherBlock.stateRoot);
	});
});
