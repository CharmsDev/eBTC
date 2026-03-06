use anyhow::{ensure, Context};
use charms_client::utxo_id_hash;
use charms_sdk::data::{
    charm_values, sum_token_amount, App, Charms, Data, NativeOutput, Transaction, NFT, TOKEN,
};
use hex_literal::hex;
use serde::{Deserialize, Serialize};

/// Vault contract tag.
const VAULT: char = 'c';

/// Dust amount in satoshis: minimum BTC per vault UTXO, not counted toward eBTC.
const DUST: u64 = 300;

/// scriptPubKey for the vault address: bc1qrn970793udj0ugc3pj0hyrptts4rw5n7qxeya2
const VAULT_DEST: [u8; 22] = hex!("00141ccbe7f8b1e364fe23110c9f720c2b5c2a37527e");

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
    app_contract_impl(app, tx)
        .context("contract not satisfied")
        .unwrap();
    true
}

pub fn app_contract_impl(app: &App, tx: &Transaction) -> anyhow::Result<()> {
    match app.tag {
        NFT => can_mint_nft(app, tx),
        TOKEN => can_mint_or_burn_token(app, tx),
        VAULT => vault_contract_satisfied(app, tx),
        _ => unreachable!(),
    }
}

/// NFT can be minted if identity == hash(1st input UTXO ID) and exactly one valid NFT output.
fn can_mint_nft(app: &App, tx: &Transaction) -> anyhow::Result<()> {
    let (utxo_id, _) = &tx.ins[0];
    ensure!(utxo_id_hash(utxo_id) == app.identity, "identity mismatch");

    let nft_charms = charm_values(app, tx.outs.iter()).collect::<Vec<_>>();
    ensure!(nft_charms.len() == 1, "expected exactly one NFT output");
    nft_charms[0]
        .value::<NftContent>()
        .context("invalid NFT content")?;
    Ok(())
}

/// eBTC is minted/burned based on BTC locked/spent at the vault address.
///
/// Balance equation:
///   token_out - token_in == (vault_out - n_vault_outs * DUST) - (vault_in - n_vault_ins * DUST)
fn can_mint_or_burn_token(app: &App, tx: &Transaction) -> anyhow::Result<()> {
    let vault_app = App {
        tag: VAULT,
        identity: app.identity.clone(),
        vk: app.vk.clone(),
    };

    let coin_ins = tx.coin_ins.as_ref().context("coin_ins required")?;
    let coin_outs = tx.coin_outs.as_ref().context("coin_outs required")?;

    let in_charms = tx.ins.iter().map(|(_, c)| c);
    let (vault_in, n_vault_ins) = vault_btc_total(&vault_app, coin_ins, in_charms)?;
    let (vault_out, n_vault_outs) = vault_btc_total(&vault_app, coin_outs, tx.outs.iter())?;

    let effective_in = vault_in - n_vault_ins * DUST;
    let effective_out = vault_out - n_vault_outs * DUST;

    let token_in = sum_token_amount(app, tx.ins.iter().map(|(_, v)| v))?;
    let token_out = sum_token_amount(app, tx.outs.iter())?;

    let token_delta = token_out as i128 - token_in as i128;
    let vault_delta = effective_out as i128 - effective_in as i128;
    ensure!(
        token_delta == vault_delta,
        "token balance mismatch: token delta ({token_delta}) != vault delta ({vault_delta})"
    );
    Ok(())
}

/// Sum BTC amounts for outputs carrying the vault contract charm.
fn vault_btc_total<'a>(
    vault_app: &App,
    coins: &[NativeOutput],
    charms_outputs: impl Iterator<Item = &'a Charms>,
) -> anyhow::Result<(u64, u64)> {
    let mut total = 0u64;
    let mut count = 0u64;
    for (coin, charms) in coins.iter().zip(charms_outputs) {
        if charms.contains_key(vault_app) {
            ensure!(
                coin.amount >= DUST,
                "vault UTXO amount {} < DUST",
                coin.amount
            );
            total += coin.amount;
            count += 1;
        }
    }
    Ok((total, count))
}

/// Vault contract: every input and output carrying this charm must be at the vault address.
fn vault_contract_satisfied(app: &App, tx: &Transaction) -> anyhow::Result<()> {
    let coin_ins = tx.coin_ins.as_ref().context("coin_ins required")?;
    let coin_outs = tx.coin_outs.as_ref().context("coin_outs required")?;

    for (i, (_, charms)) in tx.ins.iter().enumerate() {
        if charms.contains_key(app) {
            ensure!(
                coin_ins[i].dest.as_slice() == VAULT_DEST,
                "input {i} with vault charm not at vault address"
            );
        }
    }
    for (i, charms) in tx.outs.iter().enumerate() {
        if charms.contains_key(app) {
            ensure!(
                coin_outs[i].dest.as_slice() == VAULT_DEST,
                "output {i} with vault charm not at vault address"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use std::str::FromStr;

    /// eBTC vault Scrolls address: generated with nonce
    /// `1129595493` (`eBTC` in UTF-8 as LE byte-order integer)
    const EBTC_VAULT_ADDR: &str = "bc1qrn970793udj0ugc3pj0hyrptts4rw5n7qxeya2";

    #[test]
    fn test_vault_dest() {
        let address = bitcoin::Address::from_str(EBTC_VAULT_ADDR)
            .expect("valid address")
            .require_network(bitcoin::Network::Bitcoin)
            .expect("mainnet address");
        assert_eq!(address.script_pubkey().as_bytes(), VAULT_DEST);
    }
}
