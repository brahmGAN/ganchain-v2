import { expect } from "chai";
import { step } from "mocha-steps";

import { BLOCK_TIMESTAMP, ETH_BLOCK_GAS_LIMIT, GENESIS_ACCOUNT, GENESIS_ACCOUNT_PRIVATE_KEY } from "./config";
import { createAndFinalizeBlock, describeWithFrontier, customRequest } from "./util";

describeWithFrontier("Frontier RPC (Block)", (context) => {
	let previousBlock;
	// Those tests are dependant of each other in the given order.
	// The reason is to avoid having to restart the node each time
	// Running them individually will result in failure

	step("should be at block 0 at genesis", async function () {
		expect(await context.web3.eth.getBlockNumber()).to.equal(0);
	});

	it("should return genesis block by number", async function () {
		expect(await context.web3.eth.getBlockNumber()).to.equal(0);

		const block = await context.web3.eth.getBlock(0);
		expect(block).to.include({
			author: "0x0000000000000000000000000000000000000000",
			difficulty: "0",
			extraData: "0x",
			gasLimit: ETH_BLOCK_GAS_LIMIT,
			gasUsed: 0,
			logsBloom:
				"0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
			miner: "0x0000000000000000000000000000000000000000",
			number: 0,
			receiptsRoot: "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421",
			size: 505,
			timestamp: 0,
			totalDifficulty: "0",
		});

		expect(block.nonce).to.eql("0x0000000000000000");
		expect(block.hash).to.be.a("string").lengthOf(66);
		expect(block.parentHash).to.be.a("string").lengthOf(66);
		expect(block.timestamp).to.be.a("number");
		previousBlock = block;
	});

	step("should have empty uncles and correct sha3Uncles", async function () {
		const block = await context.web3.eth.getBlock(0);
		expect(block.uncles).to.be.a("array").empty;
		expect(block.sha3Uncles).to.equal("0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347");
	});

	step("should have empty transactions and correct transactionRoot", async function () {
		const block = await context.web3.eth.getBlock(0);
		expect(block.transactions).to.be.a("array").empty;
		expect(block).to.include({
			transactionsRoot: "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421",
		});
	});

	let firstBlockCreated = false;
	step("should be at block 1 after block production", async function () {
		this.timeout(15000);
		await createAndFinalizeBlock(context.web3);
		expect(await context.web3.eth.getBlockNumber()).to.equal(1);
		firstBlockCreated = true;
	});

	step("should have valid timestamp after block production", async function () {
		const block = await context.web3.eth.getBlock("latest");
		expect(block.timestamp).to.be.eq(BLOCK_TIMESTAMP);
	});

	it("genesis block should be already available by hash", async function () {
		const block = await context.web3.eth.getBlock(previousBlock.hash);
		expect(block).to.include({
			author: "0x0000000000000000000000000000000000000000",
			difficulty: "0",
			extraData: "0x",
			gasLimit: ETH_BLOCK_GAS_LIMIT,
			gasUsed: 0,
			logsBloom:
				"0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
			miner: "0x0000000000000000000000000000000000000000",
			number: 0,
			receiptsRoot: "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421",
			size: 505,
			timestamp: 0,
			totalDifficulty: "0",
		});

		expect(block.nonce).to.eql("0x0000000000000000");
		expect(block.hash).to.be.a("string").lengthOf(66);
		expect(block.parentHash).to.be.a("string").lengthOf(66);
		expect(block.timestamp).to.be.a("number");
	});

	step("retrieve block information", async function () {
		expect(firstBlockCreated).to.be.true;

		const block = await context.web3.eth.getBlock("latest");
		expect(block).to.include({
			author: "0x0000000000000000000000000000000000000000",
			difficulty: "0",
			extraData: "0x",
			gasLimit: ETH_BLOCK_GAS_LIMIT,
			gasUsed: 0,
			//hash: "0x14fe6f7c93597f79b901f8b5d7a84277a90915b8d355959b587e18de34f1dc17",
			logsBloom:
				"0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
			miner: "0x0000000000000000000000000000000000000000",
			number: 1,
			//parentHash: "0x04540257811b46d103d9896e7807040e7de5080e285841c5430d1a81588a0ce4",
			receiptsRoot: "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421",
			size: 507,
			timestamp: BLOCK_TIMESTAMP,
			totalDifficulty: "0",
			//transactions: [],
			transactionsRoot: "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421",
			//uncles: []
		});
		previousBlock = block;

		expect(block.transactions).to.be.a("array").empty;
		expect(block.uncles).to.be.a("array").empty;
		expect(block.nonce).to.eql("0x0000000000000000");
		expect(block.hash).to.be.a("string").lengthOf(66);
		expect(block.parentHash).to.be.a("string").lengthOf(66);
		expect(block.timestamp).to.be.a("number");
	});

	step("get block by hash", async function () {
		const latest_block = await context.web3.eth.getBlock("latest");
		const block = await context.web3.eth.getBlock(latest_block.hash);
		expect(block.hash).to.be.eq(latest_block.hash);
	});

	step("get block by number", async function () {
		const block = await context.web3.eth.getBlock(1);
		expect(block).not.null;
	});

	it.skip("should include previous block hash as parent", async function () {
		this.timeout(15000);
		await createAndFinalizeBlock(context.web3);
		const block = await context.web3.eth.getBlock("latest");
		expect(block.hash).to.not.equal(previousBlock.hash);
		expect(block.parentHash).to.equal(previousBlock.hash);
	});
});

describeWithFrontier("Frontier RPC (Pending Block)", (context) => {
	const TEST_ACCOUNT = "0x1111111111111111111111111111111111111111";

	it("should return pending block", async function () {
		var nonce = 0;
		let sendTransaction = async () => {
			const tx = await context.web3.eth.accounts.signTransaction(
				{
					from: GENESIS_ACCOUNT,
					to: TEST_ACCOUNT,
					value: "0x200", // Must be higher than ExistentialDeposit
					gasPrice: "0x3B9ACA00",
					gas: "0x100000",
					nonce: nonce,
				},
				GENESIS_ACCOUNT_PRIVATE_KEY
			);
			nonce = nonce + 1;
			return (await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction])).result;
		};

		// block 1 send 5 transactions
		const expectedXtsNumber = 5;
		for (var _ of Array(expectedXtsNumber)) {
			await sendTransaction();
		}

		// test still invalid future transactions can be safely applied (they are applied, just not overlayed)
		nonce = nonce + 100;
		await sendTransaction();

		// do not seal, get pendign block
		let pending_transactions = [];
		{
			const pending = (await customRequest(context.web3, "eth_getBlockByNumber", ["pending", false])).result;
			expect(pending.hash).to.be.null;
			expect(pending.miner).to.be.null;
			expect(pending.nonce).to.be.null;
			expect(pending.totalDifficulty).to.be.null;
			pending_transactions = pending.transactions;
			expect(pending_transactions.length).to.be.eq(expectedXtsNumber);
		}

		// seal and compare latest blocks transactions with the previously pending
		await createAndFinalizeBlock(context.web3);
		const latest_block = await context.web3.eth.getBlock("latest", false);
		expect(pending_transactions).to.be.deep.eq(latest_block.transactions);
	});
});
