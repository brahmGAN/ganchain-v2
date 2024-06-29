#!/usr/bin/env bash
set -e

if [ "$#" -ne 1 ]; then
	echo "Please provide the number of initial validators!"
	exit 1
fi

# Copy paste your mnemonic here.

generate_account_id() {
	echo "$1 $2"
	./target/release/gpu key inspect ${3:-} ${4:-} "$SECRET//$1//$2" | grep "Account ID" | awk '{ print $3 }'
}

generate_address() {
	./target/release/gpu key inspect ${3:-} ${4:-} "$SECRET//$1//$2" | grep "SS58 Address" | awk '{ print $3 }'
}

generate_public_key() {
	./target/release/gpu key inspect ${3:-} ${4:-} "$SECRET//$1//$2" | grep "Public" | awk '{ print $4 }'
}


generate_address_and_public_key() {
	ADDRESS=$(generate_address $1 $2 $3)
	PUBLIC_KEY=$(generate_public_key $1 $2 $3)

	printf "// $ADDRESS\narray_bytes::hex2array_unchecked(\"${ACCOUNT#'0x'}\").$INTO(),"
}

generate_address_and_account_id() {
	ACCOUNT=$(generate_account_id $1 $2 $3)
	ADDRESS=$(generate_address $1 $2 $3)
	if ${4:-false}; then
		INTO="unchecked_into"
        printf "// $ADDRESS\narray_bytes::hex2array_unchecked(\"${ACCOUNT#'0x'}\").$INTO(),"
	else
		INTO="into"
        printf "// $ADDRESS\narray_bytes::hex_n_into_unchecked(\"${ACCOUNT#'0x'}\"),"
	fi

	# printf "// $ADDRESS\nhex![\"${ACCOUNT#'0x'}\"].$INTO(),"
}

V_NUM=$1

AUTHORITIES=""

for i in $(seq 1 $V_NUM); do
	# AUTHORITIES+="(\n"
	AUTHORITIES+="$(generate_address_and_account_id $i stash)"
	AUTHORITIES+="$(generate_address_and_account_id $i controller)"
	AUTHORITIES+="$(generate_address_and_account_id $i grandpa '--scheme ed25519' true)"
	AUTHORITIES+="$(generate_address_and_account_id $i babe '--scheme sr25519' true)"
	AUTHORITIES+="$(generate_address_and_account_id $i im_online '--scheme sr25519' true)"
	AUTHORITIES+="$(generate_address_and_account_id $i authority_discovery '--scheme sr25519' true)"
	# AUTHORITIES+="),\n"
done

printf "$AUTHORITIES"
# printf "$SECRET"