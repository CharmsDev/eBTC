use anyhow::{Context, bail, ensure};
use charms_sdk::data::{
    App, B32, Data, NFT, SCROLL, TOKEN, Transaction, UtxoId, charm_values, sum_token_amount,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NftContent {
    pub ticker: String,
    pub name: String,
    pub description: String,
    pub url: String,
    pub image: String,
    pub decimals: u8,
}

#[derive(Clone, Copy, Debug)]
struct AppId {
    identity: Identity,
    vk: Vk,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Identity([u8; 32]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Vk([u8; 32]);

const DUST_SATS: u64 = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScrollSats(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct P2wpkh {
    hash160: [u8; 20],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputScript {
    Unbound,
    Bound(P2wpkh),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BoundScroll {
    sats: ScrollSats,
    script: P2wpkh,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OpenScroll {
    sats: ScrollSats,
    dest: OutputScript,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScrollLedger {
    spent: Vec<BoundScroll>,
    created: Vec<OpenScroll>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TokenDelta {
    incoming: u64,
    outgoing: u64,
}

impl AppId {
    fn token(&self) -> App {
        self.app(TOKEN)
    }

    fn scroll(&self) -> App {
        self.app(SCROLL)
    }

    #[cfg(test)]
    fn nft(&self) -> App {
        self.app(NFT)
    }

    fn app(&self, tag: char) -> App {
        App {
            tag,
            identity: B32(self.identity.0),
            vk: B32(self.vk.0),
        }
    }
}

impl ScrollSats {
    fn parse(amount: u64) -> anyhow::Result<Self> {
        ensure!(
            amount >= DUST_SATS,
            "scroll coin {amount} is below {DUST_SATS}"
        );
        Ok(Self(amount))
    }

    fn effective(self) -> u64 {
        self.0 - DUST_SATS
    }
}

impl P2wpkh {
    fn parse(dest: &[u8]) -> anyhow::Result<Self> {
        ensure!(dest.len() == 22, "script length {}", dest.len());
        ensure!(dest[0] == 0x00 && dest[1] == 0x14, "not a p2wpkh script");
        let mut hash160 = [0u8; 20];
        hash160.copy_from_slice(&dest[2..]);
        Ok(Self { hash160 })
    }

    #[cfg(test)]
    fn to_script(self) -> [u8; 22] {
        let mut script = [0u8; 22];
        script[0] = 0x00;
        script[1] = 0x14;
        script[2..].copy_from_slice(&self.hash160);
        script
    }
}

impl TokenDelta {
    fn delta(self) -> i128 {
        i128::from(self.outgoing) - i128::from(self.incoming)
    }
}

impl ScrollLedger {
    fn effective_delta(&self) -> i128 {
        let created: i128 = self
            .created
            .iter()
            .map(|coin| i128::from(coin.sats.effective()))
            .sum();
        let spent: i128 = self
            .spent
            .iter()
            .map(|coin| i128::from(coin.sats.effective()))
            .sum();
        created - spent
    }
}

pub fn app_contract(app: &App, tx: &Transaction, _x: &Data, _w: &Data) -> bool {
    match app_contract_impl(app, tx) {
        Ok(()) => true,
        Err(err) => {
            eprintln!("contract not satisfied: {err:#}");
            false
        }
    }
}

fn app_contract_impl(app: &App, tx: &Transaction) -> anyhow::Result<()> {
    match app.tag {
        NFT => nft_arm(app, tx),
        TOKEN | SCROLL => vault_checks(app, tx),
        _ => bail!("unsupported tag"),
    }
}

fn utxo_identity(utxo: &UtxoId) -> Identity {
    Identity(Sha256::digest(utxo.to_bytes()).into())
}

fn nft_arm(app: &App, tx: &Transaction) -> anyhow::Result<()> {
    let (utxo, _) = tx.ins.first().context("nft mint has no input")?;
    ensure!(utxo_identity(utxo).0 == app.identity.0, "identity mismatch");
    let nfts: Vec<_> = charm_values(app, tx.outs.iter()).collect();
    ensure!(nfts.len() == 1, "expected exactly one NFT output");
    nfts[0]
        .value::<NftContent>()
        .context("invalid NFT content")?;
    Ok(())
}

fn vault_checks(app: &App, tx: &Transaction) -> anyhow::Result<()> {
    let id = AppId {
        identity: Identity(app.identity.0),
        vk: Vk(app.vk.0),
    };
    let ledger = scroll_ledger(&id, tx)?;
    let tokens = token_delta(&id, tx)?;
    ensure!(
        tokens.delta() == ledger.effective_delta(),
        "token delta {} != scroll delta {}",
        tokens.delta(),
        ledger.effective_delta()
    );
    Ok(())
}

fn scroll_ledger(id: &AppId, tx: &Transaction) -> anyhow::Result<ScrollLedger> {
    let coin_ins = tx.coin_ins.as_ref().context("coin_ins required")?;
    let coin_outs = tx.coin_outs.as_ref().context("coin_outs required")?;
    ensure!(
        coin_ins.len() == tx.ins.len(),
        "coin_ins length {} != ins length {}",
        coin_ins.len(),
        tx.ins.len()
    );
    ensure!(
        coin_outs.len() == tx.outs.len(),
        "coin_outs length {} != outs length {}",
        coin_outs.len(),
        tx.outs.len()
    );

    let scroll = id.scroll();
    let mut spent = Vec::new();
    for (coin, (_, charms)) in coin_ins.iter().zip(tx.ins.iter()) {
        if !charms.contains_key(&scroll) {
            continue;
        }
        let script = match output_script(&coin.dest)? {
            OutputScript::Bound(script) => script,
            OutputScript::Unbound => bail!("spent scroll has an empty dest"),
        };
        spent.push(BoundScroll {
            sats: ScrollSats::parse(coin.amount)?,
            script,
        });
    }

    let mut created = Vec::new();
    for (coin, charms) in coin_outs.iter().zip(tx.outs.iter()) {
        if !charms.contains_key(&scroll) {
            continue;
        }
        created.push(OpenScroll {
            sats: ScrollSats::parse(coin.amount)?,
            dest: output_script(&coin.dest)?,
        });
    }

    Ok(ScrollLedger { spent, created })
}

fn output_script(dest: &[u8]) -> anyhow::Result<OutputScript> {
    if dest.is_empty() {
        return Ok(OutputScript::Unbound);
    }
    Ok(OutputScript::Bound(P2wpkh::parse(dest)?))
}

fn token_delta(id: &AppId, tx: &Transaction) -> anyhow::Result<TokenDelta> {
    let token = id.token();
    Ok(TokenDelta {
        incoming: sum_token_amount(&token, tx.ins.iter().map(|(_, charms)| charms))?,
        outgoing: sum_token_amount(&token, tx.outs.iter())?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use anyhow::{Context, ensure};
    use bitcoin::absolute::LockTime;
    use bitcoin::consensus::encode::serialize_hex;
    use bitcoin::hashes::Hash as _;
    use bitcoin::{
        Amount, OutPoint, ScriptBuf, Sequence, Transaction as BitcoinTransaction, TxIn, TxOut, Txid,
        Witness,
    };
    use charms_client::bitcoin_tx::{BitcoinTx, SCROLLS_ADDRS_PUBKEY};
    use charms_client::tx::{EnchantedTx as _, Tx};
    use charms_client::{NormalizedSpell, SignedScrollOutputs, utxo_id_hash};
    use charms_data::AppInput;
    use charms_sdk::data::{AppSignature, B32, NativeOutput, TxId};
    use secp256k1::schnorr::Signature;
    use secp256k1::{Message, Secp256k1, XOnlyPublicKey};
    use serde::Deserialize;
    use sha2::{Digest, Sha256};
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::str::FromStr;

    const MINT_OUTPOINT: &str =
        "eb7c000000000000000000000000000000000000000000000000000000000001:0";
    const MINT_IDENTITY: &str = "74b556e5b324e10c519c5f0f0f0ee8c43ffab84c9295477cbe99f0e4a9b47cea";

    const V1_VK: [u8; 32] = [
        0xfd, 0x0c, 0xac, 0x89, 0x2e, 0x45, 0x74, 0x54, 0xbe, 0x02, 0x12, 0xfa, 0x7d, 0x9a, 0x0e,
        0x15, 0x17, 0xd5, 0xbd, 0x6a, 0x33, 0xaa, 0x7c, 0x66, 0xa1, 0xf1, 0x0f, 0x55, 0xe3, 0x75,
        0xc2, 0x90,
    ];
    const V1_IDENTITY: [u8; 32] = [
        0x07, 0x96, 0xf6, 0x3e, 0xd4, 0x81, 0x44, 0xb4, 0xec, 0x69, 0xfb, 0x79, 0x4f, 0xbc, 0x22,
        0x90, 0xae, 0x63, 0xac, 0xf9, 0x45, 0xfb, 0x03, 0x5d, 0x54, 0x74, 0x64, 0x8b, 0x50, 0xee,
        0x43, 0xb6,
    ];
    const V1_SCRIPT: [u8; 22] = [
        0x00, 0x14, 0x1c, 0xcb, 0xe7, 0xf8, 0xb1, 0xe3, 0x64, 0xfe, 0x23, 0x11, 0x0c, 0x9f, 0x72,
        0x0c, 0x2b, 0x5c, 0x2a, 0x37, 0x52, 0x7e,
    ];

    struct Example {
        spent: Vec<(RawDest, u64)>,
        created: Vec<(RawDest, u64)>,
        token_in: u64,
        token_out: u64,
    }

    #[derive(Clone, Debug)]
    enum RawDest {
        Empty,
        Script(P2wpkh),
        Bytes(Vec<u8>),
    }

    #[derive(Debug, Deserialize)]
    struct ScrollFixture {
        tx_in_0: String,
        output_index: u32,
        script_pubkeys: BTreeMap<u32, String>,
        signature: String,
    }

    fn open_canister_script(fixture_yaml: &str) -> anyhow::Result<P2wpkh> {
        let fixture: ScrollFixture = serde_yaml::from_str(fixture_yaml).context("fixture yaml")?;
        let keys: Vec<u32> = fixture.script_pubkeys.keys().copied().collect();
        ensure!(
            keys == vec![fixture.output_index],
            "script map keys {keys:?} != [{}]",
            fixture.output_index
        );
        UtxoId::from_str(&fixture.tx_in_0).context("tx_in_0")?;
        let script_hex = fixture
            .script_pubkeys
            .get(&fixture.output_index)
            .context("missing script")?;
        let message = charms_sdk::data::util::write(&fixture.script_pubkeys)
            .context("encoding script map")?;
        let digest: [u8; 32] = Sha256::digest(&message).into();
        let signature = hex::decode(&fixture.signature).context("signature hex")?;
        verify_bip340(&SCROLLS_ADDRS_PUBKEY, digest, &signature)?;
        let script = hex::decode(script_hex).context("script hex")?;
        P2wpkh::parse(&script).context("script")
    }

    fn verify_bip340(pk: &[u8; 32], digest: [u8; 32], sig: &[u8]) -> anyhow::Result<()> {
        let pk = XOnlyPublicKey::from_slice(pk).context("public key")?;
        let signature = Signature::from_slice(sig).context("signature")?;
        let message = Message::from_digest(digest);
        Secp256k1::verification_only()
            .verify_schnorr(&signature, &message, &pk)
            .context("BIP-340")
    }

    fn example_tx(id: &AppId, example: &Example) -> Transaction {
        let mut ins = Vec::new();
        let mut coin_ins = Vec::new();
        let mut outs = Vec::new();
        let mut coin_outs = Vec::new();
        let mut n = 0u32;

        for (dest, amount) in &example.spent {
            ins.push((utxo(n), scroll_charm(id)));
            coin_ins.push(coin(*amount, dest));
            n += 1;
        }
        if example.token_in != 0 {
            ins.push((utxo(n), token_charm(id, example.token_in)));
            coin_ins.push(coin(0, &RawDest::Empty));
        }

        for (dest, amount) in &example.created {
            outs.push(scroll_charm(id));
            coin_outs.push(coin(*amount, dest));
        }
        if example.token_out != 0 {
            outs.push(token_charm(id, example.token_out));
            coin_outs.push(coin(0, &RawDest::Empty));
        }

        Transaction {
            ins,
            refs: Vec::new(),
            outs,
            coin_ins: Some(coin_ins),
            coin_outs: Some(coin_outs),
            prev_txs: BTreeMap::new(),
            app_public_inputs: BTreeMap::new(),
        }
    }

    fn scroll_charm(id: &AppId) -> BTreeMap<App, Data> {
        BTreeMap::from([(id.scroll(), Data::empty())])
    }

    fn token_charm(id: &AppId, amount: u64) -> BTreeMap<App, Data> {
        BTreeMap::from([(id.token(), Data::from(&amount))])
    }

    fn coin(amount: u64, dest: &RawDest) -> NativeOutput {
        NativeOutput {
            amount,
            dest: dest.bytes(),
            content: None,
        }
    }

    fn utxo(index: u32) -> UtxoId {
        UtxoId(TxId([index as u8; 32]), index)
    }

    impl RawDest {
        fn bytes(&self) -> Vec<u8> {
            match self {
                RawDest::Empty => Vec::new(),
                RawDest::Script(script) => script.to_script().to_vec(),
                RawDest::Bytes(bytes) => bytes.clone(),
            }
        }
    }

    fn nft_mint_tx(id: &AppId, first_input: &UtxoId) -> Transaction {
        let content = NftContent {
            ticker: "eBTC".to_string(),
            name: "eBTC".to_string(),
            description: "Enchanted BTC. Bitcoin wrapped into a charm. Backing satoshis sit in Scrolls outputs.".to_string(),
            url: "https://ebtc.charms.dev".to_string(),
            image: "ipfs://bafkreiamp6ocmalh4uu77jhcr2yh3i5ea4knet53qnmfuupmg45326u4wa".to_string(),
            decimals: 8,
        };
        Transaction {
            ins: vec![(first_input.clone(), BTreeMap::new())],
            refs: Vec::new(),
            outs: vec![BTreeMap::from([(id.nft(), Data::from(&content))])],
            coin_ins: Some(vec![coin(0, &RawDest::Empty)]),
            coin_outs: Some(vec![coin(0, &RawDest::Empty)]),
            prev_txs: BTreeMap::new(),
            app_public_inputs: BTreeMap::new(),
        }
    }

    fn sample_id() -> AppId {
        AppId {
            identity: Identity([7; 32]),
            vk: Vk([9; 32]),
        }
    }

    fn published_app_id() -> AppId {
        let bytes = hex::decode(MINT_IDENTITY).unwrap();
        AppId {
            identity: Identity(bytes.try_into().unwrap()),
            vk: Vk([9; 32]),
        }
    }

    fn published_outpoint() -> UtxoId {
        UtxoId::from_str(MINT_OUTPOINT).unwrap()
    }

    fn empty() -> Data {
        Data::empty()
    }

    #[test]
    fn mint_on_a_signed_script_satisfies_token_and_scroll() {
        let id = sample_id();
        let script_a = open_canister_script(include_str!("../tests/fixtures/lock-a.yaml")).unwrap();
        let lock = example_tx(
            &id,
            &Example {
                spent: vec![],
                created: vec![(RawDest::Script(script_a), 10_000)],
                token_in: 0,
                token_out: 9_700,
            },
        );
        assert!(app_contract(&id.token(), &lock, &empty(), &empty()));
        assert!(app_contract(&id.scroll(), &lock, &empty(), &empty()));
    }

    #[test]
    fn preflight_empty_dest_satisfies_the_scroll_arm() {
        let id = sample_id();
        let preflight = example_tx(
            &id,
            &Example {
                spent: vec![],
                created: vec![(RawDest::Empty, 10_000)],
                token_in: 0,
                token_out: 9_700,
            },
        );
        assert!(app_contract(&id.scroll(), &preflight, &empty(), &empty()));
    }

    #[test]
    fn burn_and_relock_accepts_two_different_signed_scripts() {
        let id = sample_id();
        let script_a = open_canister_script(include_str!("../tests/fixtures/lock-a.yaml")).unwrap();
        let script_b = open_canister_script(include_str!("../tests/fixtures/lock-b.yaml")).unwrap();
        assert_ne!(script_a.to_script(), script_b.to_script());
        let relock = example_tx(
            &id,
            &Example {
                spent: vec![(RawDest::Script(script_a), 10_000)],
                created: vec![(RawDest::Script(script_b), 5_000)],
                token_in: 9_700,
                token_out: 4_700,
            },
        );
        assert!(app_contract(&id.token(), &relock, &empty(), &empty()));
        assert!(app_contract(&id.scroll(), &relock, &empty(), &empty()));
    }

    #[test]
    fn full_release_burns_the_effective_sats() {
        let id = sample_id();
        let script_a = open_canister_script(include_str!("../tests/fixtures/lock-a.yaml")).unwrap();
        let release = example_tx(
            &id,
            &Example {
                spent: vec![(RawDest::Script(script_a), 10_000)],
                created: vec![],
                token_in: 9_700,
                token_out: 0,
            },
        );
        assert!(app_contract(&id.scroll(), &release, &empty(), &empty()));
    }

    #[test]
    fn release_without_a_token_burn_fails() {
        let id = sample_id();
        let script_a = open_canister_script(include_str!("../tests/fixtures/lock-a.yaml")).unwrap();
        let unbacked = example_tx(
            &id,
            &Example {
                spent: vec![(RawDest::Script(script_a), 10_000)],
                created: vec![],
                token_in: 0,
                token_out: 0,
            },
        );
        assert!(!app_contract(&id.scroll(), &unbacked, &empty(), &empty()));
    }

    #[test]
    fn mint_without_a_scroll_output_fails() {
        let id = sample_id();
        let free_mint = example_tx(
            &id,
            &Example {
                spent: vec![],
                created: vec![],
                token_in: 0,
                token_out: 9_700,
            },
        );
        assert!(!app_contract(&id.token(), &free_mint, &empty(), &empty()));
    }

    #[test]
    fn empty_spent_dest_fails() {
        let id = sample_id();
        let tx = example_tx(
            &id,
            &Example {
                spent: vec![(RawDest::Empty, 10_000)],
                created: vec![],
                token_in: 9_700,
                token_out: 0,
            },
        );
        assert!(!app_contract(&id.scroll(), &tx, &empty(), &empty()));
    }

    #[test]
    fn other_script_fails() {
        let id = sample_id();
        let tx = example_tx(
            &id,
            &Example {
                spent: vec![],
                created: vec![(RawDest::Bytes(vec![0x51]), 10_000)],
                token_in: 0,
                token_out: 9_700,
            },
        );
        assert!(!app_contract(&id.scroll(), &tx, &empty(), &empty()));
    }

    #[test]
    fn below_dust_fails() {
        let id = sample_id();
        let tx = example_tx(
            &id,
            &Example {
                spent: vec![],
                created: vec![(RawDest::Empty, 299)],
                token_in: 0,
                token_out: 0,
            },
        );
        assert!(!app_contract(&id.scroll(), &tx, &empty(), &empty()));
    }

    #[test]
    fn tampered_signature_does_not_open() {
        let yaml = include_str!("../tests/fixtures/lock-a.yaml");
        let fixture: ScrollFixture = serde_yaml::from_str(yaml).unwrap();
        let mut sig = hex::decode(&fixture.signature).unwrap();
        sig[0] ^= 0xff;
        let tampered = yaml.replace(&fixture.signature, &hex::encode(sig));
        assert!(open_canister_script(&tampered).is_err());
    }

    #[test]
    fn nft_mint_follows_the_first_input() {
        let published = published_app_id();
        let outpoint = published_outpoint();
        assert_eq!(utxo_identity(&outpoint).0, published.identity.0);
        assert_eq!(utxo_id_hash(&outpoint).0, published.identity.0);
        assert!(app_contract(
            &published.nft(),
            &nft_mint_tx(&published, &outpoint),
            &empty(),
            &empty()
        ));

        let v1_outpoint =
            UtxoId::from_str("22ac8af70480f31287b9e3a9884e88af32dee0f365ff184385f71ccece93b3df:0")
                .unwrap();
        assert!(!app_contract(
            &published.nft(),
            &nft_mint_tx(&published, &v1_outpoint),
            &empty(),
            &empty()
        ));
    }

    #[test]
    fn published_record_agrees() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let repo = root.parent().unwrap();
        let identity_txt = fs::read_to_string(root.join("identity.txt")).unwrap();
        let mut lines = identity_txt.lines();
        assert_eq!(lines.next().unwrap(), format!("outpoint {MINT_OUTPOINT}"));
        assert_eq!(lines.next().unwrap(), format!("identity {MINT_IDENTITY}"));
        assert_eq!(
            lines.next().unwrap(),
            "note test commitment, not a bitcoin output"
        );
        assert!(lines.next().is_none());

        let outpoint = published_outpoint();
        let identity = utxo_identity(&outpoint);
        assert_eq!(hex::encode(identity.0), MINT_IDENTITY);
        assert_eq!(utxo_id_hash(&outpoint).0, identity.0);

        let public_key = hex::decode(
            fs::read_to_string(root.join("vk/public-key.hex"))
                .unwrap()
                .trim(),
        )
        .unwrap();
        let public_key: [u8; 32] = public_key.try_into().unwrap();
        let vk: [u8; 32] = Sha256::digest(public_key).into();
        assert_eq!(
            fs::read_to_string(root.join("vk/vk.hex")).unwrap().trim(),
            hex::encode(vk)
        );

        let wasm = fs::read(root.join("wasm/ebtc_v2.wasm")).unwrap();
        let wasm_hash: [u8; 32] = Sha256::digest(&wasm).into();
        assert_ne!(wasm_hash, vk);
        let signatures: BTreeMap<B32, AppSignature> = serde_yaml::from_str(
            &fs::read_to_string(root.join("wasm/ebtc_v2.wasm.sig.yaml")).unwrap(),
        )
        .unwrap();
        assert_eq!(signatures.len(), 1);
        let (signed_vk, app_sig) = signatures.iter().next().unwrap();
        assert_eq!(signed_vk.0, vk);
        assert_eq!(app_sig.public_key.0, public_key);
        verify_bip340(&public_key, wasm_hash, &app_sig.signature).unwrap();

        let vk_hex = hex::encode(vk);
        for readme in [repo.join("README.md"), root.join("README.md")] {
            let text = fs::read_to_string(&readme).unwrap();
            assert!(text.contains(MINT_IDENTITY), "{}", readme.display());
            assert!(text.contains(&vk_hex), "{}", readme.display());
        }
    }

    #[test]
    fn v1_wasm_hash_is_unchanged() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../wasm/ebtc.wasm");
        let bytes = fs::read(&path).unwrap();
        let digest: [u8; 32] = Sha256::digest(&bytes).into();
        assert_eq!(digest, V1_VK);
    }

    #[test]
    fn v2_sources_omit_v1_vault_constants() {
        let needles = [
            hex::encode(V1_VK),
            hex::encode(V1_IDENTITY),
            hex::encode(V1_SCRIPT),
        ];
        let mut files = Vec::new();
        walk_files(Path::new(env!("CARGO_MANIFEST_DIR")), &mut files);
        for path in files {
            let text = String::from_utf8_lossy(&fs::read(&path).unwrap()).to_ascii_lowercase();
            for needle in &needles {
                assert!(
                    !text.contains(needle),
                    "{} contains {needle}",
                    path.display()
                );
            }
        }
    }

    fn funding_input() -> (Tx, UtxoId) {
        let funding = BitcoinTransaction {
            version: bitcoin::transaction::Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint::new(Txid::from_byte_array([9; 32]), 0),
                script_sig: ScriptBuf::new(),
                sequence: Sequence::MAX,
                witness: Witness::new(),
            }],
            output: vec![TxOut {
                value: Amount::from_sat(20_000),
                script_pubkey: ScriptBuf::new(),
            }],
        };
        let wrapped = BitcoinTx::from_hex(&serialize_hex(&funding)).unwrap();
        let utxo = UtxoId(wrapped.tx_id(), 0);
        (Tx::Bitcoin(wrapped), utxo)
    }

    fn rendered_spell(template: &str, pairs: &[(&str, String)]) -> NormalizedSpell {
        let mut yaml = template.to_string();
        for (key, value) in pairs {
            yaml = yaml.replace(&format!("${{{key}}}"), value);
        }
        serde_yaml::from_str(&yaml).unwrap()
    }

    fn published_vk() -> String {
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("vk/vk.hex"))
            .unwrap()
            .trim()
            .to_string()
    }

    fn wasm_bytes() -> Vec<u8> {
        fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("wasm/ebtc_v2.wasm")).unwrap()
    }

    fn app_input() -> AppInput {
        let wasm = wasm_bytes();
        let wasm_hash: [u8; 32] = Sha256::digest(&wasm).into();
        let signatures: BTreeMap<B32, AppSignature> = serde_yaml::from_str(
            &fs::read_to_string(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("wasm/ebtc_v2.wasm.sig.yaml"),
            )
            .unwrap(),
        )
        .unwrap();
        AppInput {
            app_binaries: BTreeMap::from([(B32(wasm_hash), wasm)]),
            app_private_inputs: BTreeMap::new(),
            app_signatures: signatures,
        }
    }

    fn lock_spell(input: &UtxoId) -> NormalizedSpell {
        let wasm_hash = hex::encode(Sha256::digest(wasm_bytes()));
        rendered_spell(
            include_str!("../spells/lock.yaml"),
            &[
                ("in_utxo_0", input.to_string()),
                ("mint_amount", "9700".to_string()),
                ("vault_amount", "10000".to_string()),
                ("token_output_sats", "546".to_string()),
                (
                    "change_dest",
                    "00140000000000000000000000000000000000000000".to_string(),
                ),
                ("app_id", MINT_IDENTITY.to_string()),
                ("app_vk", published_vk()),
                ("wasm_hash", wasm_hash),
            ],
        )
    }

    fn fixture_scrolls(yaml: &str) -> (P2wpkh, SignedScrollOutputs) {
        let script = open_canister_script(yaml).unwrap();
        let fixture: ScrollFixture = serde_yaml::from_str(yaml).unwrap();
        (
            script,
            SignedScrollOutputs {
                script_pubkeys: fixture.script_pubkeys,
                signature: fixture.signature,
            },
        )
    }

    #[test]
    fn nft_spell_lists_a_coin() {
        let spell = rendered_spell(
            include_str!("../spells/mint-nft.yaml"),
            &[
                ("in_utxo_0", MINT_OUTPOINT.to_string()),
                ("nft_amount", "546".to_string()),
                (
                    "nft_dest",
                    "00140000000000000000000000000000000000000000".to_string(),
                ),
                ("app_id", MINT_IDENTITY.to_string()),
                ("app_vk", published_vk()),
                ("wasm_hash", hex::encode([7u8; 32])),
            ],
        );
        let coins = spell.tx.coins.expect("coins");
        assert_eq!(coins.len(), spell.tx.outs.len());
        assert!(coins[0].content.is_none());
        assert!(!coins[0].dest.is_empty());
    }

    #[test]
    fn protocol_accepts_the_lock_spell_and_rejects_a_user_script() {
        let (prev, input) = funding_input();
        let (script, signed) = fixture_scrolls(include_str!("../tests/fixtures/lock-a.yaml"));
        let mut spell = lock_spell(&input);
        assert!(spell.tx.coins.as_ref().unwrap()[1].dest.is_empty());
        let preflight = charms_client::is_correct(
            &spell,
            &vec![prev.clone()],
            Some(app_input()),
            &[0u8; 32],
            &BTreeMap::new(),
            None,
        )
        .unwrap();
        assert!(!preflight);

        spell.tx.coins.as_mut().unwrap()[1].dest = script.to_script().to_vec();
        let accepted = charms_client::is_correct(
            &spell,
            &vec![prev.clone()],
            Some(app_input()),
            &[0u8; 32],
            &BTreeMap::new(),
            Some(&signed),
        )
        .unwrap();
        assert!(accepted);

        let user_script = [0x00, 0x14].into_iter().chain([0x11; 20]).collect();
        spell.tx.coins.as_mut().unwrap()[1].dest = user_script;
        let rejected = charms_client::is_correct(
            &spell,
            &vec![prev],
            Some(app_input()),
            &[0u8; 32],
            &BTreeMap::new(),
            Some(&signed),
        )
        .unwrap_err();
        assert!(
            rejected.to_string().contains("does not match"),
            "{rejected:#}"
        );
    }

    fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            if name == "target" || name == ".git" {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                walk_files(&path, out);
            } else {
                out.push(path);
            }
        }
    }
}
