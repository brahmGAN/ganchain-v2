import { expect } from "chai";

import { GENESIS_ACCOUNT, GENESIS_ACCOUNT_PRIVATE_KEY } from "./config";
import { createAndFinalizeBlock, customRequest, describeWithFrontier } from "./util";

describeWithFrontier("Frontier RPC (Constructor Revert)", (context) => {
	// ```
	// pragma solidity >=0.4.22 <0.7.0;
	//
	// contract WillFail {
	//		 constructor() public {
	//				 require(false);
	//		 }
	// }
	// ```
	const FAIL_BYTECODE =
		"6080604052348015600f57600080fd5b506000601a57600080fd5b603f8060276000396000f3fe6080604052600080fdfea26469706673582212209f2bb2a4cf155a0e7b26bd34bb01e9b645a92c82e55c5dbdb4b37f8c326edbee64736f6c63430006060033";
	const GOOD_BYTECODE =
		"6080604052348015600f57600080fd5b506001601a57600080fd5b603f8060276000396000f3fe6080604052600080fdfea2646970667358221220c70bc8b03cdfdf57b5f6c4131b836f9c2c4df01b8202f530555333f2a00e4b8364736f6c63430006060033";

	it("should provide a tx receipt after successful deployment", async function () {
		this.timeout(15000);

		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: GOOD_BYTECODE,
				value: "0x00",
				gasPrice: "0x3B9ACA00",
				gas: "0x100000",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);

		const txHash = (await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction])).result;

		// Verify the receipt exists after the block is created
		await createAndFinalizeBlock(context.web3);
		const receipt = await context.web3.eth.getTransactionReceipt(txHash);
		expect(receipt).to.include({
			from: GENESIS_ACCOUNT,
			to: null,
			transactionHash: txHash,
			transactionIndex: 0,
			status: true,
			type: "0x0",
		});
	});

	it("should provide a tx receipt after failed deployment", async function () {
		this.timeout(15000);

		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: FAIL_BYTECODE,
				value: "0x00",
				gasPrice: "0x3B9ACA00",
				gas: "0x100000",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);

		const txHash = (await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction])).result;

		// Verify the receipt exists after the block is created
		await createAndFinalizeBlock(context.web3);
		const receipt = await context.web3.eth.getTransactionReceipt(txHash);
		expect(receipt).to.include({
			from: GENESIS_ACCOUNT,
			to: null,
			transactionHash: txHash,
			transactionIndex: 0,
			status: false,
		});
	});
});
