pragma solidity ^0.8.2;

contract Storage {
  function getStorage(bytes32 key) public view returns (bytes32 value) {
      assembly {
          value := sload(key)
      }
  }
  function setStorage(bytes32 key, bytes32 value) public {
      assembly {
          sstore(key, value)
      }
  }
}
