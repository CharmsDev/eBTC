use charms_client::utxo_id_hash;
use charms_sdk::data::{charm_values, check, sum_token_amount, App, Data, Transaction, NFT, TOKEN};
use serde::{Deserialize, Serialize};

/// Vault contract tag.
const VAULT: char = 'c';

/// Dust amount in satoshis: minimum BTC per vault UTXO, not counted toward eBTC.
const DUST: u64 = 300;

/// scriptPubKey for the vault address: bc1qrn970793udj0ugc3pj0hyrptts4rw5n7qxeya2
const VAULT_SPK: [u8; 22] = [
    0x00, 0x14, 0x1c, 0xcb, 0xe7, 0xf8, 0xb1, 0xe3, 0x64, 0xfe, 0x23, 0x11, 0x0c, 0x9f, 0x72,
    0x0c, 0x2b, 0x5c, 0x2a, 0x37, 0x52, 0x7e,
];

/// CHIP-0420 reference NFT metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftContent {
    pub ticker: String,
    pub name: String,
    pub description: String,
    pub url: String,
    pub image: String,
    pub decimals: u8,
}

pub fn app_contract(app: &App, tx: &Transaction, _x: &Data, _w: &Data) -> bool {
    match app.tag {
        NFT => check!(can_mint_nft(app, tx)),
        TOKEN => check!(can_mint_or_burn_token(app, tx)),
        VAULT => check!(vault_contract_satisfied(app, tx)),
        _ => unreachable!(),
    }
    true
}

/// NFT can be minted if identity == hash(1st input UTXO ID) and exactly one valid NFT output.
fn can_mint_nft(app: &App, tx: &Transaction) -> bool {
    let (utxo_id, _) = &tx.ins[0];
    check!(utxo_id_hash(utxo_id) == app.identity);

    let nft_charms = charm_values(app, tx.outs.iter()).collect::<Vec<_>>();
    check!(nft_charms.len() == 1);
    check!(nft_charms[0].value::<NftContent>().is_ok());
    true
}

/// eBTC is minted/burned based on BTC locked/spent at the vault address.
///
/// Balance equation:
///   token_out - token_in == (vault_out - n_vault_outs * DUST) - (vault_in - n_vault_ins * DUST)
fn can_mint_or_burn_token(app: &App, tx: &Transaction) -> bool {
    let coin_ins = tx.coin_ins.as_ref().expect("coin_ins required");
    let coin_outs = tx.coin_outs.as_ref().expect("coin_outs required");

    let (vault_in, n_vault_ins) = vault_btc_total(coin_ins);
    let (vault_out, n_vault_outs) = vault_btc_total(coin_outs);

    let effective_in = vault_in - n_vault_ins * DUST;
    let effective_out = vault_out - n_vault_outs * DUST;

    let token_in = sum_token_amount(app, tx.ins.iter().map(|(_, v)| v)).unwrap_or(0);
    let token_out = sum_token_amount(app, tx.outs.iter()).unwrap_or(0);

    // Use wrapping arithmetic to handle both mint (positive delta) and burn (negative delta).
    check!(token_out.wrapping_sub(token_in) == effective_out.wrapping_sub(effective_in));
    true
}

/// Sum BTC amounts at the vault address and count the number of vault UTXOs.
fn vault_btc_total(coins: &[charms_sdk::data::NativeOutput]) -> (u64, u64) {
    let mut total = 0u64;
    let mut count = 0u64;
    for coin in coins {
        if coin.dest.as_slice() == VAULT_SPK {
            assert!(coin.amount >= DUST, "vault UTXO amount must be >= DUST");
            total += coin.amount;
            count += 1;
        }
    }
    (total, count)
}

/// Vault contract: every input and output carrying this charm must be at the vault address.
fn vault_contract_satisfied(app: &App, tx: &Transaction) -> bool {
    let coin_ins = tx.coin_ins.as_ref().expect("coin_ins required");
    let coin_outs = tx.coin_outs.as_ref().expect("coin_outs required");

    for (i, (_, charms)) in tx.ins.iter().enumerate() {
        if charms.contains_key(app) {
            check!(coin_ins[i].dest.as_slice() == VAULT_SPK);
        }
    }
    for (i, charms) in tx.outs.iter().enumerate() {
        if charms.contains_key(app) {
            check!(coin_outs[i].dest.as_slice() == VAULT_SPK);
        }
    }
    true
}

#[cfg(test)]
mod test {
    #[test]
    fn dummy() {}
}
