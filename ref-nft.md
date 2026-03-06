# eBTC Reference NFT

The eBTC reference NFT is a [CHIP-0420](https://github.com/CharmsDev/charms/tree/main/CHIPs/CHIP-0420) NFT that provides on-chain metadata for the eBTC token.

## Minting the Reference NFT

The NFT's `identity` is the hash of the UTXO ID of the 1st input of the minting transaction.

```sh
app_bin=$(charms app build)
export app_vk=$(charms app vk $app_bin)

# UTXO you're spending (1st input determines the NFT identity)
export in_utxo_0="<txid>:<vout>"

# compute app_id from the 1st input UTXO ID
export app_id=$(hashutxoid ${in_utxo_0})

# change output
export addr_0="<your_address>"
export dest_0=$(charms util dest --addr ${addr_0})
export amount_0=<change_sats>

cat ./spells/mint-nft.yaml | envsubst | charms spell check \
  --prev-txs=<prev_txs_hex> \
  --app-bins=${app_bin}
```

The spell in `spells/mint-nft.yaml` creates an NFT with the following CHIP-0420 fields:
- `ticker` — token ticker symbol
- `name` — display name
- `description` — token description
- `url` — project URL
- `image` — token logo URL
- `decimals` — decimal places
