#!/usr/bin/env bash

# This script can be used for running frontier's benchmarks.
#
# The frontier binary is required to be compiled with --features=runtime-benchmarks
# in release mode.

set -e

BINARY="./target/release/frontier-template-node"

function choose_and_bench {
    readarray -t options < <(${BINARY} benchmark pallet --list | sed 1d)
    options+=('EXIT')

    select opt in "${options[@]}"; do
        IFS=', ' read -ra parts <<< "${opt}"
        echo "${parts[0]} -- ${parts[1]}"
        [[ "${opt}" == 'EXIT' ]] && exit 0
        
        bench "${parts[0]}" "${parts[1]}"
        break
    done
}

function bench {
    echo "benchmarking ${1}::${2}"
    WASMTIME_BACKTRACE_DETAILS=1 ${BINARY} benchmark pallet \
        --chain=dev \
        --steps=50 \
        --repeat=20 \
        --pallet="${1}" \
        --extrinsic="${2}" \
        --execution=wasm \
        --wasm-execution=compiled \
        --output=weights.rs \
        --template=./benchmarking/frame-weight-template.hbs
}

if  [[ $# -eq 1 && "${1}" == "--help" ]]; then
    echo "USAGE:"
    echo "  ${0} [<pallet> <extrinsic>]" 
elif [[ $# -ne 2 ]]; then
    choose_and_bench
else
    bench "${1}" "${2}"
fi
