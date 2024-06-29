const help = `--erc20-slot <slot> <address>: Calculate the storage slot for an (EVM) address's ERC-20 balance, where <slot> is the storage slot for the ERC-20 balances map.`;

module.exports = () => {
	if (process.argv.length < 5) {
		console.error('Please provide both the <slot> and <address> parameters.');
		console.error(help);
		process.exit(9);
	}

	const slot = process.argv[3];
	const address = process.argv[4];
	if (!address.match(/^0x[a-f0-9]{40}$/)) {
		console.error('Please enter a valid EVM address.');
		console.error(help);
		process.exit(9);
	}

	const mapStorageSlot = slot.padStart(64, '0');
	const mapKey = address.substring(2).padStart(64, '0');

	console.log('0x'.concat(mapKey.concat(mapStorageSlot)))
	const web3 = require('web3');
	return web3.utils.sha3('0x'.concat(mapKey.concat(mapStorageSlot)));
};
