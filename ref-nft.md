# eBTC Reference NFT

The eBTC reference NFT is a [CHIP-0420](https://github.com/CharmsDev/charms/tree/main/CHIPs/CHIP-0420) NFT that provides on-chain metadata for the eBTC token.

## Minting the Reference NFT

The NFT's `identity` is the hash of the UTXO ID of the 1st input of the minting transaction.

See the script in `./scripts/ref-nft.sh`

The spell in `spells/mint-nft.yaml` creates an NFT with the following CHIP-0420 fields:
- `ticker` — token ticker symbol
- `name` — display name
- `description` — token description
- `url` — project URL
- `image` — token logo URL
- `decimals` — decimal places
