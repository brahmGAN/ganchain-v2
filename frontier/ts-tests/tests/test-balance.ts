import { expect } from "chai";
import { step } from "mocha-steps";

import { GENESIS_ACCOUNT, GENESIS_ACCOUNT_PRIVATE_KEY, GENESIS_ACCOUNT_BALANCE, EXISTENTIAL_DEPOSIT } from "./config";
import { createAndFinalizeBlock, describeWithFrontier, customRequest } from "./util";

describeWithFrontier("Frontier RPC (Balance)", (context) => {
	const TEST_ACCOUNT = "0x1111111111111111111111111111111111111111";

	step("genesis balance is setup correctly", async function () {
		expect(await context.web3.eth.getBalance(GENESIS_ACCOUNT)).to.equal(GENESIS_ACCOUNT_BALANCE);
	});

	step("balance to be updated after transfer", async function () {
		await createAndFinalizeBlock(context.web3);
		this.timeout(15000);

		const value = "0x200"; // 512, must be higher than ExistentialDeposit
		const gasPrice = "0x3B9ACA00"; // 1000000000
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				to: TEST_ACCOUNT,
				value: value,
				gasPrice: gasPrice,
				gas: "0x100000",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);
		await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);

		// GENESIS_ACCOUNT_BALANCE - (21000 * gasPrice) - value;
		const expectedGenesisBalance = (
			BigInt(GENESIS_ACCOUNT_BALANCE) -
			BigInt(21000) * BigInt(gasPrice) -
			BigInt(value)
		).toString();
		const expectedTestBalance = (Number(value) - EXISTENTIAL_DEPOSIT).toString();
		expect(await context.web3.eth.getBalance(GENESIS_ACCOUNT, "pending")).to.equal(expectedGenesisBalance);
		expect(await context.web3.eth.getBalance(TEST_ACCOUNT, "pending")).to.equal(expectedTestBalance);

		await createAndFinalizeBlock(context.web3);

		expect(await context.web3.eth.getBalance(GENESIS_ACCOUNT)).to.equal(expectedGenesisBalance);
		expect(await context.web3.eth.getBalance(TEST_ACCOUNT)).to.equal(expectedTestBalance);
	});
});
