# AI Lite

The first Scroll Studio addon. It runs as its own small program; nothing in it is part of the
Scroll Studio app, and the app works the same without it.

- **Star button (☆).** Appears on character sheets, the lore board, the story style window and in
  each beat's `···` menu while AI Lite is enabled. Type what should change; AI Lite asks a model
  on your computer and Scroll Studio shows the suggested change. Apply makes it one undo step.
- **MCP server.** Off by default. Turn it on to let an AI app you already use (Claude Desktop,
  an editor, any MCP client) read the open story and suggest edits. Every edit still waits for
  your yes in Scroll Studio.

## Model server

AI Lite talks to any model server with an OpenAI-compatible API over plain `http://`:

| Server | Address in AI Lite's settings |
| --- | --- |
| [Ollama](https://ollama.com) (`ollama pull llama3.2`) | `http://127.0.0.1:11434` (default) |
| LM Studio (start its local server) | `http://127.0.0.1:1234` |
| llama.cpp `llama-server` | `http://127.0.0.1:8080` |

Leave **Model** empty to use the first model the server lists. Settings are kept in
`config.json` in the addon's folder, never in your stories. Cloud APIs with your own key are
planned for a later version.

## MCP

Turn on **MCP server** in Settings → Addons → AI Lite → Settings. It listens on
`http://127.0.0.1:39390/mcp` (port adjustable) using the Streamable HTTP transport, and every
request needs the header `Authorization: Bearer <token>` with the token shown there. The token
changes each time Scroll Studio starts. Requests carrying a web page `Origin` are refused.

Tools mirror Scroll Studio's read and edit methods one to one (`story_summary`,
`characters_list`, `edit_set_field`, `edit_batch`, …), with descriptions and parameter schemas
supplied by the app. An edit tool returns once you apply or cancel it; one edit can wait at a
time.

## Build and package

```bash
cargo build --release
```

Then zip `addon.json` and the program (named `ai-lite.exe`, as `addon.json` says) at the top
level of the archive, and install the `.zip` from Settings → Addons → Install from file.

## Protocol

Scroll Studio starts the program in its folder and talks JSON-RPC 2.0, one message per line,
over stdin/stdout; the addon exits when its input closes. See `src/addons/host.rs` in the app
for the message list.
