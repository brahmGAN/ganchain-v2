import { ApiPromise, WsProvider, Keyring } from "@polkadot/api";
import { KeyringPair } from '@polkadot/keyring/types';
import { U8aFixed } from '@polkadot/types/codec';
import * as web3Utils from 'web3-utils';
import * as crypto from '@polkadot/util-crypto';

// Provider is set to 127.0.0.1 for development
const wsProvider = new WsProvider("ws://127.0.0.1:9944");

// Keyring needed to sign using Alice account
const keyring = new Keyring({ type: 'sr25519' });

// ByteCode of our ERC20 exemple: copied from ./truffle/contracts/MyToken.json
const ERC20_BYTECODES = require("./truffle/contracts/MyToken.json").bytecode;

// Setup the API and Alice Account
async function init() {
	console.log(`Initiating the API (ignore message "Unable to resolve type B..." and "Unknown types found...")`);

	// Initiate the polkadot API.
	const api = await ApiPromise.create({
		provider: wsProvider,
		types: {
			// mapping the actual specified address format
			Address: "AccountId",
			// mapping the lookup
			LookupSource: "AccountId",
			Account: {
				nonce: "U256",
				balance: "U256"
			},
			Transaction: {
				nonce: "U256",
				action: "String",
				gas_price: "u64",
				gas_limit: "u64",
				value: "U256",
				input: "Vec<u8>",
				signature: "Signature"
			},
			Signature: {
				v: "u64",
				r: "H256",
				s: "H256"
			}
		}
	});
	console.log(`Initialiation done`);
	console.log(`Genesis at block: ${api.genesisHash.toHex()}`);

	const alice = keyring.addFromUri('//Alice', { name: 'Alice default' });
	const bob = keyring.addFromUri('//Bob', { name: 'Bob default' });

	const { nonce, data: balance } = await api.query.system.account(alice.address);
	console.log(`Alice Substrate Account: ${alice.address}`);
	console.log(`Alice Substrate Account (nonce: ${nonce}) balance, free: ${balance.free.toHex()}`);

	const aliceEvmAccount = `0x${crypto.blake2AsHex(crypto.decodeAddress(alice.address), 256).substring(26)}`;

	console.log(`Alice EVM Account: ${aliceEvmAccount}`);
	const evmData = (await api.query.evm.accounts(aliceEvmAccount)) as any;
	console.log(`Alice EVM Account (nonce: ${evmData.nonce}) balance: ${evmData.balance.toHex()}`);

	return { api, alice, bob };
}

// Create the ERC20 contract from ALICE
async function step1(api: ApiPromise, alice: KeyringPair) {

	console.log(`\nStep 1: Creating Smart Contract`);

	// params: [bytecode, initialBalance, gasLimit, gasPrice],
	// tx: api.tx.evm.create

	const transaction = await api.tx.evm.create(ERC20_BYTECODES, 0, 4294967295, 1, null);

	const contract = new Promise<{ block: string, address: string }>(async (resolve, reject) => {
		const unsub = await transaction.signAndSend(alice, (result) => {
			console.log(`Contract creation is ${result.status}`);
			if (result.status.isInBlock) {
				console.log(`Contract included at blockHash ${result.status.asInBlock}`);
				console.log(`Waiting for finalization... (can take a minute)`);
			} else if (result.status.isFinalized) {
				const contractAddress = (
					result.events?.find(
						event => event?.event?.index.toHex() == "0x0500"
					)?.event.data[0] as any
				).address as string;
				console.log(`Contract finalized at blockHash ${result.status.asFinalized}`);
				console.log(`Contract address: ${contractAddress}`);
				unsub();
				resolve({
					block: result.status.asFinalized.toString(),
					address: contractAddress
				});
			}
		});
	});
	return contract;
}

