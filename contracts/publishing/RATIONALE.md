# Private publishing shape

## Problem

Scroll Studio is a local Rust editor. A story has chapters, assets, and layers in a `.scro` file. `src/export.rs` writes PNG or JPEG slices for WEBTOON and Tapas and stops there. The app has a Phone reader at 360 logical pixels and no account, upload, or publication type. Prose is absent from `Story` in `src/model.rs`.

`docs/hosting-research.md` names a future platform and says none of its accounts or uploads are deployed. `website/src/studio/runtime.php` is a temporary browser editor. The publishing work cannot sit in that PHP process, and it cannot change the desktop crate, the root lockfiles, or the live site.

`contracts/publishing/` and `prototypes/publishing/` were empty. Those two trees can hold a shared contract without touching the editor.

## Usage

The desktop client selects one episode and sends JSON commands. It opens the system browser for an authorization code with PKCE S256, listens on `127.0.0.1` and an ephemeral port, and exchanges the code on the command channel. It stores the returned credential in the OS credential store, or in process memory when that store is missing. It uploads the selected comic slices or UTF-8 prose, previews that revision id, confirms that same id, reads the canonical URL at the phone width, and withdraws.

The sample client is `prototypes/publishing/consumer_example.py`. The bodies it sends live in `prototypes/publishing/fixtures/templates/`.

## Shape

One command envelope and an append-only revision list. The publication record is a small state machine with `draft`, `published`, and `withdrawn`. Preview reads any sealed revision. Confirm names the current head. Read returns the published revision. Withdraw changes the publication and leaves the revision bytes in place.

Auth, discovery, upload, preview, confirm, read, withdraw, and cancel are commands on that envelope. The browser sees two URLs, the authorize URL and the loopback redirect. Those URLs carry the code challenge, the state, and later the code. They do not carry a credential. Asset bytes move inside an authenticated command result. The canonical reader URL is an opaque id, and a path under `/ss-publish/assets` or `/slices/` is `asset_not_addressable`.

Validation sits on the envelope, the redirect, the issuer, and the episode bytes. The revision list stores the checked content. Idempotency records the terminal result of a mutating command so a retry converges.

## Synthesis decision

Two shapes were compared against the same constraints.

The command catalog is the base. It keeps one request shape, one error object, and no asset URL template. The desktop can implement it as a single JSON codec.

The resource-shaped alternative was a set of HTTP paths for series, episodes, revisions, and slice files. It lost because a slice path is something a client can guess. The part kept from that shape is the pair of browser URLs, because the system browser has to open a page and return to a loopback port.

Reusing `website/src/studio/runtime.php` lost because that process is a temporary editor with its own session files. The hosting-research vendors lost for this checkpoint because they need accounts, spend, and a deployment, and none of that is authorized here.

## Tradeoffs accepted

The oracle clock is the fixture clock, or `2026-11-09T18:00:00Z` when a request omits one. A later service uses its own clock. Tests stay repeatable.

Fixture ids and secrets are deterministic and prefixed `test`. A production credential would be a new protocol id.

`testOnlyRedirect` lets the oracle see the browser redirect without binding a port. The desktop listener is still the real callback.

`completion` is outside the idempotency hash so `defer` and a later `seal` are one operation. A cancelled key stays cancelled.

A request that fails the schema does not consume the idempotency key. A valid domain error does.

JPEG checks the markers and the dimensions. It does not expand the compressed scan. PNG IDAT inflation stops at the expected image size.

The fixture world already contains each owner's series. Creating a series is a later command.

There is no refresh token. A lost credential starts at `auth.begin`. Account recovery stays with a future production provider.

`policyEligibility: public` is a closed gate. This protocol has no terms checkbox.

GitHub Actions runs `cargo test` for the desktop crate. These Python tests stay out of that workflow so the root CI files stay unchanged. Run the unittest command in `CONSUMER.md`.

## Open questions

Should the desktop be able to create a series, or will the prototype keep seeding series for each fixture account?

The OS credential store is desktop work. This contract states the allowed storage records and refuses a plaintext file.

Operator report, hold, and remove controls belong to the later private service, along with a loopback listener that passes these sequences.

## Next implementation step

Stop here for review. The next build is a loopback service that runs these sequences, after the desktop owner accepts the command shapes.
