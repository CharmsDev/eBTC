---
title: Run the conservation check on both the scroll tag and the token tag
date: 2026-11-03
category: architecture-patterns
module: ebtc_v2
problem_type: architecture_pattern
component: service_layer
severity: critical
root_cause: missing_validation
resolution_type: code_fix
applies_when:
  - A spell spends the scroll output and omits the token app
  - The balance check is placed on the token tag and a separate check is placed on the lock tag
  - The v2 app contract tag dispatch is being changed
  - A spent lock output can be the only reason the host runs this app
symptoms:
  - A scroll-only spend releases the locked bitcoin with no token burn when the conservation check runs only on the token tag
tags:
  - ebtc-v2
  - charms
  - conservation
  - scroll-tag
  - token-tag
  - vault-checks
  - app-listing
  - locked-bitcoin
---

# Run the conservation check on both the scroll tag and the token tag

## Context

eBTC v1 keeps the balance check on the token tag and the vault script check on the vault tag. The Charms host runs a listed app. On the path that supplies binaries, an unchanged simple transfer with empty inputs skips the contract. A scroll app is not a simple transfer, so that skip does not apply. The host requires the listing for an app on a spent input. A spell can spend one tag's coin and leave the other coin unspent. Pull request 2 is merged. The merged v2 contract is the current tree.

## Guidance

Send both tags through the conservation check. `app_contract_impl` matches `TOKEN` and `SCROLL` to `vault_checks` (`v2/src/lib.rs:161`). `vault_checks` builds one `AppId` from the running app's identity and verification key (`v2/src/lib.rs:182`). It then loads the scroll ledger and the token delta (`v2/src/lib.rs:186`) and requires the deltas to be equal (`v2/src/lib.rs:188`).

`scroll_ledger` records each coin that carries the scroll app (`v2/src/lib.rs:213`). `token_delta` sums amounts that carry the token app (`v2/src/lib.rs:251`). The tag that entered `vault_checks` still produces both lookups. `AppId::token` and `AppId::scroll` differ by tag (`v2/src/lib.rs:71`, `v2/src/lib.rs:75`). An `App` is a tag, an identity, and a verification key (`/tmp/charms/charms-data/src/lib.rs:223`). Those two tags are two apps.

The host list is `app_public_inputs`. `apps` returns that map's keys (`/tmp/charms/charms-client/src/lib.rs:340`). On the path that supplies app binaries, `is_correct` passes the same map to `apps_satisfied` (`/tmp/charms/charms-client/src/lib.rs:536`). `run_all` iterates that map (`/tmp/charms/charms-app-runner/src/lib.rs:561`). An app absent from the map is outside that iteration.

`check_input_apps_are_referenced` walks spent inputs and rejects a charm whose app is missing from the map (`/tmp/charms/charms-client/src/lib.rs:669`). `is_correct` calls it (`/tmp/charms/charms-client/src/lib.rs:487`). A coin the spell leaves unspent is outside that walk, so the host does not require its app.

v1 splits the work by tag. `app_contract_impl` sends `TOKEN` to `can_mint_or_burn_token` and the vault tag to `vault_contract_satisfied` (`src/lib.rs:39`, `src/lib.rs:40`). The balance comparison is the token arm (`src/lib.rs:84`). The vault arm checks the vault script on vault-tagged coins and then returns (`src/lib.rs:114`, `src/lib.rs:134`). Spending the vault coin runs the vault arm. Leaving the token coin unspent does not require the token app, so a spell that also omits it leaves the balance check unrun. That split does not transfer.

## Why This Matters

A spell can spend the scroll coin, leave the token coin unspent, and omit the token app. `check_input_apps_are_referenced` does not demand the token app for that spell. `run_all` does not execute it. The scroll app is on the spent input, so the host does require the scroll app, and the scroll arm is the conservation check that runs.

The other direction is a token output with no scroll coin. Output charms are indices into `app_public_inputs` (`/tmp/charms/charms-client/src/lib.rs:133`). `well_formed` requires each output index to fall inside that map (`/tmp/charms/charms-client/src/lib.rs:290`). Creating a token output lists the token app, and the spell can still omit the scroll app when no scroll coin is spent. The token arm is the conservation check that runs.

Either arm has to reject an unbalanced pair. A check that stays on one tag accepts the spell that spends the other tag.

## When to Apply

- Two apps share an identity and a verification key and differ by tag, and one invariant covers both.
- A spell can spend one of those apps and leave the other unspent.
- The host executes `app_public_inputs` and forces a listing for apps on spent inputs.
- A second tag is added beside a conservation check that already lives on one tag.

## Examples

v1 reaches the balance check from the token tag (`src/lib.rs:37`):

```rust
match app.tag {
    NFT => can_mint_nft(app, tx),
    TOKEN => can_mint_or_burn_token(app, tx),
    VAULT => vault_contract_satisfied(app, tx),
    _ => unreachable!(),
}
```

`can_mint_or_burn_token` is the arm that requires the token delta to equal the vault delta (`src/lib.rs:84`). `vault_contract_satisfied` ends after the script checks (`src/lib.rs:134`).

v2 uses one arm for both tags (`v2/src/lib.rs:159`):

```rust
match app.tag {
    NFT => nft_arm(app, tx),
    TOKEN | SCROLL => vault_checks(app, tx),
    _ => bail!("unsupported tag"),
}
```

`vault_checks` always compares both sides (`v2/src/lib.rs:186`):

```rust
let ledger = scroll_ledger(&id, tx)?;
let tokens = token_delta(&id, tx)?;
ensure!(
    tokens.delta() == ledger.effective_delta(),
    "token delta {} != scroll delta {}",
    tokens.delta(),
    ledger.effective_delta()
);
```

`release_without_a_token_burn_fails` spends a scroll-tagged coin and sets both token amounts to zero (`v2/src/lib.rs:534`). `example_tx` attaches the scroll app to every spent coin in that list (`v2/src/lib.rs:364`) and adds a token input when `token_in` is non-zero (`v2/src/lib.rs:368`). The test calls the scroll app and expects failure (`v2/src/lib.rs:546`).

`mint_without_a_scroll_output_fails` has a token output and no scroll output (`v2/src/lib.rs:550`). The test calls the token app and expects failure (`v2/src/lib.rs:561`).

## Related

No earlier learning in this repo covers the same check. The contract this note describes landed in [pull request 2](https://github.com/CharmsDev/eBTC/pull/2).
