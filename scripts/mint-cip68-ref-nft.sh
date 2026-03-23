#!/bin/bash
set -e

# Mint a CIP-68 reference NFT for the eBTC charm on Cardano.
#
# Usage: ./scripts/mint-cip68-ref-nft.sh
#
# Builds and signs the transaction with the wallet key.
# The Scrolls signer (ICP canister, VKey Hash 15bf560d...) must co-sign separately.

WALLET_DIR="/Users/ivan/src/sigma0-dev/charms/tmp/charms-inc-wallet"
WALLET_ADDR=$(cat "$WALLET_DIR/payment.addr")
PAYMENT_SKEY="$WALLET_DIR/payment.skey"

POLICY_ID="552b22f4989ea698fabbf6314b70d2e5edb49c1fdbdeb6096e8c84b6"
# CIP-68 reference token prefix (000643b0) + base name from FT asset name
REF_NFT_ASSET_NAME="000643b0d48144b4ec69fb794fbc2290ae63acf945fb035d5474648b50ee43b6"

# Reference input providing the Scrolls withdraw-0 validator as a reference script
REF_INPUT="94df3842c70e64d320bb918efb08b023f22c364707b29a4532efe8e5eafca09e#0"

# Version NFT token name at the reference input (used as the mint redeemer)
VERSION_NFT_NAME="000de140763131"

# Extra signatory required by the Scrolls withdraw validator
SCROLLS_SIGNER_VKEY_HASH="15bf560dabf4fe7f7ef78ac49c4fa846ebcde7009b1e886dd70d350d"

# Scrolls validator script hash (blake2b-224 of 0x03 || reference script CBOR)
SCROLLS_SCRIPT_HASH="29764648940a3b7208bc99a246bc96a69817bea017560972432f076f"

# Fully parameterized minting policy (main validator with version_nft_policy_id + app_vk applied)
MINT_POLICY_CBOR="59026b010100332229800aba2aba1aba0aab9faab9eaab9dab9a9bae0039bae0024888888889660033001300537540152259800800c5300103d87a80008992cc004006266e9520003300a300b0024bd7044cc00c00c0050091805800a010918049805000cdc3a400091111991194c004c02cdd5000cc03c01e44464b30013008300f37540031325980099b87323322330020020012259800800c00e2646644b30013372201400515980099b8f00a0028800c01901544cc014014c06c0110151bae3014001375a602a002602e00280a8c8c8cc004004dd59806980a1baa300d3014375400844b3001001801c4c8cc896600266e4403000a2b30013371e0180051001803202c899802802980e002202c375c602a0026eacc058004c0600050160a5eb7bdb180520004800a264b3001300a301137540031323322330020020012259800800c528456600266ebc00cc050c0600062946266004004603200280990161bab30163017301730173017301730173017301730173013375400a66e952004330143374a90011980a180a98091baa0014bd7025eb822c8080c050c054c054c054c044dd5180518089baa0018b201e301330103754003164038600a6eb0c020c03cdd5000cc03c00d222259800980400244ca600201d375c005004400c6eb8c04cc040dd5002c56600266e1d200200489919914c0040426eb801200c8028c050004c050c054004c040dd5002c5900e201c1807180780118068021801801a29344d959003130011e581c1775920b2f415d295553835fb7d26d8186cff73d352c9e9b98cad240004c01225820fd0cac892e457454be0212fa7d9a0e1517d5bd6a33aa7c66a1f10f55e375c2900001"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORK_DIR="$SCRIPT_DIR/../tmp/mint-cip68-ref-nft"
mkdir -p "$WORK_DIR"

echo "Work dir: $WORK_DIR"

# 1. Create minting policy script file
cat > "$WORK_DIR/mint-policy.plutus" <<EOF
{
  "type": "PlutusScriptV3",
  "description": "eBTC CIP-68 minting policy",
  "cborHex": "$MINT_POLICY_CBOR"
}
EOF

# 2. Generate the CIP-68 datum and other JSON files using Python
python3 - "$WORK_DIR" <<'PYEOF'
import json, sys

work_dir = sys.argv[1]

def to_hex(s):
    return s.encode('utf-8').hex()

def bytes_val(hex_str):
    return {"bytes": hex_str}

def chunk_bytes(s, max_bytes=64):
    """Split a UTF-8 string into chunks of at most max_bytes bytes."""
    encoded = s.encode('utf-8')
    chunks = []
    for i in range(0, len(encoded), max_bytes):
        chunks.append(encoded[i:i+max_bytes].hex())
    if len(chunks) == 1:
        return bytes_val(chunks[0])
    return {"list": [bytes_val(c) for c in chunks]}

# CIP-68 datum: constructor 0 with [metadata_map, version, extra]
metadata = [
    {"k": bytes_val(to_hex("name")),        "v": chunk_bytes("eBTC")},
    {"k": bytes_val(to_hex("description")), "v": chunk_bytes("Enchanted BTC: Bitcoin magically wrapped into a charm. Backed 1:1 by BTC held in a Scrolls smart contract vault.")},
    {"k": bytes_val(to_hex("ticker")),      "v": chunk_bytes("eBTC")},
    {"k": bytes_val(to_hex("url")),         "v": chunk_bytes("https://ebtc.charms.dev")},
    {"k": bytes_val(to_hex("decimals")),    "v": {"int": 8}},
    {"k": bytes_val(to_hex("logo")),        "v": chunk_bytes("ipfs://bafkreiamp6ocmalh4uu77jhcr2yh3i5ea4knet53qnmfuupmg45326u4wa")},
]

