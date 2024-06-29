#!/usr/bin/env bash

# This script can be used for releasing polkadot branch.

if  [[ $# -eq 1 && "${1}" == "--help" ]]; then
    echo "USAGE:"
    echo "  ${0} [<from>] <to>, like master or polkadot-v0.9.27"
elif [[ $# -ne 2 ]]; then
    to_branch=${1} 
    rg "https://github.com/brahmGAN/substrate" -t toml -T lock -l | xargs sed -i "s/master/$to_branch/g"
else
    from_branch=${1}
    to_branch=${2}
    rg "https://github.com/brahmGAN/substrate" -t toml -T lock -l | xargs sed -i "s/$from_branch/$to_branch/g"
fi
