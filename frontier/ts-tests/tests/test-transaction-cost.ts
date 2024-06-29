import { expect } from "chai";
import { step } from "mocha-steps";

import { describeWithFrontier, customRequest } from "./util";

describeWithFrontier("Frontier RPC (Transaction cost)", (context) => {
	step("should take transaction cost into account and not submit it to the pool", async function () {
		// Simple transfer with gas limit 0 manually signed to prevent web3 from rejecting client-side.
		const tx = await customRequest(context.web3, "eth_sendRawTransaction", [
			"0xf86180843b9aca00809412cb274aad8251c875c0bf6872b67d9983e53fdd01801ca00e28ba2dd3c5a3fd467\
			d4afd7aefb4a34b373314fff470bb9db743a84d674a0aa06e5994f2d07eafe1c37b4ce5471caecec29011f6f5b\
			f0b1a552c55ea348df35f",
		]);
		let msg = "intrinsic gas too low";
		expect(tx.error).to.include({
			message: msg,
		});
	});
});
