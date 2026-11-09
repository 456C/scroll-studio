# Manual demo on a Windows machine

This is the local procedure. It is not a physical Windows desktop test of the egui window. The hosted runner executed `scripts/windows-demo.ps1` and did not start `Scroll Studio.exe`.

The pair these commands were written for:

- Desktop sources that produced the passing hosted run: `72c0ed65c671fab099a7203a74f43695a9288685`
- Service pin: `464766d97087cb26a12de6e009323951c43e1596`

A later documentation commit does not move the Linux window onto that service pin. The Linux window that showed comic withdrawal used listener `e3eb462a63b86f793cdf9de5a37a07090ccbc8b7`.

## Identity and limits

Accounts are the synthetic TEST ONLY fixtures (`acct_test_owner_a`, `test-owner-a`, series Fixture Lane, episodes North door and South stair). The listener keeps publications, sessions, and credentials in process memory. Stopping it drops them. It binds only `127.0.0.1:19427`. Do not publish that port and do not put a credential secret on a browser URL.

## Python

The hosted runner already had Python and did not download it:

```
3.12.10 (tags/v3.12.10:0cc8128, Apr  8 2025, 12:21:36) [MSC v.1943 64 bit (AMD64)]
C:\hostedtoolcache\windows\Python\3.12.10\x64\python.exe
```

On another machine, install Python 3.12 so that `python` is on `PATH`, then run:

```powershell
python -c "import sys; print(sys.version); print(sys.executable)"
```

## FFmpeg

Download the 7.1 essentials zip from the `GyanD/codexffmpeg` release (the Windows builds described at https://www.gyan.dev/ffmpeg/builds/):

```powershell
$url = 'https://github.com/GyanD/codexffmpeg/releases/download/7.1/ffmpeg-7.1-essentials_build.zip'
$zip = Join-Path $env:TEMP 'ffmpeg-7.1-essentials_build.zip'
Invoke-WebRequest -Uri $url -OutFile $zip
$hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $zip).Hash.ToLowerInvariant()
if ($hash -ne 'fa7d4d7e795db0e2503f49f105f46ed5852386f0cfdd819899be3b65ebde24fc') { throw "checksum $hash" }
Expand-Archive -LiteralPath $zip -DestinationPath (Join-Path $env:TEMP 'ffmpeg-7.1') -Force
$bin = (Get-ChildItem -LiteralPath (Join-Path $env:TEMP 'ffmpeg-7.1') -Recurse -Filter ffmpeg.exe | Select-Object -First 1).DirectoryName
$env:Path = "$bin;$env:Path"
ffmpeg -version
ffprobe -version
ffmpeg -hide_banner -decoders
```

The hosted run logged `ffmpeg version 7.1-essentials_build-www.gyan.dev`, png decoder True, and mjpeg decoder True.

## Service archive

The workflow builds `scroll-studio-service-464766d.zip` from the pin checkout and uploads it as an Actions artifact of that name. Download that artifact from the run page while signed in to GitHub. The guide does not contain a token.

```powershell
Expand-Archive -LiteralPath scroll-studio-service-464766d.zip -DestinationPath C:\scroll-demo -Force
Set-Location C:\scroll-demo
python -m unittest prototypes.publishing.test_contract prototypes.publishing.test_listener.ListenerTests.test_bind_refuses_non_loopback prototypes.publishing.test_listener.ListenerTests.test_publish_preview_reader_and_pixels prototypes.publishing.test_listener.ListenerTests.test_prose_unicode_preview_and_byte_cap prototypes.publishing.test_listener.ListenerTests.test_replay_revoke_expiry_cancel_and_recovery prototypes.publishing.test_listener.ListenerTests.test_browser_csrf_host_origin_and_withdrawal prototypes.publishing.test_listener.ListenerTests.test_operator_report_hold_remove prototypes.publishing.test_listener.ListenerTests.test_body_cap_closes_before_the_payload prototypes.publishing.test_listener.ListenerTests.test_hold_and_remove_do_not_yield_to_the_owner prototypes.publishing.test_listener.ListenerTests.test_sign_in_csrf_expires_once_and_matches_next prototypes.publishing.test_listener.ListenerTests.test_rejected_connections_close prototypes.publishing.test_listener.ListenerTests.test_busy_admission_does_not_follow_a_trickle
python -m prototypes.publishing.listener
```

`scripts/windows-demo.ps1` is the same extract, test, start, browser, and stop sequence. The hosted job runs that file. Stop the listener by ending that process. State is memory only.

These two tests are Linux Chrome tests and are not part of the command above. Skipping them is not Windows browser proof:

```
test_chrome_sign_in_sends_service_origin
test_phone_viewport_screenshot
```

## Portable app

Download the Actions artifact whose name starts with `scroll-studio-0.13.2-` and ends with `-windows-portable`. Unzip it and start `Scroll Studio.exe` only on a machine where you can see the window. The hosted job built that exe and did not launch it. A physical Windows GUI check still requires that launch on a machine with a desktop. This session did not do that launch.

## What the hosted run already showed

Run https://github.com/456C/scroll-studio/actions/runs/42722190076 at desktop `72c0ed65c671fab099a7203a74f43695a9288685` logged Python 3.12.10, the FFmpeg hash above, service directory `D:\a\scroll-studio\scroll-studio\service-464766d`, `Ran 30 tests` / `OK`, and Chrome at `C:\Program Files\Google\Chrome\Application\chrome.exe` completing authorize, affirm, preview, reader, and withdraw. That run did not use `scripts/windows-demo.ps1`. The next hosted run of this branch executes the script and uploads `scroll-studio-service-464766d`.
