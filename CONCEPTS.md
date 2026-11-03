# Concepts

Shared domain vocabulary for this project — entities, named processes, and status concepts with project-specific meaning. Seeded with core domain vocabulary, then accretes as ce-compound and ce-compound-refresh process learnings; direct edits are fine. Glossary only, not a spec or catch-all.

## App

An app identified by a tag, an identity, and a verification key. The same identity and key with different tags are different apps.

The host runs an app only when a spell lists it. A listing is required when that app sits on a spent input, and when the spell creates an output whose charm names that app. An app that sits only on an existing output the spell leaves untouched can be left off the spell, and then it does not run.

## Scroll

Bitcoin locked by this app on an output whose script is supplied by the Scrolls protocol. The locked value compared with the token change excludes a dust reserve on each coin.

A newly created scroll may omit its script until the prover fills it. A spent scroll must present a pay-to-witness-public-key-hash script.

## Token

Fungible units of this app. Creating or destroying units must match the change in locked scroll value.

The published lock spell places the token and the scroll on separate outputs. Conservation does not require that split. One output may carry both tags, and each tag is still its own app.

## Relationships

A scroll and a token that share an identity and a verification key must change by the same amount. Spending one does not make the host run the other.

## Flagged ambiguities

"Vault" had been used for the v1 lock, which is a fixed address. The v2 lock is a scroll.
