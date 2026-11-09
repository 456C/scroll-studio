# Private publishing contract

Protocol id `scroll-studio.publishing.private-prototype.v1`.

## Breaking changes from the previous pin

The previous head returned cached command results, including credential secrets and `status: published`, on an idempotency-key replay. That behavior is gone.

- `auth.exchange` returns the credential secret once. The same idempotency key then returns `exchange_consumed` and no secret. A lost exchange response requires a new `auth.begin`.
- An idempotency key is not authorization. A retry without a live credential returns `unauthorized`. A revoked credential returns `credential_revoked`. An expired credential returns `credential_expired`. The stored result is omitted.
- `episode.upload`, `publication.confirm`, and `publication.withdraw` return `result.acknowledgement` with `authoritative: false`. They do not include current publication `status`. Current status comes from `account.discover` and `publication.read`.
- `fixture`, `loginHint`, and `testOnlyRedirect` are not network command fields. A request that contains `fixture` or `loginHint` is `invalid_request`. The begin result has no `testOnlyRedirect`.
- Comic bytes are accepted only after a bounded decode to pixels. A JPEG header check is not acceptance.

The desktop client sends one JSON command at a time. The shared files are the protocol, the envelope schema, and the fixtures. `prototypes/publishing/oracle.py` is the executable check for those files. It keeps state in memory. A later service has to accept the same fixtures. This checkpoint does not open a port.

## Files the desktop reads

`contracts/publishing/v1/protocol.json` holds limits, command names, error codes, the issuer, the redirect rule, and the client storage rule.

`contracts/publishing/v1/envelope.schema.json` holds the request and response shapes.

`prototypes/publishing/fixtures/world.json` holds two `TEST-ONLY` accounts and their series.

`prototypes/publishing/fixtures/templates/` holds the begin, exchange, and upload bodies.

`prototypes/publishing/fixtures/sequences/` holds the comic path, the prose path, and the negative paths.

`prototypes/publishing/consumer_example.py` is the sample client. It builds envelopes, checks storage before it keeps a credential, and runs a sequence.

Run the contract from the repository root.

```text
python3 -m unittest prototypes.publishing.test_contract
python3 -m prototypes.publishing.consumer_example
```

The second command prints `comic ok` and `prose ok`.

## Envelope

A request contains `protocol`, `command`, `idempotencyKey`, and `body`.

`credential` is required for `auth.revoke`, `account.discover`, `episode.upload`, `episode.preview`, `publication.confirm`, `publication.withdraw`, `publication.read`, and `operation.cancel`. The object has `credentialId` and `secret`. Both use the `cred_test_` and `sek_test_` prefixes in this protocol. There is no `Authorization` header and no credential in a URL.

The network envelope has no `fixture` object. Clock, pause, `loginHint`, and `testOnlyRedirect` belong to the oracle harness in `prototypes/publishing/fixture_harness.py`. A live listener uses `runtime_rules.server_now`, `runtime_rules.new_credential`, and `runtime_rules.new_authorization_code`. It must not call `FixtureHarness`.

A success response has `ok: true`, `command`, `idempotentReplay`, and `result`. A failure has `ok: false` and `error.code`. `error.retryable` is false. Resend the same key and the same body only while the credential is still live. The idempotency hash ignores `episode.upload` field `completion`.

The idempotency key is not authorization and does not cache a credential. `auth.exchange` returns the secret on the first success only.

## Auth

`auth.begin` body fields are `transportProfile`, `issuer`, `redirectUri`, `codeChallengeMethod`, `codeChallenge`, `state`, `clientId`, and `scope`.

The only enabled profile is `local-http-isolated-fixture`. `production-secure-transport` returns `production_transport_not_enabled`.

The issuer string is exactly `http://127.0.0.1:19427/ss-publish/test-issuer`. The issuer role is `prototype-test-issuer`. It is a fixture issuer, separate from any future production provider.

The redirect is `http`, host `127.0.0.1`, an explicit port, and path `/ss-publish/callback`. Ports `19427` and `19428` are reserved for the issuer and the reader. `localhost`, `::1`, `0.0.0.0`, userinfo, a query, and a fragment are rejected with `redirect_rejected`.

`codeChallengeMethod` is `S256`. `plain` returns `pkce_rejected`. `state` is 43 to 128 unreserved characters. The desktop generates it with a CSPRNG. Fixture files label their state `synthetic-fixture-not-csprng`.

`clientId` is `scroll-studio-desktop-test`. `scope` is `["prototype:creator"]`. The fixture harness selects `acct_test_owner_a` or `acct_test_owner_b` outside the command. The network body has no account picker.

The begin result has `authorizationUrl` and no authorization code. The authorization query keys are `response_type`, `client_id`, `redirect_uri`, `scope`, `state`, `code_challenge`, and `code_challenge_method`. The loopback query keys are `code` and `state`. The desktop opens `authorizationUrl` in the system browser and reads the redirect on its own `127.0.0.1` listener. The harness records that redirect for the contract tests. A listener does not put it in the begin result.

