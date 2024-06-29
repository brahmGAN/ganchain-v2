import { expect } from "chai";
import { AbiItem } from "web3-utils";

import Test from "../build/contracts/Test.json";
import {
	GENESIS_ACCOUNT,
	GENESIS_ACCOUNT_PRIVATE_KEY,
	FIRST_CONTRACT_ADDRESS,
	BLOCK_HASH_COUNT,
	ETH_BLOCK_GAS_LIMIT,
} from "./config";
import { createAndFinalizeBlock, createAndFinalizeBlockNowait, customRequest, describeWithFrontier } from "./util";

describeWithFrontier("Frontier RPC (Contract Methods)", (context) => {
	const TEST_CONTRACT_BYTECODE = Test.bytecode;
	const TEST_CONTRACT_ABI = Test.abi as AbiItem[];

	// Those test are ordered. In general this should be avoided, but due to the time it takes
	// to spin up a frontier node, it saves a lot of time.

	before("create the contract", async function () {
		this.timeout(15000);
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: TEST_CONTRACT_BYTECODE,
				value: "0x00",
				gasPrice: "0x3B9ACA00",
				gas: "0x100000",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);
		await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		await createAndFinalizeBlock(context.web3);
	});

	it("get transaction by hash", async () => {
		const latestBlock = await context.web3.eth.getBlock("latest");
		expect(latestBlock.transactions.length).to.equal(1);

		const txHash = latestBlock.transactions[0];
		const tx = await context.web3.eth.getTransaction(txHash);
		expect(tx.hash).to.equal(txHash);
	});

	it("should return contract method result", async function () {
		const contract = new context.web3.eth.Contract(TEST_CONTRACT_ABI, FIRST_CONTRACT_ADDRESS, {
			from: GENESIS_ACCOUNT,
			gasPrice: "0x3B9ACA00",
		});

		expect(await contract.methods.multiply(3).call()).to.equal("21");
	});
	it("should get correct environmental block number", async function () {
		// Solidity `block.number` is expected to return the same height at which the runtime call was made.
		const contract = new context.web3.eth.Contract(TEST_CONTRACT_ABI, FIRST_CONTRACT_ADDRESS, {
			from: GENESIS_ACCOUNT,
			gasPrice: "0x3B9ACA00",
		});
		let block = await context.web3.eth.getBlock("latest");
		expect(await contract.methods.currentBlock().call()).to.eq(block.number.toString());
		await createAndFinalizeBlock(context.web3);
		block = await context.web3.eth.getBlock("latest");
		expect(await contract.methods.currentBlock().call()).to.eq(block.number.toString());
	});

	it("should get correct environmental block hash", async function () {
		this.timeout(20000);
		// Solidity `blockhash` is expected to return the ethereum block hash at a given height.
		const contract = new context.web3.eth.Contract(TEST_CONTRACT_ABI, FIRST_CONTRACT_ADDRESS, {
			from: GENESIS_ACCOUNT,
			gasPrice: "0x3B9ACA00",
		});
		let number = (await context.web3.eth.getBlock("latest")).number;
		let last = number + BLOCK_HASH_COUNT;
		for (let i = number; i <= last; i++) {
			let hash = (await context.web3.eth.getBlock("latest")).hash;
			expect(await contract.methods.blockHash(i).call()).to.eq(hash);
			await createAndFinalizeBlockNowait(context.web3);
		}
		// should not store more than `BLOCK_HASH_COUNT` hashes
		expect(await contract.methods.blockHash(number).call()).to.eq(
			"0x0000000000000000000000000000000000000000000000000000000000000000"
		);
	});

	it("should get correct environmental block gaslimit", async function () {
		const contract = new context.web3.eth.Contract(TEST_CONTRACT_ABI, FIRST_CONTRACT_ADDRESS, {
			from: GENESIS_ACCOUNT,
			gasPrice: "0x3B9ACA00",
		});
		expect(await contract.methods.gasLimit().call()).to.eq(ETH_BLOCK_GAS_LIMIT.toString());
	});

	// Requires error handling
	it.skip("should fail for missing parameters", async function () {
		const contract = new context.web3.eth.Contract(
			[{ ...TEST_CONTRACT_ABI[0], inputs: [] }],
			FIRST_CONTRACT_ADDRESS,
			{
				from: GENESIS_ACCOUNT,
				gasPrice: "0x3B9ACA00",
			}
		);
		await contract.methods
			.multiply()
			.call()
			.catch((err) =>
				expect(err.message).to.equal(`Returned error: VM Exception while processing transaction: revert.`)
			);
	});

	// Requires error handling
	it.skip("should fail for too many parameters", async function () {
		const contract = new context.web3.eth.Contract(
			[
				{
					...TEST_CONTRACT_ABI[0],
					inputs: [
						{ internalType: "uint256", name: "a", type: "uint256" },
						{ internalType: "uint256", name: "b", type: "uint256" },
					],
				},
			],
			FIRST_CONTRACT_ADDRESS,
			{
				from: GENESIS_ACCOUNT,
				gasPrice: "0x3B9ACA00",
			}
		);
		await contract.methods
			.multiply(3, 4)
			.call()
			.catch((err) =>
				expect(err.message).to.equal(`Returned error: VM Exception while processing transaction: revert.`)
			);
	});

	// Requires error handling
	it.skip("should fail for invalid parameters", async function () {
		const contract = new context.web3.eth.Contract(
			[
				{
					...TEST_CONTRACT_ABI[0],
					inputs: [{ internalType: "address", name: "a", type: "address" }],
				},
			],
			FIRST_CONTRACT_ADDRESS,
			{ from: GENESIS_ACCOUNT, gasPrice: "0x3B9ACA00" }
		);
		await contract.methods
			.multiply("0x0123456789012345678901234567890123456789")
			.call()
			.catch((err) =>
				expect(err.message).to.equal(`Returned error: VM Exception while processing transaction: revert.`)
			);
	});
});
