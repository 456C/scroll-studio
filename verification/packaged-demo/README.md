# Packaged-demo verification

Independent support harness. It is not wired into owner startup, packaging, workflows, or `scripts/windows-demo.ps1`. Kaijin reviews this directory before using it. He is the laptop and hosted-Windows operator.

This unit validates an owner-supplied manifest, checks local archive bytes, and can supervise one child process. It does not download, extract, or publish. Cloud Linux runs recorded here are synthetic negative and lifecycle checks. They are not packaged-demo proof.

## Invocation

From the repository:

```text
python verification/packaged-demo/harness.py --manifest MANIFEST.json --evidence-dir EVIDENCE_DIR
```

From a copy of only this directory:

```text
python harness.py --manifest MANIFEST.json --evidence-dir EVIDENCE_DIR
```

`EVIDENCE_DIR` must be absent or empty. The process prints one JSON object. `packagedDemoProved` is false. Exit 0 is not used.

| Exit | Meaning |
| --- | --- |
| 2 | Malformed manifest |
| 3 | Provenance missing, artifact missing, or unsafe path |
| 4 | Selected wrapper or inner bytes differ in size or SHA256. A short read also lands here, including a Windows text-mode stop at byte 0x1A. |
| 5 | Loopback port occupied; occupant was not signaled |
| 6 | Acceptance blocked, including every run of this unit on Linux |
| 7 | Evidence path already had files; nothing was written |
| 8 | Owned-process cleanup could not prove termination |

The command writes only these files in the evidence directory:

- `http.json`
- `process-listener.json`
- `browser-render.json`
- `owned-process-cleanup.json`

## Manifest

`contract` must be `scroll-studio.packaged-demo.manifest.v1`. Keys are exact. Commits are 40 lowercase hex characters. Each of app and service declares two distinct byte identities, `wrapper` and `inner`, each with a size and a SHA256. `validated.bytes` is `wrapper` or `inner`. `validated.path` is the absolute path of the file whose bytes are hashed. The other identity is recorded and is not hashed, and it is not a fallback. Paths are absolute. `argv` is a list of strings and is passed with `shell` false. `argv[0]` must not be a shell. The landing URL is `http://127.0.0.1:<port>/` with no query. `routes.paths` lists owner-supplied absolute paths. A fetch, when the acceptance gate is open, records those paths as expectations and still parses links from the landing response. It does not crawl them as a creator script. A missing path, a missing digest, or bytes that differ from the selected identity is a rejection.

```json
{
  "contract": "scroll-studio.packaged-demo.manifest.v1",
  "synthetic": false,
  "provenance": {
    "app": {
      "commit": "<40 lowercase hex>",
      "wrapper": {"size": "<wrapper byte length>", "sha256": "<wrapper sha256>"},
      "inner": {"size": "<inner byte length>", "sha256": "<inner sha256>"},
      "validated": {"bytes": "wrapper", "path": "<absolute local path of the selected bytes>"}
    },
    "service": {
      "commit": "<40 lowercase hex>",
      "wrapper": {"size": "<wrapper byte length>", "sha256": "<wrapper sha256>"},
      "inner": {"size": "<inner byte length>", "sha256": "<inner sha256>"},
      "validated": {"bytes": "inner", "path": "<absolute local path of the selected bytes>"}
    }
  },
  "lifecycle": {
    "workingDirectory": "<absolute existing directory>",
    "argv": ["python", "-m", "prototypes.publishing.listener"],
    "loopback": {"host": "127.0.0.1", "port": 19427}
  },
  "routes": {
    "landingUrl": "http://127.0.0.1:19427/",
    "discovery": "natural-links-from-landing",
    "paths": ["/<owner-supplied-path>"]
  },
  "environment": {
    "kind": "isolated-hosted-windows",
    "sanctioned": false
  }
}
```

`environment.sanctioned` stays false until Kaijin is running on the sanctioned isolated hosted-Windows machine. A synthetic manifest cannot set `sanctioned` to true. Launch stays closed unless the manifest is non-synthetic, sanctioned, kind `isolated-hosted-windows`, and the process platform is `win32`. Browser capture is still blocked in this unit, so a Windows launch would also exit non-zero. That Windows path was not executed here.

Kaijin can review this directory by itself. From that directory, using a new empty temp directory for evidence:

```text
python harness.py --manifest MANIFEST.json --evidence-dir EVIDENCE_DIR
```

Do not point `EVIDENCE_DIR` at a directory that already holds files. The harness does not integrate into owner workflows.

## Owner-supplied expected fragment

`fixtures/owner-expected-fragment.json` is not a manifest and was not measured. Passing it to the harness is a rejection. Wrapper bytes, inner bytes, and the earlier coordinator tar are different identities. None is a substitute for another. The frozen pair is app `69d1f9b2a7e5839880d948e2e289125c76ae89cb` and service `91ba4894750d4e0d22a784c05b50ba89837451cf`. A docs commit beginning `6815aeb` is not either pin.

- App artifact `13730379871` wrapper: 5760452 bytes, SHA256 `09d55a859c6e8bc8603c934b672d8ba07458aae8c8a4f7c880c031e65729e4d1`. The owner reported that file on the coordinator shared box, not on this VM: `/cursor/stores/user/grok-bot/agents/5dbc5fbf-04bd-4c97-8dfd-c0e764b5e800/assets/01M71PWKV4XN5T0YVA63PK6E21.zip`, with sibling manifest `01M71PWKV44Z3AQ9S3RK7QDM7N.txt`.
- App inner portable ZIP: 5760240 bytes, SHA256 `33889c6d99fba1599b23a9e0ea07d5b1eb1275a216ee2ad419da811a9861d337`. No local path was supplied.
- Service artifact `13731975216` wrapper: 13232992 bytes, SHA256 `c0e638ddceda13d4540002882ccaf416b304deac66e809f822346dd653dc9c89`. No local path was supplied.
- Service inner ZIP: 13260377 bytes, SHA256 `b61d17fb307320a229475fdaeb970e9b0d9a72282f77200ace2d25d591882873`. No local path was supplied.
- Coordinator tar, a separate service file: `/workspace/service-handoff-91ba489/scroll-studio-service-91ba489.tar.gz`, SHA256 `a3677623d821a294644006e1a377474e92b067a65a78b0501f4cbd7d271edec6`.
- Extracted working directory: archive root. Argv: `python -m prototypes.publishing.listener`. Landing: `http://127.0.0.1:19427/`.
- Owner routes: `/ss-publish/demo`, `/ss-publish/demo/paper-harbor`, `/ss-publish/demo/paper-harbor/the-fold`, `/ss-publish/demo/paper-harbor/margin-note`.

The owner reported Windows tests and browser navigation for run `42746459364`. That report did not include an EXE launch or screenshots. This unit did not run that job, did not open the coordinator files, and does not treat the report as its own validation.

Still required before any acceptance run:

- a local path for the exact wrapper or inner file Kaijin wants hashed
- a complete manifest whose `validated.bytes` names that layer
- Kaijin's sanctioned isolated hosted-Windows run

## Proof command

```text
python verification/packaged-demo/test_harness.py --evidence-dir verification/packaged-demo/evidence/synthetic
```

A passing unit run sets `unitTests` and leaves `packagedDemoAcceptance` as `BLOCKED`.
