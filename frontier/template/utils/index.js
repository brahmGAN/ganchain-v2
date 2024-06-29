const help =
`--erc20-slot <slot> <address>: Calculate the storage slot for an (EVM) address's ERC-20 balance, where <slot> is the storage slot for the ERC-20 balances map.
--evm-address <address>: Calculate the EVM address that corresponds to a native Substrate address.
--help: Print this message.`;

if (process.argv.length < 3) {
  console.error('Please provide a command.');
  console.error(help);
  process.exit(9);
}

const command = process.argv[2];
switch (command) {
  case "--erc20-slot":
    console.log(require('./erc20-slot')());
    break;
  case "--evm-address":
    console.log(require('./evm-address')());
    break;
  case "--help":
    console.log(help);
    break;
  default:
    console.error(`Unrecognized command: ${command}.`);
    console.error(help);
    process.exit(9);
}
