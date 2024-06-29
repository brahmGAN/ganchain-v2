import { expect } from "chai";
import { step } from "mocha-steps";

import { GENESIS_ACCOUNT, GENESIS_ACCOUNT_PRIVATE_KEY, GENESIS_ACCOUNT_BALANCE, EXISTENTIAL_DEPOSIT } from "./config";
import { createAndFinalizeBlock, describeWithFrontier, customRequest } from "./util";

describeWithFrontier("Frontier RPC (BlockNumber tags)", (context) => {
	before("Send some transactions across blocks", async function () {
		// block #1 finalized
		await createAndFinalizeBlock(context.web3);
		// block #2 not finalized
		await createAndFinalizeBlock(context.web3, false);
	});

	step("`earliest` returns genesis", async function () {
		expect((await context.web3.eth.getBlock("earliest")).number).to.equal(0);
	});

	step("`latest` returns `BlockchainInfo::best_hash` number", async function () {
		expect((await context.web3.eth.getBlock("latest")).number).to.equal(2);
	});

	step("`finalized` uses `BlockchainInfo::finalized_hash`  number", async function () {
		expect((await context.web3.eth.getBlock("finalized")).number).to.equal(1);
	});

	step("`safe` is an alias for `finalized` in Polkadot", async function () {
		expect((await context.web3.eth.getBlock("safe")).number).to.equal(1);
	});
});
