# eBTC v2

eBTC v2 is a second charm. Holders of v1 keep the v1 app, the v1 verification key, and the v1 identity.

The v2 verification key is `39469872aa978cb935f239e7cd137088f5aa212c22a9da48260426cdff7d3ce0`. It is the SHA-256 of the 32-byte key in `vk/public-key.hex`. `vk/vk.hex` stores the same hex. Spell files use it as `app_vk`. The SHA-256 of `wasm/ebtc_v2.wasm` is the wasm hash. Spells record that hash on `versioned_apps`, separate from the verification key.

The live identity is the SHA-256 of the first input of the reference NFT mint. That input has to be a Bitcoin output the minter can spend. The wasm does not store the identity. `nft_arm` accepts the mint only when input 0 hashes to `app.identity`.

`identity.txt` records a test commitment. The outpoint `eb7c000000000000000000000000000000000000000000000000000000000001:0` is not a Bitcoin output. Its hash is `74b556e5b324e10c519c5f0f0f0ee8c43ffab84c9295477cbe99f0e4a9b47cea`. Tests use that pair. A live mint spends a real outpoint, and the identity is the hash of that outpoint.

A v2 lock is a Scrolls output. The spell lists the output index in `tx.scrolls` and leaves that output's dest empty. The prover fills dest with the canister script. A v1 vault coin matches the script of `bc1qrn970793udj0ugc3pj0hyrptts4rw5n7qxeya2`. v2 has no vault address constant.

The signature from `addresses` covers the output index and the script. It does not cover the creating transaction's first input. Binding that input belongs in `sign_script_pubkeys` and in `check_scroll_outputs` in Charms. This app cannot check it, and it does not fall back to a fixed address.

Do not lock mainnet bitcoin with these spells until canister `rpgc6-oqaaa-aaaak-qy3uq-cai` has been upgraded in place to wasm that verifies spell version 16. A new canister id derives different keys, so it cannot sign a withdrawal of an output that canister already controls.

`spells/lock.yaml` sets the token output amount from `${token_output_sats}`. Set it to at least the dust threshold of `change_dest`. 300 sats is above the P2WPKH dust limit. P2TR needs 330 sats. P2PKH needs 546 sats. The 300 sat figure in the contract is the reserve on each scroll output.

The app version is 1. The committed wasm is `wasm/ebtc_v2.wasm`. The committed signature is `wasm/ebtc_v2.wasm.sig.yaml`. The repository copy of the key is the public half.

## Build

Generate the signing key outside this repository. Commit `vk/public-key.hex`, `vk/vk.hex`, and the wasm signature. Leave the secret key out of the tree. `.gitignore` ignores `.charms/` and `app-key.json`.

```sh
rustup target add wasm32-wasip1 --toolchain 1.94.1
cargo build --locked --release --target wasm32-wasip1
```

Cargo writes `target/wasm32-wasip1/release/ebtc_v2.wasm`.

## Test

`v2/rust-toolchain.toml` pins Rust 1.94.1. The test dependency `charms-client` pulls `uplc` 1.1.23, and that crate requires this compiler. The v1 package stays on Rust 1.93.

```sh
cargo test --locked
```
