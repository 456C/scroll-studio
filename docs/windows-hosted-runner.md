# Manual demo on a Windows machine

This is the local procedure. It is not a physical Windows desktop test of the egui window. The hosted runner executed `scripts/windows-demo.ps1` and did not start `Scroll Studio.exe`.

The package this guide starts:

- Desktop sources for the passing hosted run: `69d1f9b2a7e5839880d948e2e289125c76ae89cb`
- Service pin: `91ba4894750d4e0d22a784c05b50ba89837451cf`
- Start route: http://127.0.0.1:19427/

The homepage does not show the story. Choose **Enter the synthetic demo**, then Paper Harbor, episode 1 The Fold, and episode 2 What the Margin Kept. Reading those pages does not sign in and does not publish a creator episode. FFmpeg is not required to read the demo. It is required for a creator image upload.

A machine already running desktop `e70248036f1180a96fcd0b9a585960f406a72613` with service `464766d97087cb26a12de6e009323951c43e1596` stays on that pair until a reviewed next action. This guide does not switch it.

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

Download the Actions artifact named `scroll-studio-service-91ba489` while signed in to GitHub. That zip is the runnable service package. The repository is private, so an unsigned download fails. The guide does not contain a token. The hosted job builds this zip from service pin `91ba4894750d4e0d22a784c05b50ba89837451cf`. A person does not check out that pin to start the listener.

The zip contains `prototypes\publishing\demo_site.py` and these panel files, unchanged from the pin:

```
panel-fold.jpg    166b246d70a3b6d55601db68d6c0da042f6aa651921b03e13b7e2fb23fe7c7bd
panel-hull.jpg    b9b8e277b8191a10d47050e8b66a93b32ce0daf1a15bcae7003590314ddaba70
panel-sill.jpg    5ab59c0b261e86d81dd628dde809af15ababbada64d1da41980103b1b1d260a3
panel-puddle.jpg  e9ca13b83505f0d3b26cabc45db3ac369dd192b3ab87a9b954705296a298bcb5
```

```powershell
Expand-Archive -LiteralPath scroll-studio-service-91ba489.zip -DestinationPath C:\scroll-demo -Force
Set-Location C:\scroll-demo
python -m unittest prototypes.publishing.test_contract prototypes.publishing.test_listener.ListenerTests.test_bind_refuses_non_loopback prototypes.publishing.test_listener.ListenerTests.test_publish_preview_reader_and_pixels prototypes.publishing.test_listener.ListenerTests.test_prose_unicode_preview_and_byte_cap prototypes.publishing.test_listener.ListenerTests.test_replay_revoke_expiry_cancel_and_recovery prototypes.publishing.test_listener.ListenerTests.test_browser_csrf_host_origin_and_withdrawal prototypes.publishing.test_listener.ListenerTests.test_operator_report_hold_remove prototypes.publishing.test_listener.ListenerTests.test_body_cap_closes_before_the_payload prototypes.publishing.test_listener.ListenerTests.test_hold_and_remove_do_not_yield_to_the_owner prototypes.publishing.test_listener.ListenerTests.test_sign_in_csrf_expires_once_and_matches_next prototypes.publishing.test_listener.ListenerTests.test_rejected_connections_close prototypes.publishing.test_listener.ListenerTests.test_busy_admission_does_not_follow_a_trickle prototypes.publishing.test_listener.ListenerTests.test_demo_catalogue_is_separate_from_creator_state
python -m prototypes.publishing.listener
```

Open http://127.0.0.1:19427/ in a browser. The first page offers Enter the synthetic demo. It does not show The Fold.

`python -m prototypes.publishing.listener` keeps running in that PowerShell window. Stop it by pressing Ctrl+C in that window. Publications and sessions exist only in process memory and disappear when that process exits.

`scripts/windows-demo.ps1` is the hosted runner's non-interactive copy of the extract, test, and browser sequence. The runner has no console for Ctrl+C, so the script stops the listener process it started after the browser check. A person using the commands above presses Ctrl+C.

These Linux Chrome tests are not part of the command above. Skipping them is not Windows browser proof:

```
test_demo_reading_in_chrome
test_chrome_sign_in_sends_service_origin
test_phone_viewport_screenshot
```

## Portable app

Download the Actions artifact whose name starts with `scroll-studio-0.13.2-` and ends with `-windows-portable` from the same run. Unzip it and start `Scroll Studio.exe` only on a machine where you can see the window. The hosted job builds that exe and does not launch it.

## Hosted proof

Run https://github.com/456C/scroll-studio/actions/runs/42746459364 at desktop `69d1f9b2a7e5839880d948e2e289125c76ae89cb`.

- Process: listener pid 5484, working directory `D:\a\_temp\demo-work\extracted`, command `python -m prototypes.publishing.listener`.
- HTTP: `GET /` returned `HTTP/1.1 200 OK`. Each `panel-fold.jpg`, `panel-hull.jpg`, `panel-sill.jpg`, and `panel-puddle.jpg` returned `HTTP/1.1 200 OK` and `image/jpeg`. `GET /ss-publish/browser/series` and `GET /ss-publish/browser/operator` returned `HTTP/1.1 401 Unauthorized`.
- Render: Windows Chrome at `C:\Program Files\Google\Chrome\Application\chrome.exe` showed Enter the synthetic demo, entered it, opened Paper Harbor, showed The Fold and four pictures, showed What the Margin Kept, used Previous back to The Fold, and used Back to the series. The same browser then completed the existing creator route: affirm, preview paragraph, reader paragraph, and unavailable after withdraw.
- Selected tests, including `test_demo_catalogue_is_separate_from_creator_state`: `Ran 31 tests in 7.723s` / `OK`.
- Service package sha256 `b61d17fb307320a229475fdaeb970e9b0d9a72282f77200ace2d25d591882873`. Uploaded artifact `scroll-studio-service-91ba489`, id 13731975216, 13232992 bytes: https://github.com/456C/scroll-studio/actions/runs/42746459364/artifacts/13731975216
- Portable zip `scroll-studio-0.13.2-69d1f9b-windows-portable`, id 13730379871, 5760452 bytes: https://github.com/456C/scroll-studio/actions/runs/42746459364/artifacts/13730379871
- `Scroll Studio.exe` was not launched. This run saved no Windows screenshot.

## Earlier hosted pair

Run https://github.com/456C/scroll-studio/actions/runs/42723986006 at desktop `e70248036f1180a96fcd0b9a585960f406a72613` and service `464766d97087cb26a12de6e009323951c43e1596` passed 30 tests and the creator Chrome route. Its portable artifact id is 13720274253. It did not include this demo website. Linux proof of pin `91ba4894750d4e0d22a784c05b50ba89837451cf` was 34 tests and Chrome 156. That Linux run is not Windows proof.