The authorization lifetime is 120 seconds. `auth.cancel` with the same `state`, `issuer`, and `redirectUri` makes a later exchange return `authorization_cancelled`.

`auth.exchange` sends `code`, `codeVerifier`, `redirectUri`, `state`, and `issuer` on the command channel. `grantType` defaults to `authorization_code`. Any other grant, including `bypass`, returns `unsupported_grant`. A second use of the code on a new idempotency key returns `code_replayed`. The same idempotency key returns `exchange_consumed` and no secret. A state, issuer, redirect, or PKCE mismatch burns the code.

The exchange result credential has `tokenType` `prototype-credential`, `delivery` `service-channel-only`, `testOnly` true, and `nonProductionLabel` `TEST-ONLY`. The secret appears in this result. The client keeps it in the OS credential store. If that store is unavailable, the client may keep it in process memory with `mode` `session-memory-only`, `label` `session-memory-only`, `persistence` `process-memory`, and `plaintextFallback` false. `plaintext-file` is refused. The client does not log the secret or the authorization code.

`auth.revoke` revokes one credential. A second credential for the same account keeps working. There is no refresh token and no recovery command. An expired or revoked credential starts again at `auth.begin`. The credential lifetime is 3600 seconds.

`account.discover` returns the account and the series that account owns, with episodes ordered by `ordinal`.

## Episode commands

`episode.upload` seals one selected episode into a private revision. `selection.mode` is `selected-episode-only`. `localStoryRef` and `localEpisodeRef` are labels matching `[A-Za-z0-9_-]{1,128}`. The body has no project, world, reference list, chapter list, or `.scro` bytes. The acknowledgement `localOriginals` is `untouched`. The acknowledgement `authoritative` is false.

`publicMetadata` allows `seriesTitle`, `episodeTitle`, `synopsis`, and `language`. The titles must match the owned catalog entry. `language` is two lowercase letters.

`content.kind` is `comic` or `prose`. Comic slices are ordered `image/png` or `image/jpeg` objects with `ordinal`, `width`, `height`, `byteLength`, `sha256`, and `bytesBase64`. Ordinals must be `1..n` in array order. Prose uses `mediaType` `text/plain; charset=utf-8` and a UTF-8 string. The desktop story model has no prose document. Prose is new in this protocol.

`completion` is `seal` or `defer`. `defer` leaves the operation `in_progress` and creates no revision. `operation.cancel` of that key cancels it. A later `seal` with the same key and the same body stays cancelled. A sealed upload, confirm, or withdraw returns `already_terminal` from `operation.cancel` and does not roll back. Withdrawal is a separate command.

`episode.preview` returns one sealed revision to its owner, including bytes. A newer upload does not change an older revision.

`publication.confirm` records a confirm for the current head revision. The body names `revisionId` and `policyEligibility`. The private value is `private-prototype-only`. `public` returns `public_launch_gate` and leaves the episode unpublished. An older revision returns `stale_revision`. The acknowledgement names `canonicalUrl` as `http://127.0.0.1:19428/ss-publish/reader/` plus `publicationId`, with `publicInternet` false and `authoritative` false. It does not report current publication status. A later retry of that confirm, after withdrawal, returns the same acknowledgement, and `publication.read` returns `withdrawn`.

`publication.read` is authoritative. It requires the exact canonical URL and a live credential. It returns `authoritative: true`, `status: published`, and `presentation.widthPx` 360. That width is the desktop Phone reader width in `src/app/reader_ui.rs`. A withdrawn publication returns `withdrawn`. A path under `/ss-publish/assets` or `/slices/` returns `asset_not_addressable`. An unknown reader URL returns `not_found`. Another account's publication returns `not_owner`.

`publication.withdraw` records a withdrawal. Its acknowledgement has `authoritative: false`. `account.discover` is the other authoritative status read. Confirming the same head revision again publishes that same URL.

## Limits

Comic caps copied from `src/export.rs` are 100 slices, 2,000,000 bytes for one slice, and 20,000,000 bytes for one episode. Prototype caps are 4096 by 4096 pixels and 4,000,000 pixels for one slice. PNG IDAT inflation stops at the image's expected byte count. Acceptance of PNG and JPEG also requires a bounded decoder to return a pixel buffer of the declared width and height. The checkpoint runs that decode with ffmpeg. A JPEG marker parse is not acceptance. The renderable fixtures are `prototypes/publishing/fixtures/media/north.png` and `north.jpg`, each 120 by 160. Prose stops at 65,536 UTF-8 bytes. A BOM, a UTF-16 media type, and C0 controls other than tab and line feed are `media_rejected`.

## Fixture identity

Accounts, credentials, codes, and verifiers in the fixtures are labeled `TEST-ONLY` or `synthetic-fixture-not-a-secret`. They are not production identity proof. The harness selects the fixture account. The oracle refuses an account whose `testOnly` flag is false. `FixtureHarness` mints `sek_test_` secrets from a counter. `runtime_rules.new_credential` uses a CSPRNG and is the function a listener must call.
