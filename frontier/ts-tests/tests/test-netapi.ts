import { expect } from "chai";
import { step } from "mocha-steps";

import { CHAIN_ID } from "./config";
import { describeWithFrontier, customRequest } from "./util";

describeWithFrontier("Frontier RPC (Net)", (context) => {
	step("should return `net_version`", async function () {
		expect(await context.web3.eth.net.getId()).to.equal(CHAIN_ID);
	});
	step("should return `peer_count` in hex directly using the provider", async function () {
		expect((await customRequest(context.web3, "net_peerCount", [])).result).to.be.eq("0x0");
	});
	step("should format `peer_count` as decimal using `web3.net`", async function () {
		expect(await context.web3.eth.net.getPeerCount()).to.equal(0);
	});
});
