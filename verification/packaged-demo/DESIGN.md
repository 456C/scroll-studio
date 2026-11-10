# Packaged-demo verification design

Read-only grounding, then a lane-local design. This directory does not replace owner packaging, startup, or the hosted-Windows procedure.

## Interfaces observed, not taken over

- `scripts/windows-demo.ps1` starts `python -m prototypes.publishing.listener` with the extracted service tree as the working directory, probes `127.0.0.1:19427`, then `Stop-Process` on the process object it created. This harness does not edit or call that script.
- `tests/windows_browser_publish.py` drives the owner browser and creator commands against that listener. This harness does not import it, duplicate its route script, or run the service unittest list.
- `src/publish_wire.rs` is the desktop loopback client (`127.0.0.1`, port `19427`). This harness does not link to it.
- `docs/windows-hosted-runner.md` is the owner's manual procedure. Pins in that document are not inputs to this harness.

Owner-supplied expected values are stored only in `fixtures/owner-expected-fragment.json`. They are not a manifest and were not measured here. No coordinator file was opened, and no local path was invented for a missing archive.

## Sketch A — manifest gate only

A single validator reads JSON, requires full commit SHAs and archive hashes, and writes one verdict file. It never binds a port or creates a process.

- Caller keeps using the owner script for start and stop.
- The gate cannot kill the wrong PID because it kills nothing.
- It also cannot record occupied-port refusal or owned-process cleanup, and it leaves lifecycle policy in scripts this lane must not edit.

## Sketch B — copy of the hosted demo supervisor

Reimplement extract, the service unittest list, the HTTP readiness GET, the browser creator flow, and `Stop-Process -Id` inside this directory.

- It resembles the current hosted script.
- It duplicates owner tests and the creator/browser script, deletes or overwrites an extract directory, and stops a PID without a later identity check.
- That is a second integration path, which this lane is not allowed to become.

## Synthesis

Keep the strict gate from sketch A, and add a narrow process supervisor that sketch A lacks, without sketch B's extract, unittest list, or browser driver.

The supervisor:

- accepts an argv list and passes it to `CreateProcess` / `exec` with `shell=False`
- refuses shell interpreters as `argv[0]`
- refuses to unpack archives
- probes a loopback port with a TCP connect and, if the connect succeeds, does not spawn and does not signal the occupant
- records a durable handle for a child it spawned (Linux pidfd; Windows process handle plus creation time)
- signals only through that handle, and only after the process identity still matches the snapshot taken at spawn
- writes four evidence files that do not embed one another: HTTP, process/listener, browser-render, owned-process cleanup

Browser pixels stay a separate evidence file. This unit does not launch a browser and cannot mark that file captured. HTTP is not treated as browser-render proof.

`routes.paths` is the owner-supplied route list. Natural discovery still parses the landing response when a fetch is allowed. The supplied paths are expectations, not a crawl and not evidence that those URLs were served. Each component declares distinct `wrapper` and `inner` byte identities. `validated.bytes` selects which identity the path's bytes are checked against. The sibling identity is stored on the evidence record and is not hashed.

`run()` hashes and validates first. It then refuses launch unless the manifest is non-synthetic, `environment.sanctioned` is true, `environment.kind` is `isolated-hosted-windows`, and the process platform is `win32`. On this cloud Linux host the gate stays closed, so the real listener argv is not executed. Synthetic tests call the port and cleanup primitives directly and label the records `synthetic`.

The checked-in fragment records the owner-reported wrapper and inner sizes and digests for both artifacts, plus the earlier coordinator tar as a separate service identity. It is not a manifest. No local path is invented for the inner archives, the coordinator path is not treated as a file on this VM, and none of those digests were measured here. The owner Windows run is an owner report, not validation by this unit.

Exit 0 is reserved for a future accepted run that includes a real browser capture. This unit does not return 0.

## Caller interface

From the repository:

```text
python verification/packaged-demo/harness.py --manifest MANIFEST.json --evidence-dir EVIDENCE_DIR
```

From a copy of only this directory:

```text
python harness.py --manifest MANIFEST.json --evidence-dir EVIDENCE_DIR
```

`EVIDENCE_DIR` must be missing or empty. The command prints one JSON object to stdout and writes exactly:

- `http.json`
- `process-listener.json`
- `browser-render.json`
- `owned-process-cleanup.json`

Unit and negative proof, which is not packaged-demo acceptance:

```text
python verification/packaged-demo/test_harness.py --evidence-dir verification/packaged-demo/evidence/synthetic
```

Kaijin reviews this directory before any consumption. Nothing here is wired into `scripts/`, workflows, or the owner browser test.
