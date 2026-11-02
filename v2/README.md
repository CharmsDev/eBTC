# eBTC v2

eBTC v2 is a second charm. Holders of v1 keep the v1 app, the v1 verification key, and the v1 identity.

The v2 verification key is `39469872aa978cb935f239e7cd137088f5aa212c22a9da48260426cdff7d3ce0`. It is the SHA-256 of the 32-byte key in `vk/public-key.hex`. `vk/vk.hex` stores the same hex. Spell files use it as `app_vk`. The SHA-256 of `wasm/ebtc_v2.wasm` is the wasm hash. Spells record that hash on `versioned_apps`, separate from the verification key.

The v2 identity is `74b556e5b324e10c519c5f0f0f0ee8c43ffab84c9295477cbe99f0e4a9b47cea`. It is the SHA-256 of the 36-byte form of outpoint `eb7c000000000000000000000000000000000000000000000000000000000001:0`. Those 36 bytes are `UtxoId::to_bytes`, the txid in internal byte order plus the index as a little-endian u32. The host passes this hash in as `app.identity`. The NFT mint spends that outpoint as input 0. `identity.txt` records the outpoint and the identity.

A v2 lock is a Scrolls output. The spell lists the output index in `tx.scrolls` and leaves that output's dest empty. The prover fills dest with the canister script. A v1 vault coin matches the script of `bc1qrn970793udj0ugc3pj0hyrptts4rw5n7qxeya2`. v2 has no vault address constant.

The app version is 1. The committed wasm is `wasm/ebtc_v2.wasm`. The committed signature is `wasm/ebtc_v2.wasm.sig.yaml`. The repository copy of the key is the public half.

## Build

Generate the signing key outside this repository. Commit `vk/public-key.hex`, `vk/vk.hex`, and the wasm signature. Leave the secret key out of the tree. `.gitignore` ignores `.charms/` and `app-key.json`.

```sh
rustup target add wasm32-wasip1 --toolchain 1.93
cargo build --locked --release --target wasm32-wasip1
```

Cargo writes `target/wasm32-wasip1/release/ebtc_v2.wasm`.

## Test

`charms-client` depends on `uplc` 1.1.23, which declares Rust 1.94.1. This package uses Rust 1.93. `--ignore-rust-version` tells Cargo to skip that declaration. The tests compile and run on 1.93.

```sh
cargo test --locked --ignore-rust-version
```
