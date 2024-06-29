const Web3 = require('web3');
const web3 = new Web3('http://127.0.0.1:8545');

const ABI = [{"inputs":[],"payable":false,"stateMutability":"nonpayable","type":"constructor"},{"anonymous":false,"inputs":[{"indexed":false,"internalType":"string","name":"message","type":"string"}],"name":"Said","type":"event"},{"constant":true,"inputs":[],"name":"owner","outputs":[{"internalType":"address","name":"","type":"address"}],"payable":false,"stateMutability":"view","type":"function"}];
const BYTECODE = "0x608060405234801561001057600080fd5b50336000806101000a81548173ffffffffffffffffffffffffffffffffffffffff021916908373ffffffffffffffffffffffffffffffffffffffff1602179055507fc68045c3c562488255b55aa2c4c7849de001859ff0d8a36a75c2d5ed80100fb660405180806020018281038252600d8152602001807f48656c6c6f2c20776f726c64210000000000000000000000000000000000000081525060200191505060405180910390a160cf806100c76000396000f3fe6080604052348015600f57600080fd5b506004361060285760003560e01c80638da5cb5b14602d575b600080fd5b60336075565b604051808273ffffffffffffffffffffffffffffffffffffffff1673ffffffffffffffffffffffffffffffffffffffff16815260200191505060405180910390f35b6000809054906101000a900473ffffffffffffffffffffffffffffffffffffffff168156fea265627a7a72315820fae816ad954005c42bea7bc7cb5b19f7fd5d3a250715ca2023275c9ca7ce644064736f6c634300050f0032";

const main = async () => {
	const devAccount = web3.eth.accounts.create();

	const createTransaction = await web3.eth.accounts.signTransaction({
		from: devAccount.address,
		data: BYTECODE,
		value: "0x00",
		gasPrice: "0x00",
		gas: "0x100000",
	}, devAccount.privateKey);
	console.log(createTransaction);

	const createReceipt = await web3.eth.sendSignedTransaction(createTransaction.rawTransaction);
	console.log("Contract deployed at address", createReceipt.contractAddress);
};

main();
