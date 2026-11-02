# eBTC

eBTC (enchanted BTC) is Bitcoin wrapped as a charm.

Logo files are in [`assets/`](assets/).

eBTC tokens are minted by locking BTC in the eBTC vault and burned by spending BTC from it. The amount of eBTC minted or burned equals the BTC amount minus 300 sats (dust) per vault UTXO.

## Vault Address

The vault address is a dedicated [Scrolls](https://scrolls-v11.charms.dev) contract address generated with nonce `1129595493` (`eBTC` encoded as UTF-8 bytes, interpreted as a little-endian 32-bit integer):

```sh
curl https://scrolls-v11.charms.dev/main/address/1129595493
# bc1qrn970793udj0ugc3pj0hyrptts4rw5n7qxeya2
```

This address is hardcoded in the contract as `VAULT_DEST`.

A [CHIP-0420](https://github.com/CharmsDev/charms/tree/main/CHIPs/CHIP-0420) reference NFT provides on-chain metadata for the token (ticker, name, description, etc.). See [ref-nft.md](ref-nft.md) for NFT minting instructions.

## eBTC v2

eBTC v2 is a second charm. Holders of v1 keep the v1 app, the v1 verification key, and the v1 identity.

The v2 verification key is `39469872aa978cb935f239e7cd137088f5aa212c22a9da48260426cdff7d3ce0`. It is the SHA-256 of the 32-byte key in `v2/vk/public-key.hex`. `v2/vk/vk.hex` stores the same hex. Spell files use it as `app_vk`. The SHA-256 of `v2/wasm/ebtc_v2.wasm` is the wasm hash. Spells record that hash on `versioned_apps`, separate from the verification key.

The v2 identity is `74b556e5b324e10c519c5f0f0f0ee8c43ffab84c9295477cbe99f0e4a9b47cea`. It is the SHA-256 of the 36-byte form of outpoint `eb7c000000000000000000000000000000000000000000000000000000000001:0`. Those 36 bytes are `UtxoId::to_bytes`, the txid in internal byte order plus the index as a little-endian u32. The host passes this hash in as `app.identity`. The NFT mint spends that outpoint as input 0.

A v2 lock is a Scrolls output. The spell lists the output index in `tx.scrolls` and leaves that output's dest empty. The prover fills dest with the canister script. A v1 vault coin matches the script of `bc1qrn970793udj0ugc3pj0hyrptts4rw5n7qxeya2`.

The app version is 1. The committed wasm is `v2/wasm/ebtc_v2.wasm`. The committed signature is `v2/wasm/ebtc_v2.wasm.sig.yaml`. The repository copy of the key is the public half.

# Building

## Prerequisites

Install Wasm WASI P1 support:

```sh
rustup target add wasm32-wasip1
```

## Build

```sh
cargo update
app_bin=$(charms app build)
```

The resulting Wasm binary will be at `./target/wasm32-wasip1/release/ebtc.wasm`.

Get the verification key:

```sh
export app_vk=$(charms app vk $app_bin)
```

## Minting eBTC

To mint eBTC tokens, create a transaction that sends BTC to the vault address and attaches the token and vault contract charms to the outputs.

```sh
# UTXO you're spending
export in_utxo_0="<txid>:<vout>"

# app_id: hash of the UTXO ID of the 1st input of the reference NFT minting transaction
export app_id="<app_id_hex>"

# vault output: BTC amount to lock (eBTC minted = amount - 300 dust)
export vault_amount=10000

# eBTC minted
export mint_amount=9700  # vault_amount - 300 (dust)

# change output
export change_dest=$(charms util dest --addr <your_change_address>)
export change_amount=<change_sats>

cat ./spells/mint-token.yaml | envsubst | charms spell check \
  --prev-txs=<prev_txs_hex> \
  --app-bins=${app_bin}
```

The spell in `spells/mint-token.yaml` creates:
- An output at the vault address carrying the vault contract charm (`c`)
- An output carrying the newly minted eBTC tokens (`t`)

The token contract verifies that the eBTC minted equals the BTC locked minus 300 sats dust per vault UTXO.

## Burning eBTC

To burn eBTC and release the locked BTC, you need to spend a vault UTXO. Since the vault is a Scrolls contract address, spending from it requires signing via the Scrolls API.

1. Create a spell that spends the vault UTXO (destroying eBTC tokens) and sends the released BTC to your address. The transaction must include a fee output to the Scrolls fee address.

2. Query the Scrolls fee configuration:

```sh
curl https://scrolls-v11.charms.dev/config
```

The fee is calculated as: `fixed_cost + fee_per_input * n_inputs + fee_basis_points / 10000 * total_input_sats`. Add a fee output to the `fee_address` from the config.

3. Sign the vault input with Scrolls. The signing endpoint takes the unsigned transaction, the vault input indices with their derivation nonce, and the previous transactions:

```sh
curl -X POST https://scrolls-v11.charms.dev/main/sign \
  -H "Content-Type: application/json" \
  -d '{
    "sign_inputs": [{"index": 0, "nonce": 1129595493}],
    "prev_txs": ["<hex of the tx that created the vault UTXO>"],
    "tx_to_sign": "<unsigned_tx_hex>"
  }'
```

The response is the signed transaction hex.

4. The token contract verifies that the eBTC burned equals the BTC released from the vault minus 300 sats dust per vault input.