datum = {
    "constructor": 0,
    "fields": [
        {"map": metadata},
        {"int": 1},
        {"bytes": ""}
    ]
}

with open(f"{work_dir}/cip68-datum.json", "w") as f:
    json.dump(datum, f, indent=2)

# Mint redeemer: the version NFT token name as a ByteArray
mint_redeemer = {"bytes": "000de140763131"}
with open(f"{work_dir}/mint-redeemer.json", "w") as f:
    json.dump(mint_redeemer, f)

# Withdrawal redeemer: unit (constructor 0, no fields)
withdrawal_redeemer = {"constructor": 0, "fields": []}
with open(f"{work_dir}/withdrawal-redeemer.json", "w") as f:
    json.dump(withdrawal_redeemer, f)

print("Generated datum, mint redeemer, and withdrawal redeemer files.")
PYEOF

# 3. Compute the Scrolls stake (reward) address (bech32)
SCROLLS_STAKE_ADDR=$(python3 -c "
import hashlib

# bech32 encoding
CHARSET = 'qpzry9x8gf2tvdw0s3jn54khce6mua7l'

def bech32_polymod(values):
    GEN = [0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3]
    chk = 1
    for v in values:
        b = chk >> 25
        chk = ((chk & 0x1ffffff) << 5) ^ v
        for i in range(5):
            chk ^= GEN[i] if ((b >> i) & 1) else 0
    return chk

def bech32_hrp_expand(hrp):
    return [ord(x) >> 5 for x in hrp] + [0] + [ord(x) & 31 for x in hrp]

def bech32_create_checksum(hrp, data):
    values = bech32_hrp_expand(hrp) + data
    polymod = bech32_polymod(values + [0,0,0,0,0,0]) ^ 1
    return [(polymod >> 5 * (5 - i)) & 31 for i in range(6)]

def bech32_encode(hrp, data_bytes):
    # Convert bytes to 5-bit groups
    acc = 0
    bits = 0
    ret = []
    for b in data_bytes:
        acc = (acc << 8) | b
        bits += 8
        while bits >= 5:
            bits -= 5
            ret.append((acc >> bits) & 31)
    if bits > 0:
        ret.append((acc << (5 - bits)) & 31)
    checksum = bech32_create_checksum(hrp, ret)
    return hrp + '1' + ''.join([CHARSET[d] for d in ret + checksum])

# Reward script address: header 0xf1 (script hash, mainnet network id 1)
script_hash = bytes.fromhex('$SCROLLS_SCRIPT_HASH')
addr_bytes = bytes([0xf1]) + script_hash
print(bech32_encode('stake', addr_bytes))
")

echo "Scrolls stake address: $SCROLLS_STAKE_ADDR"

# 4. Query wallet UTxOs and pick an ADA-only one for input/collateral
echo "Querying wallet UTxOs..."
cardano-cli query utxo --address "$WALLET_ADDR" --output-json > "$WORK_DIR/utxos.json"

# Pick the first ADA-only UTxO with enough lovelace
INPUT_UTXO=$(python3 -c "
import json
d = json.load(open('$WORK_DIR/utxos.json'))
for k, v in d.items():
    val = v['value']
    if list(val.keys()) == ['lovelace'] and val['lovelace'] >= 5000000:
        print(k)
        break
")

if [ -z "$INPUT_UTXO" ]; then
    echo "ERROR: No suitable ADA-only UTxO found in wallet."
    exit 1
fi

echo "Using input UTxO: $INPUT_UTXO"

# 5. Build the transaction
echo "Building transaction..."
cardano-cli conway transaction build \
  --tx-in "$INPUT_UTXO" \
  --read-only-tx-in-reference "$REF_INPUT" \
  --mint "1 ${POLICY_ID}.${REF_NFT_ASSET_NAME}" \
  --mint-script-file "$WORK_DIR/mint-policy.plutus" \
  --mint-redeemer-file "$WORK_DIR/mint-redeemer.json" \
  --tx-out "${WALLET_ADDR}+2000000+1 ${POLICY_ID}.${REF_NFT_ASSET_NAME}" \
  --tx-out-inline-datum-file "$WORK_DIR/cip68-datum.json" \
  --withdrawal "${SCROLLS_STAKE_ADDR}+0" \
  --withdrawal-tx-in-reference "$REF_INPUT" \
  --withdrawal-plutus-script-v3 \
  --withdrawal-reference-tx-in-redeemer-file "$WORK_DIR/withdrawal-redeemer.json" \
  --required-signer-hash "$SCROLLS_SIGNER_VKEY_HASH" \
  --tx-in-collateral "$INPUT_UTXO" \
  --change-address "$WALLET_ADDR" \
  --out-file "$WORK_DIR/tx.unsigned"

echo "Transaction built: $WORK_DIR/tx.unsigned"

# 6. Sign with the wallet key
echo "Signing with wallet key..."
cardano-cli conway transaction sign \
  --tx-file "$WORK_DIR/tx.unsigned" \
  --signing-key-file "$PAYMENT_SKEY" \
  --out-file "$WORK_DIR/tx.wallet-signed"

TX_ID=$(cardano-cli conway transaction txid --tx-file "$WORK_DIR/tx.wallet-signed")
echo ""
echo "Transaction ID: $TX_ID"
echo "Reference NFT:  ${POLICY_ID}.${REF_NFT_ASSET_NAME}"
echo ""
echo "Wallet-signed tx: $WORK_DIR/tx.wallet-signed"
echo "The Scrolls signer (ICP canister) must co-sign before submission."
