import Web3 from "web3";
import * as web3Utils from 'web3-utils';

const web3 = new Web3("http://127.0.0.1:9933");
const ERC20_BYTECODE = require("./truffle/contracts/MyToken.json").bytecode;
const STORAGE_SLOT = "0";

const createAccount = () => {
	const account = web3.eth.accounts.create();
	const mapStorageSlot = STORAGE_SLOT.padStart(64, '0');
	const mapKey = account.address.toString().substring(2).padStart(64, '0');
	const storageKey = web3Utils.sha3('0x'.concat(mapKey.concat(mapStorageSlot)));
	return {...account, storageKey};
}

const main = async () => {

	console.log("Generating accounts...");
	const alice = createAccount();
	const bob = createAccount();

	// Step 1: Creating the contract.
	console.log(`Alice account: ${alice.address}\n		storageKey [slot ${STORAGE_SLOT}]: ${alice.storageKey}`);
	console.log(`Bob account: ${bob.address}\n	  storageKey [slot ${STORAGE_SLOT}]: ${bob.storageKey}`);

	console.log(`\nCreating contract using Eth RPC "sendTransaction" from alice`);
	const createTransaction = await alice.signTransaction(
		{
			data: ERC20_BYTECODE,
			value: "0x00",
			gasPrice: "0x00",
			gas: "0x100000",
		});
	console.log("Transaction", {
		...createTransaction,
		rawTransaction: `${createTransaction.rawTransaction.substring(
			0,
			32
		)}... (${createTransaction.rawTransaction.length} length)`,
	});

	const createReceipt = await web3.eth.sendSignedTransaction(
		createTransaction.rawTransaction
	);
	console.log(
		`Contract deployed at address ${createReceipt.contractAddress}`
	);

	// Step 2: Sending contract tokens to bob
	console.log(`\nSending 221 Contract tokens from alice to bob`);
	const transferFnCode = `a9059cbb000000000000000000000000`;
	const tokensToTransfer = `00000000000000000000000000000000000000000000000000000000000000dd`;
	const inputCode = `0x${transferFnCode}${bob.address.substring(
		2
	)}${tokensToTransfer}`;

	const transferTransaction = await alice.signTransaction(
		{
			to: createReceipt.contractAddress,
			data: inputCode,
			value: "0x00",
			gasPrice: "0x00",
			gas: "0x100000",
		});
	console.log("Transaction", {
		...transferTransaction,
		rawTransaction: `${transferTransaction.rawTransaction.substring(
			0,
			32
		)}... (${transferTransaction.rawTransaction.length} length)`,
	});

	const transferReceipt = await web3.eth.sendSignedTransaction(
		transferTransaction.rawTransaction
	);
	console.log(
		`Transfer executed to ${transferReceipt.to} (H: ${transferReceipt.transactionHash})`
	);
};

main().catch((err) => {
	console.log("Error", err);
});
