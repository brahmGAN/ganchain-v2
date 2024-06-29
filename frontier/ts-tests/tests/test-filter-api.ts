import { expect } from "chai";
import { step } from "mocha-steps";

import { GENESIS_ACCOUNT, GENESIS_ACCOUNT_PRIVATE_KEY } from "./config";
import { createAndFinalizeBlock, createAndFinalizeBlockNowait, describeWithFrontier, customRequest } from "./util";

describeWithFrontier("Frontier RPC (EthFilterApi)", (context) => {
	const TEST_CONTRACT_BYTECODE =
		"0x608060405234801561001057600080fd5b50610041337fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff61004660201b60201c565b610291565b600073ffffffffffffffffffffffffffffffffffffffff168273ffffffffffffffffffffffffffffffffffffffff1614156100e9576040517f08c379a000000000000000000000000000000000000000000000000000000000815260040180806020018281038252601f8152602001807f45524332303a206d696e7420746f20746865207a65726f20616464726573730081525060200191505060405180910390fd5b6101028160025461020960201b610c7c1790919060201c565b60028190555061015d816000808573ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019081526020016000205461020960201b610c7c1790919060201c565b6000808473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020819055508173ffffffffffffffffffffffffffffffffffffffff16600073ffffffffffffffffffffffffffffffffffffffff167fddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef836040518082815260200191505060405180910390a35050565b600080828401905083811015610287576040517f08c379a000000000000000000000000000000000000000000000000000000000815260040180806020018281038252601b8152602001807f536166654d6174683a206164646974696f6e206f766572666c6f77000000000081525060200191505060405180910390fd5b8091505092915050565b610e3a806102a06000396000f3fe608060405234801561001057600080fd5b50600436106100885760003560e01c806370a082311161005b57806370a08231146101fd578063a457c2d714610255578063a9059cbb146102bb578063dd62ed3e1461032157610088565b8063095ea7b31461008d57806318160ddd146100f357806323b872dd146101115780633950935114610197575b600080fd5b6100d9600480360360408110156100a357600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff16906020019092919080359060200190929190505050610399565b604051808215151515815260200191505060405180910390f35b6100fb6103b7565b6040518082815260200191505060405180910390f35b61017d6004803603606081101561012757600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff169060200190929190803573ffffffffffffffffffffffffffffffffffffffff169060200190929190803590602001909291905050506103c1565b604051808215151515815260200191505060405180910390f35b6101e3600480360360408110156101ad57600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff1690602001909291908035906020019092919050505061049a565b604051808215151515815260200191505060405180910390f35b61023f6004803603602081101561021357600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff16906020019092919050505061054d565b6040518082815260200191505060405180910390f35b6102a16004803603604081101561026b57600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff16906020019092919080359060200190929190505050610595565b604051808215151515815260200191505060405180910390f35b610307600480360360408110156102d157600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff16906020019092919080359060200190929190505050610662565b604051808215151515815260200191505060405180910390f35b6103836004803603604081101561033757600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff169060200190929190803573ffffffffffffffffffffffffffffffffffffffff169060200190929190505050610680565b6040518082815260200191505060405180910390f35b60006103ad6103a6610707565b848461070f565b6001905092915050565b6000600254905090565b60006103ce848484610906565b61048f846103da610707565b61048a85604051806060016040528060288152602001610d7060289139600160008b73ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019081526020016000206000610440610707565b73ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054610bbc9092919063ffffffff16565b61070f565b600190509392505050565b60006105436104a7610707565b8461053e85600160006104b8610707565b73ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002060008973ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054610c7c90919063ffffffff16565b61070f565b6001905092915050565b60008060008373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020549050919050565b60006106586105a2610707565b8461065385604051806060016040528060258152602001610de160259139600160006105cc610707565b73ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002060008a73ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054610bbc9092919063ffffffff16565b61070f565b6001905092915050565b600061067661066f610707565b8484610906565b6001905092915050565b6000600160008473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002060008373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054905092915050565b600033905090565b600073ffffffffffffffffffffffffffffffffffffffff168373ffffffffffffffffffffffffffffffffffffffff161415610795576040517f08c379a0000000000000000000000000000000000000000000000000000000008152600401808060200182810382526024815260200180610dbd6024913960400191505060405180910390fd5b600073ffffffffffffffffffffffffffffffffffffffff168273ffffffffffffffffffffffffffffffffffffffff16141561081b576040517f08c379a0000000000000000000000000000000000000000000000000000000008152600401808060200182810382526022815260200180610d286022913960400191505060405180910390fd5b80600160008573ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002060008473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020819055508173ffffffffffffffffffffffffffffffffffffffff168373ffffffffffffffffffffffffffffffffffffffff167f8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925836040518082815260200191505060405180910390a3505050565b600073ffffffffffffffffffffffffffffffffffffffff168373ffffffffffffffffffffffffffffffffffffffff16141561098c576040517f08c379a0000000000000000000000000000000000000000000000000000000008152600401808060200182810382526025815260200180610d986025913960400191505060405180910390fd5b600073ffffffffffffffffffffffffffffffffffffffff168273ffffffffffffffffffffffffffffffffffffffff161415610a12576040517f08c379a0000000000000000000000000000000000000000000000000000000008152600401808060200182810382526023815260200180610d056023913960400191505060405180910390fd5b610a7d81604051806060016040528060268152602001610d4a602691396000808773ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054610bbc9092919063ffffffff16565b6000808573ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002081905550610b10816000808573ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054610c7c90919063ffffffff16565b6000808473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020819055508173ffffffffffffffffffffffffffffffffffffffff168373ffffffffffffffffffffffffffffffffffffffff167fddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef836040518082815260200191505060405180910390a3505050565b6000838311158290610c69576040517f08c379a00000000000000000000000000000000000000000000000000000000081526004018080602001828103825283818151815260200191508051906020019080838360005b83811015610c2e578082015181840152602081019050610c13565b50505050905090810190601f168015610c5b5780820380516001836020036101000a031916815260200191505b509250505060405180910390fd5b5060008385039050809150509392505050565b600080828401905083811015610cfa576040517f08c379a000000000000000000000000000000000000000000000000000000000815260040180806020018281038252601b8152602001807f536166654d6174683a206164646974696f6e206f766572666c6f77000000000081525060200191505060405180910390fd5b809150509291505056fe45524332303a207472616e7366657220746f20746865207a65726f206164647265737345524332303a20617070726f766520746f20746865207a65726f206164647265737345524332303a207472616e7366657220616d6f756e7420657863656564732062616c616e636545524332303a207472616e7366657220616d6f756e74206578636565647320616c6c6f77616e636545524332303a207472616e736665722066726f6d20746865207a65726f206164647265737345524332303a20617070726f76652066726f6d20746865207a65726f206164647265737345524332303a2064656372656173656420616c6c6f77616e63652062656c6f77207a65726fa265627a7a72315820c7a5ffabf642bda14700b2de42f8c57b36621af020441df825de45fd2b3e1c5c64736f6c63430005100032";

	var nonce = 0;
	async function sendTransaction(context) {
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: TEST_CONTRACT_BYTECODE,
				value: "0x00",
				gasPrice: "0x3B9ACA00",
				gas: "0x100000",
				nonce: nonce,
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);
		nonce = nonce + 1;
		await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		return tx;
	}

	step("should create a Log filter and return the ID", async function () {
		let createFilter = await customRequest(context.web3, "eth_newFilter", [
			{
				fromBlock: "0x0",
				toBlock: "latest",
				address: ["0xC2Bf5F29a4384b1aB0C063e1c666f02121B6084a", "0x5c4242beB94dE30b922f57241f1D02f36e906915"],
				topics: ["0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"],
			},
		]);
		expect(createFilter.result).to.be.eq("0x1");
	});

	step("should increment filter ID", async function () {
		let createFilter = await customRequest(context.web3, "eth_newFilter", [
			{
				fromBlock: "0x1",
				toBlock: "0x2",
				address: "0xC2Bf5F29a4384b1aB0C063e1c666f02121B6084a",
				topics: ["0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"],
			},
		]);
		expect(createFilter.result).to.be.eq("0x2");
	});

	step("should create a Block filter and return the ID", async function () {
		let createFilter = await customRequest(context.web3, "eth_newBlockFilter", []);
		expect(createFilter.result).to.be.eq("0x3");
	});

	step("should return unsupported error for Pending Transaction filter creation", async function () {
		let createFilter = await customRequest(context.web3, "eth_newPendingTransactionFilter", []);
		expect(createFilter.result).to.be.eq("0x4");
	});

	step("should return responses for Block filter polling.", async function () {
		let block = await context.web3.eth.getBlock(0);
		let poll = await customRequest(context.web3, "eth_getFilterChanges", ["0x3"]);

		expect(poll.result.length).to.be.eq(1);
		expect(poll.result[0]).to.be.eq(block.hash);

		await createAndFinalizeBlock(context.web3);

		block = await context.web3.eth.getBlock(1);
		poll = await customRequest(context.web3, "eth_getFilterChanges", ["0x3"]);

		expect(poll.result.length).to.be.eq(1);
		expect(poll.result[0]).to.be.eq(block.hash);

		await createAndFinalizeBlock(context.web3);
		await createAndFinalizeBlock(context.web3);

		block = await context.web3.eth.getBlock(2);
		let block_b = await context.web3.eth.getBlock(3);
		poll = await customRequest(context.web3, "eth_getFilterChanges", ["0x3"]);

		expect(poll.result.length).to.be.eq(2);
		expect(poll.result[0]).to.be.eq(block.hash);
		expect(poll.result[1]).to.be.eq(block_b.hash);
	});

	step("should return responses for pending transaction polling.", async function () {
		let poll = await customRequest(context.web3, "eth_getFilterChanges", ["0x4"]);
		expect(poll.result.length).to.be.eq(0);

		// fist polling
		let tx = await sendTransaction(context);
		poll = await customRequest(context.web3, "eth_getFilterChanges", ["0x4"]);
		expect(poll.result.length).to.be.eq(1);
		expect(poll.result).contains(tx.transactionHash);

		// second polling
		let tx1 = await sendTransaction(context);
		let tx2 = await sendTransaction(context);
		poll = await customRequest(context.web3, "eth_getFilterChanges", ["0x4"]);
		expect(poll.result.length).to.be.eq(2);
		expect(poll.result).contains(tx1.transactionHash);
		expect(poll.result).contains(tx2.transactionHash);

		await createAndFinalizeBlock(context.web3);

		// the last polling after finalized block
		poll = await customRequest(context.web3, "eth_getFilterChanges", ["0x4"]);
		expect(poll.result.length).to.be.eq(0);
	});

	step("should return responses for Log filter polling.", async function () {
		// Create contract.
		let tx = await sendTransaction(context);
		await createAndFinalizeBlock(context.web3);
		let receipt = await context.web3.eth.getTransactionReceipt(tx.transactionHash);

		expect(receipt.logs.length).to.be.eq(1);

		// Create a filter for the created contract.
		let createFilter = await customRequest(context.web3, "eth_newFilter", [
			{
				fromBlock: "0x0",
				toBlock: "latest",
				address: receipt.contractAddress,
				topics: receipt.logs[0].topics,
			},
		]);
		let poll = await customRequest(context.web3, "eth_getFilterChanges", [createFilter.result]);

		expect(poll.result.length).to.be.eq(1);
		expect(poll.result[0].address.toLowerCase()).to.be.eq(receipt.contractAddress.toLowerCase());
		expect(poll.result[0].topics).to.be.deep.eq(receipt.logs[0].topics);

		// A subsequent request must be empty.
		poll = await customRequest(context.web3, "eth_getFilterChanges", [createFilter.result]);
		expect(poll.result.length).to.be.eq(0);
	});

	step("should return response for raw Log filter request.", async function () {
		// Create contract.
		let tx = await sendTransaction(context);
		await createAndFinalizeBlock(context.web3);
		let receipt = await context.web3.eth.getTransactionReceipt(tx.transactionHash);

		expect(receipt.logs.length).to.be.eq(1);

		// Create a filter for the created contract.
		let createFilter = await customRequest(context.web3, "eth_newFilter", [
			{
				fromBlock: "0x0",
				toBlock: "latest",
				address: receipt.contractAddress,
				topics: receipt.logs[0].topics,
			},
		]);
		let poll = await customRequest(context.web3, "eth_getFilterLogs", [createFilter.result]);

		expect(poll.result.length).to.be.eq(1);
		expect(poll.result[0].address.toLowerCase()).to.be.eq(receipt.contractAddress.toLowerCase());
		expect(poll.result[0].topics).to.be.deep.eq(receipt.logs[0].topics);

		// A subsequent request must return the same response.
		poll = await customRequest(context.web3, "eth_getFilterLogs", [createFilter.result]);

		expect(poll.result.length).to.be.eq(1);
		expect(poll.result[0].address.toLowerCase()).to.be.eq(receipt.contractAddress.toLowerCase());
		expect(poll.result[0].topics).to.be.deep.eq(receipt.logs[0].topics);
	});

	step("should uninstall created filters.", async function () {
		let createFilter = await customRequest(context.web3, "eth_newBlockFilter", []);
		let filterId = createFilter.result;

		// Should return true when removed from the filter pool.
		let uninstall = await customRequest(context.web3, "eth_uninstallFilter", [filterId]);
		expect(uninstall.result).to.be.eq(true);

		// Should return error if does not exist.
		let r = await customRequest(context.web3, "eth_uninstallFilter", [filterId]);
		expect(r.error).to.include({
			message: "Filter id 7 does not exist.",
		});
	});

	step("should drain the filter pool.", async function () {
		this.timeout(15000);
		const blockLifespanThreshold = 100;

		let createFilter = await customRequest(context.web3, "eth_newBlockFilter", []);
		let filterId = createFilter.result;

		for (let i = 0; i <= blockLifespanThreshold; i++) {
			await createAndFinalizeBlockNowait(context.web3);
		}

		let r = await customRequest(context.web3, "eth_getFilterChanges", [filterId]);
		expect(r.error).to.include({
			message: "Filter id 7 does not exist.",
		});
	});

	step("should have a filter pool max size of 500.", async function () {
		const maxFilterPool = 500;

		for (let i = 0; i < maxFilterPool; i++) {
			await customRequest(context.web3, "eth_newBlockFilter", []);
		}

		let r = await customRequest(context.web3, "eth_newBlockFilter", []);
		expect(r.error).to.include({
			message: "Filter pool is full (limit 500).",
		});
	});
});
