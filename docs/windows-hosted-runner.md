# Manual demo on a Windows machine

This is the local procedure. It is not a physical Windows desktop test of the egui window. The hosted runner executed `scripts/windows-demo.ps1` and did not start `Scroll Studio.exe`.

The pair the hosted script run used:

- Desktop sources: `e70248036f1180a96fcd0b9a585960f406a72613`
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

On another machine, install that same release from the official page https://www.python.org/downloads/release/python-31210/. The Windows installer (64-bit) is https://www.python.org/ftp/python/3.12.10/python-3.12.10-amd64.exe. That page lists it as Recommended, 25.7 MB, MD5 `5eddb0b6f12c852725de071ae681dde4`. The file is 26964224 bytes. Installer switches are the ones documented at https://docs.python.org/3.12/using/windows.html (`/quiet`, `PrependPath`, `Include_test`).

```powershell
$exe = Join-Path $env:TEMP 'python-3.12.10-amd64.exe'
Invoke-WebRequest -Uri 'https://www.python.org/ftp/python/3.12.10/python-3.12.10-amd64.exe' -OutFile $exe
if ((Get-Item -LiteralPath $exe).Length -ne 26964224) { throw 'size' }
$md5 = (Get-FileHash -Algorithm MD5 -LiteralPath $exe).Hash.ToLowerInvariant()
if ($md5 -ne '5eddb0b6f12c852725de071ae681dde4') { throw "md5 $md5" }
Start-Process -FilePath $exe -ArgumentList '/quiet','PrependPath=1','Include_test=0' -Wait
```

Close that PowerShell window and open a new one so the installer change to PATH is visible. Then run:

```powershell
python -c "import sys; print(sys.version); print(sys.executable)"
```

The version line must be `3.12.10 (tags/v3.12.10:0cc8128, Apr  8 2025, 12:21:36) [MSC v.1943 64 bit (AMD64)]`. The executable path is whichever directory the installer added. It is not the hosted toolcache path.

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

The passing run uploaded `scroll-studio-service-464766d`, artifact id 13720169582, 11607074 bytes:

https://github.com/456C/scroll-studio/actions/runs/42723986006/artifacts/13720169582

Download it while signed in to GitHub. The repository is private, so an unsigned download fails. The guide does not contain a token.

```powershell
Expand-Archive -LiteralPath scroll-studio-service-464766d.zip -DestinationPath C:\scroll-demo -Force
Set-Location C:\scroll-demo
python -m unittest prototypes.publishing.test_contract prototypes.publishing.test_listener.ListenerTests.test_bind_refuses_non_loopback prototypes.publishing.test_listener.ListenerTests.test_publish_preview_reader_and_pixels prototypes.publishing.test_listener.ListenerTests.test_prose_unicode_preview_and_byte_cap prototypes.publishing.test_listener.ListenerTests.test_replay_revoke_expiry_cancel_and_recovery prototypes.publishing.test_listener.ListenerTests.test_browser_csrf_host_origin_and_withdrawal prototypes.publishing.test_listener.ListenerTests.test_operator_report_hold_remove prototypes.publishing.test_listener.ListenerTests.test_body_cap_closes_before_the_payload prototypes.publishing.test_listener.ListenerTests.test_hold_and_remove_do_not_yield_to_the_owner prototypes.publishing.test_listener.ListenerTests.test_sign_in_csrf_expires_once_and_matches_next prototypes.publishing.test_listener.ListenerTests.test_rejected_connections_close prototypes.publishing.test_listener.ListenerTests.test_busy_admission_does_not_follow_a_trickle
python -m prototypes.publishing.listener
```

`python -m prototypes.publishing.listener` keeps running in that PowerShell window. Stop it by pressing Ctrl+C in that window. Publications and sessions exist only in process memory and disappear when that process exits.

`scripts/windows-demo.ps1` is the hosted runner's non-interactive copy of the extract, test, and browser sequence. The runner has no console for Ctrl+C, so the script stops the listener process it started after the browser check. A person using the commands above presses Ctrl+C.

These two tests are Linux Chrome tests and are not part of the command above. Skipping them is not Windows browser proof:

```
test_chrome_sign_in_sends_service_origin
test_phone_viewport_screenshot
```

## Portable app

The same run uploaded `scroll-studio-0.13.2-e702480-windows-portable`, artifact id 13720274253, 5760452 bytes:

https://github.com/456C/scroll-studio/actions/runs/42723986006/artifacts/13720274253

Download it while signed in, unzip it, and start `Scroll Studio.exe` only on a machine where you can see the window. The hosted job built that exe and did not launch it. A physical Windows GUI check still requires that launch on a machine with a desktop. This session did not do that launch.

## What the hosted run already showed

Run https://github.com/456C/scroll-studio/actions/runs/42723986006 at desktop `e70248036f1180a96fcd0b9a585960f406a72613` executed `scripts/windows-demo.ps1`. It logged Python 3.12.10 at `C:\hostedtoolcache\windows\Python\3.12.10\x64\python.exe`, png decoder True, mjpeg decoder True, `Ran 30 tests in 6.171s` / `OK`, and Chrome at `C:\Program Files\Google\Chrome\Application\chrome.exe` with the lines `opened fixture authorize page`, `affirm reached the callback with code and state only`, `preview showed the uploaded paragraph`, `reader showed the published paragraph`, and `reader showed unavailable after withdraw`. Those browser lines share one log timestamp. The job uploaded `scroll-studio-service-464766d` (artifact id 13720169582, 11607074 bytes) and `scroll-studio-0.13.2-e702480-windows-portable` (artifact id 13720274253, 5760452 bytes). It did not start `Scroll Studio.exe`.

Run https://github.com/456C/scroll-studio/actions/runs/42722190076 at desktop `72c0ed65c671fab099a7203a74f43695a9288685` passed the same 30 tests and the same Chrome route before `scripts/windows-demo.ps1` existed.
