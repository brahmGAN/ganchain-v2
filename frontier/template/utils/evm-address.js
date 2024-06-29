const help = `--evm-address <address>: Calculate the EVM address that corresponds to a native Substrate address.`;

module.exports = () => {
  if (process.argv.length < 4) {
    console.error('Please provide the <address> parameter.');
    console.error(help);
    process.exit(9);
  }
  
  const address = process.argv[3];
  if (!address.match(/^[A-z0-9]{48}$/)) {
    console.error('Please enter a valid Substrate address.');
    console.error(help);
    process.exit(9);
  }
  
  const crypto = require('@polkadot/util-crypto');
  return `0x${crypto.blake2AsHex(crypto.decodeAddress(address), 256).substring(26)}`;
};
