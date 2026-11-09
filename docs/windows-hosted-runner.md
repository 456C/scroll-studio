# Hosted Windows runner for the private publishing pin

Verified pair, not a physical Windows desktop and not the Linux window:

- Desktop commit `72c0ed65c671fab099a7203a74f43695a9288685`
- Service pin `464766d97087cb26a12de6e009323951c43e1596`
- Run https://github.com/456C/scroll-studio/actions/runs/42722190076
- Portable zip `scroll-studio-0.13.2-72c0ed6-windows-portable` (5760452 bytes)
  https://github.com/456C/scroll-studio/actions/runs/42722190076/artifacts/13720217517

The Linux publish window was the listener at `e3eb462a63b86f793cdf9de5a37a07090ccbc8b7`. It was not this pin.

## Python

The `windows-latest` runner already had Python. The log recorded:

```
3.12.10 (tags/v3.12.10:0cc8128, Apr  8 2025, 12:21:36) [MSC v.1943 64 bit (AMD64)]
C:\hostedtoolcache\windows\Python\3.12.10\x64\python.exe
```

No Python package was installed.

## FFmpeg

Source: the 7.1 essentials zip published with the `GyanD/codexffmpeg` release `7.1` (the Windows builds documented at https://www.gyan.dev/ffmpeg/builds/).

```
https://github.com/GyanD/codexffmpeg/releases/download/7.1/ffmpeg-7.1-essentials_build.zip
```

SHA-256, verified on that run:

```
fa7d4d7e795db0e2503f49f105f46ed5852386f0cfdd819899be3b65ebde24fc
```

The workflow downloads that zip, checks the hash, expands it, and prepends the directory that contains `ffmpeg.exe` to `PATH`. The log then showed:

```
ffmpeg version 7.1-essentials_build-www.gyan.dev
ffprobe version 7.1-essentials_build-www.gyan.dev
png decoder: True
mjpeg decoder: True
```

## Service tree

The app checkout stays at the desktop commit. The service pin is a second checkout into `service-464766d`, using the workflow's built-in read-only `GITHUB_TOKEN`. `persist-credentials: false` removes that token from the git config after checkout. Nothing in `contracts/publishing` or `prototypes/publishing` is copied into the app tree or edited.

```
actions/checkout@v4
  ref: 464766d97087cb26a12de6e009323951c43e1596
  path: service-464766d
  fetch-depth: 1
  persist-credentials: false
```

The run recorded the working directory:

```
D:\a\scroll-studio\scroll-studio\service-464766d
```

From that directory the selected tests were:

```
python -m unittest prototypes.publishing.test_contract prototypes.publishing.test_listener.ListenerTests.test_bind_refuses_non_loopback prototypes.publishing.test_listener.ListenerTests.test_publish_preview_reader_and_pixels prototypes.publishing.test_listener.ListenerTests.test_prose_unicode_preview_and_byte_cap prototypes.publishing.test_listener.ListenerTests.test_replay_revoke_expiry_cancel_and_recovery prototypes.publishing.test_listener.ListenerTests.test_browser_csrf_host_origin_and_withdrawal prototypes.publishing.test_listener.ListenerTests.test_operator_report_hold_remove prototypes.publishing.test_listener.ListenerTests.test_body_cap_closes_before_the_payload prototypes.publishing.test_listener.ListenerTests.test_hold_and_remove_do_not_yield_to_the_owner prototypes.publishing.test_listener.ListenerTests.test_sign_in_csrf_expires_once_and_matches_next prototypes.publishing.test_listener.ListenerTests.test_rejected_connections_close prototypes.publishing.test_listener.ListenerTests.test_busy_admission_does_not_follow_a_trickle
```

Result on that run: `Ran 30 tests in 9.540s` / `OK`.

These two tests were not run. Omitting them is not Windows browser proof:

```
prototypes.publishing.test_listener.ListenerTests.test_chrome_sign_in_sends_service_origin
prototypes.publishing.test_listener.ListenerTests.test_phone_viewport_screenshot
```

## Listener and hosted browser

From `service-464766d`:

```
python -m prototypes.publishing.listener
```

The process listens on `127.0.0.1:19427` only. The runner then executed `tests/windows_browser_publish.py` from the app checkout. The log recorded the browser path and these lines:

```
browser C:\Program Files\Google\Chrome\Application\chrome.exe
opened fixture authorize page
affirm reached the callback with code and state only
preview showed the uploaded paragraph
reader showed the published paragraph
reader showed unavailable after withdraw
```

That is headless Chrome on the hosted runner. It is not the Linux egui window and not a physical Windows desktop.

## App launch

The portable artifact contains `Scroll Studio.exe`. The hosted job built and packaged that file. It did not start the egui window.
