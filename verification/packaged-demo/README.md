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
| 4 | Archive SHA256 mismatch |
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

`contract` must be `scroll-studio.packaged-demo.manifest.v1`. Keys are exact. Commits are 40 lowercase hex characters. Archive hashes are 64 lowercase hex characters, one digest per archive path. Paths are absolute. `argv` is a list of strings and is passed with `shell` false. `argv[0]` must not be a shell. The landing URL is `http://127.0.0.1:<port>/` with no query. `routes.paths` lists owner-supplied absolute paths. A fetch, when the acceptance gate is open, records those paths as expectations and still parses links from the landing response. It does not crawl them as a creator script. Missing app digest, missing local archive paths, or a digest that does not match the named file is a rejection.

```json
{
  "contract": "scroll-studio.packaged-demo.manifest.v1",
  "synthetic": false,
  "provenance": {
    "app": {
      "commit": "<40 lowercase hex>",
      "archive": {"path": "<absolute local path>", "sha256": "<64 lowercase hex>"}
    },
    "service": {
      "commit": "<40 lowercase hex>",
      "archive": {"path": "<absolute local path>", "sha256": "<64 lowercase hex>"}
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

`fixtures/owner-expected-fragment.json` is not a manifest and was not measured. Passing it to the harness is a rejection. The two service digests below are different artifacts and are not interchangeable.

- App commit: `69d1f9b2a7e5839880d948e2e289125c76ae89cb`
- App hosted ZIP: artifact `13730379871` on run `42746459364`. SHA256 is UNKNOWN. Sanctioned local path is pending.
- Service commit: `91ba4894750d4e0d22a784c05b50ba89837451cf`
- Coordinator tar path: `/workspace/service-handoff-91ba489/scroll-studio-service-91ba489.tar.gz`
- Coordinator tar SHA256: `a3677623d821a294644006e1a377474e92b067a65a78b0501f4cbd7d271edec6`
- Service hosted ZIP: artifact `13731975216` on the same run. Claimed SHA256 `b61d17fb307320a229475fdaeb970e9b0d9a72282f77200ace2d25d591882873`. This unit did not measure it. Sanctioned local path is pending.
- Extracted working directory: archive root
- Argv: `python -m prototypes.publishing.listener`
- Landing: `http://127.0.0.1:19427/`
- Owner routes: `/ss-publish/demo`, `/ss-publish/demo/paper-harbor`, `/ss-publish/demo/paper-harbor/the-fold`, `/ss-publish/demo/paper-harbor/margin-note`

The owner reported Windows tests and browser navigation for run `42746459364`. That report did not include an EXE launch or screenshots. This unit did not run that job, did not download either artifact, and does not treat the report as its own validation.

Still required before any acceptance run:

- app archive SHA256
- sanctioned local paths for the app ZIP and the service hosted ZIP
- a complete manifest file
- Kaijin's sanctioned isolated hosted-Windows run

## Proof command

```text
python verification/packaged-demo/test_harness.py --evidence-dir verification/packaged-demo/evidence/synthetic
```

A passing unit run sets `unitTests` and leaves `packagedDemoAcceptance` as `BLOCKED`.