// Retrieve Alice & Contract Storage
async function step2(api: ApiPromise, alice: KeyringPair, contractAddress: string) {

	console.log(`\nStep 2: Retrieving Contract from evm address: ${contractAddress}`);

	// Retrieve Alice account with new nonce value
	const { nonce, data: balance } = await api.query.system.account(alice.address);
	console.log(`Alice Substrate Account (nonce: ${nonce}) balance, free: ${balance.free}`);

	const accountCode = (await api.query.evm.accountCodes(contractAddress)).toString();
	console.log(`Contract account code: ${accountCode.substring(0, 16)}...${accountCode.substring(accountCode.length - 16)}`);

	// Computing Contract Storage Slot, using slot 0 and alice EVM account
	const aliceEvmAccount = `0x${crypto.blake2AsHex(crypto.decodeAddress(alice.address), 256).substring(26)}`;
	const slot = "0";
	const mapStorageSlot = slot.padStart(64, '0');
	const mapKey = aliceEvmAccount.toString().substring(2).padStart(64, '0');

	const storageKey = web3Utils.sha3('0x'.concat(mapKey.concat(mapStorageSlot)));
	console.log(`Alice Contract storage key: ${storageKey}`);

	const accountStorage = (await api.query.evm.accountStorages(contractAddress, storageKey)).toString();
	console.log(`Alice Contract account storage: ${accountStorage}`);
	return;
}


// Transfer tokens to Bob
async function step3(api: ApiPromise, alice: KeyringPair, bob: KeyringPair, contractAddress: string) {

	const bobEvmAccount = `0x${crypto.blake2AsHex(crypto.decodeAddress(bob.address), 256).substring(26)}`;
	console.log(`\nStep 3: Transfering Tokens to Bob EVM Account: ${bobEvmAccount}`);

	console.log(`Preparing transfer of 0xdd`);
	// params: [contractAddress, inputCode, value,m gasLimit, gasPrice],
	// tx: api.tx.evm.create
	const transferFnCode = `a9059cbb000000000000000000000000`;
	const tokensToTransfer = `00000000000000000000000000000000000000000000000000000000000000dd`;
	const inputCode = `0x${transferFnCode}${bobEvmAccount.substring(2)}${tokensToTransfer}`;
	console.log(`Sending call input: ${inputCode}`);
	const transaction = await api.tx.evm.call(contractAddress, inputCode, 0, 4294967295, 1, null);

	const data = new Promise<{ block: string, address: string }>(async (resolve, reject) => {
		const unsub = await transaction.signAndSend(alice, (result) => {
			console.log(`Transfer is ${result.status}`);
			if (result.status.isInBlock) {
				console.log(`Transfer included at blockHash ${result.status.asInBlock}`);
				console.log(`Waiting for finalization... (can take a minute)`);
			} else if (result.status.isFinalized) {
				console.log(`Transfer finalized at blockHash ${result.status.asFinalized}`);
				unsub();
				resolve();
			}
		});
	});
	return data;
}

// Retrieve Bob
async function step4(api: ApiPromise, bob: KeyringPair, contractAddress: string) {

	console.log(`\nStep 4: Retrieving Bob tokens`);

	// Retrieve Bob account with new nonce value
	const { nonce, data: balance } = await api.query.system.account(bob.address);
	console.log(`Bob Substrate Account (nonce: ${nonce}) balance, free: ${balance.free}`);
	const bobEvmAccount = `0x${crypto.blake2AsHex(crypto.decodeAddress(bob.address), 256).substring(26)}`;

	console.log(`Bob EVM Account: ${bobEvmAccount}`);
	const evmData = (await api.query.evm.accounts(bobEvmAccount)) as any;
	console.log(`Bob EVM Account (nonce: ${evmData.nonce}) balance: ${evmData.balance.toHex()}`);

	const slot = "0";
	const mapStorageSlot = slot.padStart(64, '0');
	const mapKey = bobEvmAccount.toString().substring(2).padStart(64, '0');

	const storageKey = web3Utils.sha3('0x'.concat(mapKey.concat(mapStorageSlot)));
	console.log(`Bob Contract storage key: ${storageKey}`);

	const accountStorage = (await api.query.evm.accountStorages(contractAddress, storageKey)).toString();
	console.log(`Bob Contract account storage: ${accountStorage}`);

	return;
}

async function main() {
	const { api, alice, bob } = await init();

	// step 1: Creating the contract from ALICE
	const contractAccount = await step1(api, alice)

	// step 2: Retrieving Alice and Contract information
	await step2(api, alice, contractAccount.address);

	// step 3: Transfering Smart Contract tokens from Alice to Bob
	await step3(api, alice, bob, contractAccount.address);

	// step 3: Retrieving Bob information
	await step4(api, bob, contractAccount.address);
}

main().catch(console.error).then(() => process.exit(0));
