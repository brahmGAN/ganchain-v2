import { expect } from "chai";

import { CHAIN_ID } from "./config";
import { describeWithFrontier } from "./util";

// All test for the RPC

describeWithFrontier("Frontier RPC (Constant)", (context) => {
	it("should have 0 hashrate", async function () {
		expect(await context.web3.eth.getHashrate()).to.equal(0);
	});

	it("should have chainId", async function () {
		// The chainId is defined by the Substrate Chain Id, default to 42
		expect(await context.web3.eth.getChainId()).to.equal(CHAIN_ID);
	});

	it("should have no account", async function () {
		expect(await context.web3.eth.getAccounts()).to.eql([]);
	});

	it("block author should be 0x0000000000000000000000000000000000000000", async function () {
		// This address `0x1234567890` is hardcoded into the runtime find_author
		// as we are running manual sealing consensus.
		expect(await context.web3.eth.getCoinbase()).to.equal("0x0000000000000000000000000000000000000000");
	});
});
