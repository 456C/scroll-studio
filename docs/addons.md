# Writing a Scroll Studio addon

An addon is a separate program that Scroll Studio starts and talks to. It never touches story
files: it reads the story through the app and *proposes* edits, which the user reviews and
applies as one undo step. With no addons installed the app shows no addon controls at all.

AI Lite (`addons/ai-lite`) is the reference addon; its source shows everything below in use.

## Package

A `.zip` with `addon.json` and the program at its top level. Users install it from
**Settings → Addons → Install from file**. It is unpacked into
`%LOCALAPPDATA%\Scroll Studio\addons\<id>\` (elsewhere: `$XDG_DATA_HOME/scroll-studio/addons`),
starts disabled, and is started from that folder once the user enables it.

```json
{
  "id": "ai-lite",
  "name": "AI Lite",
  "version": "0.1.0",
  "min_app": "0.14.0",
  "requires": [],
  "entry": "ai-lite.exe",
  "permissions": ["story.read", "story.propose"],
  "download_mb": 1,
  "hardware": "Any PC that runs Scroll Studio",
  "assist": "Ask AI Lite"
}
```

| Key | Meaning |
| --- | --- |
| `id` | Lowercase letters, digits and `-`; also the folder name. |
| `name` | 1–64 characters, shown in Settings. |
| `version`, `min_app` | Semver. The app refuses addons that need a newer app. |
| `requires` | IDs of addons that must be installed before this one can start. |
| `entry` | The program, relative to the addon folder (no `..`). |
| `permissions` | `story.read` (read API) and `story.propose` (suggest edits). `story.apply` is refused. |
| `download_mb`, `hardware` | Shown to the user. |
| `assist` | Optional label (1–32 characters, needs `story.propose`). Adds a ☆ button to character sheets, the lore board and story style, and an entry in each beat's menu. |

Unknown keys are ignored, so newer addons can add fields. Updating with a package that asks
for different permissions leaves the addon disabled until the user enables it again.

Permissions limit what an addon may ask the app for. They are not a sandbox: an addon is a
program running with the user's rights, and Settings says so.

## Protocol

JSON-RPC 2.0, one message per line (UTF-8, at most 16 MiB), over the program's stdin and
stdout. Error output (stderr) is kept as the last 20 lines and shown when the addon stops. The
environment variable `SCROLL_STUDIO_ADDON_PROTOCOL` holds the protocol version (`1`). Exit when
stdin closes: that is how the app stops an addon, before ending the process.

### From the app

| Message | Kind | Meaning |
| --- | --- | --- |
| `host.ready {protocol, app_version, permissions, revision}` | notification | Sent once at start. |
| `story.changed {revision, scope}` | notification | After every change, for addons with `story.read`. `scope` is `"everything"` or `{"ids": [...]}`. |
| `addon.assist {request, target}` | request | The user typed `request` in the star box. `target` is `{"kind": "story"}`, `{"kind": "lore"}`, or `{"kind": "character" \| "panel", "id": n}`. Answer when done: `{"applied": bool, "summary": text}`. With `applied: false` the box stays open (with `summary` shown) so the user can reword. An error answer is shown in the box. |
| `addon.cancel {id}` | notification | The user gave up on request `id`. |
| `addon.settings`, `addon.set_setting {key, value}` | request | Answer `{"fields": [{key, label, kind, value}]}`; `kind` is `text`, `toggle` or `info` (read-only, with a Copy button). Shown under the addon's Settings disclosure. |

The app's requests use integer IDs; answers to requests it did not send are ignored.

### From the addon

| Method | Needs | Answer |
| --- | --- | --- |
| `host.methods` | — | `{"methods": [...]}`: the methods this addon may call. |
| `host.describe` | — | `{"methods": [{method, about, params}]}` with each method's parameters as JSON Schema. |
| `story.summary`, `chapters.list`, `panels.list`, `layers.list`, `layer.export`, `characters.list`, `assets.list`, `locations.list`, `groups.list`, `timeline.list`, `object.get`, `lore.board`, `panels.at` | `story.read` | The data, at once. |
| `edit.*` (43 methods, including `edit.batch`) | `story.propose` | Once the user decides: `{"applied": true, "created": [ids]}`, or an error below. |

`host.describe` is the reference for parameters; `src/wire/docs.rs` is its source. IDs are
integers and the story itself is `0`. Unknown parameters are refused rather than ignored.
`edit.batch` takes a list of `{"method": "set_field", "params": {...}}` (no `edit.` prefix,
no nesting, at most 1000) and is applied and undone as one step.

Every edit is rehearsed on a copy of the story first: one that would fail, change nothing, or
change a locked layer is refused before the user sees it, and checked again when they press
Apply. Commands that need image bytes are not available to addons in protocol 1.

### Errors

| Code | Meaning |
| --- | --- |
| -32700, -32600, -32601, -32602 | JSON-RPC: not JSON, invalid request, unknown method, invalid parameters. |
| -32001 | The addon lacks the permission for this method. |
| -32002 | Refused: the edit cannot be applied (it fails, changes nothing, or touches a locked layer). |
| -32003 | The user cancelled the suggestion. |
| -32004 | Busy: another suggestion from this addon is waiting for the user. |

A line the app cannot use is answered with an error whose `id` is `null`.

## Being a good citizen

- **One suggestion at a time.** A second one gets `-32004` until the user decides.
- **Don't nag.** Turning down a suggestion the user asked for (through the star) never counts
  against an addon. After 3 unrequested suggestions in a row are turned down, further
  unrequested ones get `-32002` until the user applies one or asks through the star.
- **Stay responsive.** The app never waits on an addon: an addon that stops reading its input
  (256 queued messages) or floods output is stopped. A crashed addon is reported in the status
  bar and can be restarted from Settings; the story is never affected.
- **Keep secrets out of stories.** Store settings and keys in the addon's own folder, never in
  a `.scro` file.
