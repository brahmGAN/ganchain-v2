import { expect } from "chai";
import { step } from "mocha-steps";

import { GENESIS_ACCOUNT, GENESIS_ACCOUNT_PRIVATE_KEY } from "./config";
import { createAndFinalizeBlock, customRequest, describeWithFrontierWs } from "./util";

describeWithFrontierWs("Frontier RPC (Subscription)", (context) => {
	let subscription;
	let logsGenerated = 0;

	const TEST_CONTRACT_BYTECODE =
		"0x608060405234801561001057600080fd5b50610041337fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff61004660201b60201c565b610291565b600073ffffffffffffffffffffffffffffffffffffffff168273ffffffffffffffffffffffffffffffffffffffff1614156100e9576040517f08c379a000000000000000000000000000000000000000000000000000000000815260040180806020018281038252601f8152602001807f45524332303a206d696e7420746f20746865207a65726f20616464726573730081525060200191505060405180910390fd5b6101028160025461020960201b610c7c1790919060201c565b60028190555061015d816000808573ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019081526020016000205461020960201b610c7c1790919060201c565b6000808473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020819055508173ffffffffffffffffffffffffffffffffffffffff16600073ffffffffffffffffffffffffffffffffffffffff167fddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef836040518082815260200191505060405180910390a35050565b600080828401905083811015610287576040517f08c379a000000000000000000000000000000000000000000000000000000000815260040180806020018281038252601b8152602001807f536166654d6174683a206164646974696f6e206f766572666c6f77000000000081525060200191505060405180910390fd5b8091505092915050565b610e3a806102a06000396000f3fe608060405234801561001057600080fd5b50600436106100885760003560e01c806370a082311161005b57806370a08231146101fd578063a457c2d714610255578063a9059cbb146102bb578063dd62ed3e1461032157610088565b8063095ea7b31461008d57806318160ddd146100f357806323b872dd146101115780633950935114610197575b600080fd5b6100d9600480360360408110156100a357600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff16906020019092919080359060200190929190505050610399565b604051808215151515815260200191505060405180910390f35b6100fb6103b7565b6040518082815260200191505060405180910390f35b61017d6004803603606081101561012757600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff169060200190929190803573ffffffffffffffffffffffffffffffffffffffff169060200190929190803590602001909291905050506103c1565b604051808215151515815260200191505060405180910390f35b6101e3600480360360408110156101ad57600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff1690602001909291908035906020019092919050505061049a565b604051808215151515815260200191505060405180910390f35b61023f6004803603602081101561021357600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff16906020019092919050505061054d565b6040518082815260200191505060405180910390f35b6102a16004803603604081101561026b57600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff16906020019092919080359060200190929190505050610595565b604051808215151515815260200191505060405180910390f35b610307600480360360408110156102d157600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff16906020019092919080359060200190929190505050610662565b604051808215151515815260200191505060405180910390f35b6103836004803603604081101561033757600080fd5b81019080803573ffffffffffffffffffffffffffffffffffffffff169060200190929190803573ffffffffffffffffffffffffffffffffffffffff169060200190929190505050610680565b6040518082815260200191505060405180910390f35b60006103ad6103a6610707565b848461070f565b6001905092915050565b6000600254905090565b60006103ce848484610906565b61048f846103da610707565b61048a85604051806060016040528060288152602001610d7060289139600160008b73ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff1681526020019081526020016000206000610440610707565b73ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054610bbc9092919063ffffffff16565b61070f565b600190509392505050565b60006105436104a7610707565b8461053e85600160006104b8610707565b73ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002060008973ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054610c7c90919063ffffffff16565b61070f565b6001905092915050565b60008060008373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020549050919050565b60006106586105a2610707565b8461065385604051806060016040528060258152602001610de160259139600160006105cc610707565b73ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002060008a73ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054610bbc9092919063ffffffff16565b61070f565b6001905092915050565b600061067661066f610707565b8484610906565b6001905092915050565b6000600160008473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002060008373ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054905092915050565b600033905090565b600073ffffffffffffffffffffffffffffffffffffffff168373ffffffffffffffffffffffffffffffffffffffff161415610795576040517f08c379a0000000000000000000000000000000000000000000000000000000008152600401808060200182810382526024815260200180610dbd6024913960400191505060405180910390fd5b600073ffffffffffffffffffffffffffffffffffffffff168273ffffffffffffffffffffffffffffffffffffffff16141561081b576040517f08c379a0000000000000000000000000000000000000000000000000000000008152600401808060200182810382526022815260200180610d286022913960400191505060405180910390fd5b80600160008573ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002060008473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020819055508173ffffffffffffffffffffffffffffffffffffffff168373ffffffffffffffffffffffffffffffffffffffff167f8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925836040518082815260200191505060405180910390a3505050565b600073ffffffffffffffffffffffffffffffffffffffff168373ffffffffffffffffffffffffffffffffffffffff16141561098c576040517f08c379a0000000000000000000000000000000000000000000000000000000008152600401808060200182810382526025815260200180610d986025913960400191505060405180910390fd5b600073ffffffffffffffffffffffffffffffffffffffff168273ffffffffffffffffffffffffffffffffffffffff161415610a12576040517f08c379a0000000000000000000000000000000000000000000000000000000008152600401808060200182810382526023815260200180610d056023913960400191505060405180910390fd5b610a7d81604051806060016040528060268152602001610d4a602691396000808773ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054610bbc9092919063ffffffff16565b6000808573ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002081905550610b10816000808573ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200190815260200160002054610c7c90919063ffffffff16565b6000808473ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff168152602001908152602001600020819055508173ffffffffffffffffffffffffffffffffffffffff168373ffffffffffffffffffffffffffffffffffffffff167fddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef836040518082815260200191505060405180910390a3505050565b6000838311158290610c69576040517f08c379a00000000000000000000000000000000000000000000000000000000081526004018080602001828103825283818151815260200191508051906020019080838360005b83811015610c2e578082015181840152602081019050610c13565b50505050905090810190601f168015610c5b5780820380516001836020036101000a031916815260200191505b509250505060405180910390fd5b5060008385039050809150509392505050565b600080828401905083811015610cfa576040517f08c379a000000000000000000000000000000000000000000000000000000000815260040180806020018281038252601b8152602001807f536166654d6174683a206164646974696f6e206f766572666c6f77000000000081525060200191505060405180910390fd5b809150509291505056fe45524332303a207472616e7366657220746f20746865207a65726f206164647265737345524332303a20617070726f766520746f20746865207a65726f206164647265737345524332303a207472616e7366657220616d6f756e7420657863656564732062616c616e636545524332303a207472616e7366657220616d6f756e74206578636565647320616c6c6f77616e636545524332303a207472616e736665722066726f6d20746865207a65726f206164647265737345524332303a20617070726f76652066726f6d20746865207a65726f206164647265737345524332303a2064656372656173656420616c6c6f77616e63652062656c6f77207a65726fa265627a7a72315820c7a5ffabf642bda14700b2de42f8c57b36621af020441df825de45fd2b3e1c5c64736f6c63430005100032";

	async function sendTransaction(context) {
		const tx = await context.web3.eth.accounts.signTransaction(
			{
				from: GENESIS_ACCOUNT,
				data: TEST_CONTRACT_BYTECODE,
				value: "0x00",
				gasPrice: "0x3B9ACA00",
				gas: "0x1000000",
			},
			GENESIS_ACCOUNT_PRIVATE_KEY
		);

		await customRequest(context.web3, "eth_sendRawTransaction", [tx.rawTransaction]);
		return tx;
	}

	step("should connect", async function () {
		await createAndFinalizeBlock(context.web3);
		// @ts-ignore
		const connected = context.web3.currentProvider.connected;
		expect(connected).to.equal(true);
	}).timeout(20000);

	step("should subscribe", async function () {
		subscription = context.web3.eth.subscribe("newBlockHeaders", function (error, result) {});

		let connected = false;
		let subscriptionId = "";
		expect(subscriptionId).is.empty;
		await new Promise<void>((resolve) => {
			subscription.on("connected", function (d: any) {
				connected = true;
				subscriptionId = d;
				resolve();
			});
		});

		subscription.unsubscribe();
		expect(connected).to.equal(true);
		expect(subscriptionId).not.empty;
	}).timeout(20000);

	step("should get newHeads stream", async function (done) {
		subscription = context.web3.eth.subscribe("newBlockHeaders", function (error, result) {});
		let data = null;
		let dataResolve = null;
		let dataPromise = new Promise((resolve) => {
			dataResolve = resolve;
		});
		subscription.on("data", function (d: any) {
			data = d;
			subscription.unsubscribe();
			dataResolve();
		});

		await createAndFinalizeBlock(context.web3);
		await dataPromise;

		expect(data).to.include({
			author: "0x0000000000000000000000000000000000000000",
			difficulty: "0",
			extraData: "0x",
			logsBloom:
				"0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
			miner: "0x0000000000000000000000000000000000000000",
			number: 2,
			receiptsRoot: "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421",
			sha3Uncles: "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
			transactionsRoot: "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421",
		});
		expect(data.nonce).to.eql("0x0000000000000000");

		done();
	}).timeout(40000);

	step("should get newPendingTransactions stream", async function (done) {
		subscription = context.web3.eth.subscribe("pendingTransactions", function (error, result) {});

		await new Promise<void>((resolve) => {
			subscription.on("connected", function (d: any) {
				resolve();
			});
		});

		const tx = await sendTransaction(context);
		let data = null;
		await new Promise<void>((resolve) => {
			subscription.on("data", function (d: any) {
				data = d;
				logsGenerated += 1;
				resolve();
			});
		});

		subscription.unsubscribe();
		expect(data).to.be.not.null;
		expect(tx["transactionHash"]).to.be.eq(data);

		done();
	}).timeout(20000);

	step("should subscribe to all logs", async function (done) {
		subscription = context.web3.eth.subscribe("logs", {}, function (error, result) {});

		await new Promise<void>((resolve) => {
			subscription.on("connected", function (d: any) {
				resolve();
			});
		});

		const tx = await sendTransaction(context);
		let data = null;
		let dataResolve = null;
		let dataPromise = new Promise((resolve) => {
			dataResolve = resolve;
		});
		subscription.on("data", function (d: any) {
			data = d;
			logsGenerated += 1;
			dataResolve();
		});

		await createAndFinalizeBlock(context.web3);
		await dataPromise;

		subscription.unsubscribe();
		const block = await context.web3.eth.getBlock("latest");
		expect(data).to.include({
			blockHash: block.hash,
			blockNumber: block.number,
			data: "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
			logIndex: 0,
			removed: false,
			transactionHash: block.transactions[0],
			transactionIndex: 0,
			transactionLogIndex: "0x0",
		});
		done();
	}).timeout(20000);

	step("should subscribe to logs by multiple addresses", async function (done) {
		subscription = context.web3.eth.subscribe(
			"logs",
			{
				address: [
					"0xF8cef78E923919054037a1D03662bBD884fF4edf",
					"0x42e2EE7Ba8975c473157634Ac2AF4098190fc741",
					"0x5c4242beB94dE30b922f57241f1D02f36e906915",
					"0xC2Bf5F29a4384b1aB0C063e1c666f02121B6084a",
				],
			},
			function (error, result) {}
		);

		await new Promise<void>((resolve) => {
			subscription.on("connected", function (d: any) {
				resolve();
			});
		});

		const tx = await sendTransaction(context);
		let data = null;
		let dataResolve = null;
		let dataPromise = new Promise((resolve) => {
			dataResolve = resolve;
		});
		subscription.on("data", function (d: any) {
			data = d;
			logsGenerated += 1;
			dataResolve();
		});

		await createAndFinalizeBlock(context.web3);
		await dataPromise;

		subscription.unsubscribe();
		expect(data).to.not.be.null;
		done();
	}).timeout(20000);

	step("should subscribe to logs by topic", async function (done) {
		subscription = context.web3.eth.subscribe(
			"logs",
			{
				topics: ["0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"],
			},
			function (error, result) {}
		);

		await new Promise<void>((resolve) => {
			subscription.on("connected", function (d: any) {
				resolve();
			});
		});

		const tx = await sendTransaction(context);
		let data = null;
		let dataResolve = null;
		let dataPromise = new Promise((resolve) => {
			dataResolve = resolve;
		});

		subscription.on("data", function (d: any) {
			data = d;
			logsGenerated += 1;
			dataResolve();
		});

		await createAndFinalizeBlock(context.web3);
		await dataPromise;

		subscription.unsubscribe();
		expect(data).to.not.be.null;
		done();
	}).timeout(20000);

	step("should get past events #1: by topic", async function (done) {
		subscription = context.web3.eth.subscribe(
			"logs",
			{
				fromBlock: "0x0",
				topics: ["0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"],
			},
			function (error, result) {}
		);

		let data = [];
		await new Promise<void>((resolve) => {
			subscription.on("data", function (d: any) {
				data.push(d);
				resolve();
			});
		});
		subscription.unsubscribe();

		expect(data).to.not.be.empty;
		done();
	}).timeout(20000);

	step("should get past events #2: by address", async function (done) {
		subscription = context.web3.eth.subscribe(
			"logs",
			{
				fromBlock: "0x0",
				address: "0x42e2EE7Ba8975c473157634Ac2AF4098190fc741",
			},
			function (error, result) {}
		);

		let data = [];
		await new Promise<void>((resolve) => {
			subscription.on("data", function (d: any) {
				data.push(d);
				resolve();
			});
		});
		subscription.unsubscribe();

		expect(data).to.not.be.empty;
		done();
	}).timeout(20000);

	step("should get past events #3: by address + topic", async function (done) {
		subscription = context.web3.eth.subscribe(
			"logs",
			{
				fromBlock: "0x0",
				topics: ["0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"],
				address: "0xC2Bf5F29a4384b1aB0C063e1c666f02121B6084a",
			},
			function (error, result) {}
		);

		let data = [];
		await new Promise<void>((resolve) => {
			subscription.on("data", function (d: any) {
				data.push(d);
				resolve();
			});
		});
		subscription.unsubscribe();

		expect(data).to.not.be.empty;
		done();
	}).timeout(20000);

	step("should get past events #4: multiple addresses", async function (done) {
		subscription = context.web3.eth.subscribe(
			"logs",
			{
				fromBlock: "0x0",
				topics: ["0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"],
				address: [
					"0xe573BCA813c741229ffB2488F7856C6cAa841041",
					"0xF8cef78E923919054037a1D03662bBD884fF4edf",
					"0x42e2EE7Ba8975c473157634Ac2AF4098190fc741",
					"0x5c4242beB94dE30b922f57241f1D02f36e906915",
					"0xC2Bf5F29a4384b1aB0C063e1c666f02121B6084a",
				],
			},
			function (error, result) {}
		);

		let data = [];
		await new Promise<void>((resolve) => {
			subscription.on("data", function (d: any) {
				data.push(d);
				resolve();
			});
		});
		subscription.unsubscribe();

		expect(data).to.not.be.empty;
		done();
	}).timeout(20000);

	step("should support topic wildcards", async function (done) {
		subscription = context.web3.eth.subscribe(
			"logs",
			{
				topics: [null, "0x0000000000000000000000000000000000000000000000000000000000000000"],
			},
			function (error, result) {}
		);

		await new Promise<void>((resolve) => {
			subscription.on("connected", function (d: any) {
				resolve();
			});
		});

		const tx = await sendTransaction(context);
		let data = null;
		let dataResolve = null;
		let dataPromise = new Promise((resolve) => {
			dataResolve = resolve;
		});

		subscription.on("data", function (d: any) {
			data = d;
			logsGenerated += 1;
			dataResolve();
		});

		await createAndFinalizeBlock(context.web3);
		await dataPromise;

		subscription.unsubscribe();
		expect(data).to.not.be.null;
		done();
	}).timeout(20000);

	step("should support single values wrapped around a sequence", async function (done) {
		subscription = context.web3.eth.subscribe(
			"logs",
			{
				topics: [
					["0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef"],
					["0x0000000000000000000000000000000000000000000000000000000000000000"],
				],
			},
			function (error, result) {}
		);

		await new Promise<void>((resolve) => {
			subscription.on("connected", function (d: any) {
				resolve();
			});
		});

		const tx = await sendTransaction(context);
		let data = null;
		let dataResolve = null;
		let dataPromise = new Promise((resolve) => {
			dataResolve = resolve;
		});

		subscription.on("data", function (d: any) {
			data = d;
			logsGenerated += 1;
			dataResolve();
		});

		await createAndFinalizeBlock(context.web3);
		await dataPromise;

		subscription.unsubscribe();
		expect(data).to.not.be.null;
		done();
	}).timeout(20000);

	step("should support topic conditional parameters", async function (done) {
		subscription = context.web3.eth.subscribe(
			"logs",
			{
				topics: [
					"0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
					[
						"0x0000000000000000000000006be02d1d3665660d22ff9624b7be0551ee1ac91b",
						"0x0000000000000000000000000000000000000000000000000000000000000000",
					],
				],
			},
			function (error, result) {}
		);

		await new Promise<void>((resolve) => {
			subscription.on("connected", function (d: any) {
				resolve();
			});
		});

		const tx = await sendTransaction(context);
		let data = null;
		let dataResolve = null;
		let dataPromise = new Promise((resolve) => {
			dataResolve = resolve;
		});

		subscription.on("data", function (d: any) {
			data = d;
			logsGenerated += 1;
			dataResolve();
		});

		await createAndFinalizeBlock(context.web3);
		await dataPromise;

		subscription.unsubscribe();
		expect(data).to.not.be.null;
		done();
	}).timeout(20000);

	step("should support multiple topic conditional parameters", async function (done) {
		subscription = context.web3.eth.subscribe(
			"logs",
			{
				topics: [
					"0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef",
					[
						"0x0000000000000000000000000000000000000000000000000000000000000000",
						"0x0000000000000000000000006be02d1d3665660d22ff9624b7be0551ee1ac91b",
					],
					[
						"0x0000000000000000000000006be02d1d3665660d22ff9624b7be0551ee1ac91b",
						"0x0000000000000000000000000000000000000000000000000000000000000000",
					],
				],
			},
			function (error, result) {}
		);

		await new Promise<void>((resolve) => {
			subscription.on("connected", function (d: any) {
				resolve();
			});
		});

		const tx = await sendTransaction(context);
		let data = null;
		let dataResolve = null;
		let dataPromise = new Promise((resolve) => {
			dataResolve = resolve;
		});
		subscription.on("data", function (d: any) {
			data = d;
			logsGenerated += 1;
			dataResolve();
		});

		await createAndFinalizeBlock(context.web3);
		await dataPromise;

		subscription.unsubscribe();
		expect(data).to.not.be.null;
		done();
	}).timeout(20000);

	step("should combine topic wildcards and conditional parameters", async function (done) {
		subscription = context.web3.eth.subscribe(
			"logs",
			{
				topics: [
					null,
					[
						"0x0000000000000000000000006be02d1d3665660d22ff9624b7be0551ee1ac91b",
						"0x0000000000000000000000000000000000000000000000000000000000000000",
					],
					null,
				],
			},
			function (error, result) {}
		);

		await new Promise<void>((resolve) => {
			subscription.on("connected", function (d: any) {
				resolve();
			});
		});

		const tx = await sendTransaction(context);
		let data = null;
		let dataResolve = null;
		let dataPromise = new Promise((resolve) => {
			dataResolve = resolve;
		});
		subscription.on("data", function (d: any) {
			data = d;
			logsGenerated += 1;
			dataResolve();
		});

		await createAndFinalizeBlock(context.web3);
		await dataPromise;

		subscription.unsubscribe();
		expect(data).to.not.be.null;
		done();
	}).timeout(20000);
});
