# Kimi GUI

Desktop UI for Kimi Code CLI built with Rust + Tauri 2 and a static HTML/CSS/JS
frontend. The GUI embeds the real `kimi` CLI inside a
PTY-powered terminal to keep feature parity with the CLI.

## Prerequisites

- Node.js 18+
- Rust 1.75+
- Tauri CLI 2.x (via npm dev dependency or `cargo install tauri-cli --version "^2.0.0" --locked`)

## Development

```bash
npm install
npm run build
npm run tauri dev
```

If the CLI binary is not on PATH, set the command explicitly:

```bash
KIMI_GUI_COMMAND="python -m kimi_cli" npm run tauri dev
```

`npm run build` validates the static UI assets (no bundler required).

## Build

```bash
npm run tauri build
```

## Notes

- The Tauri config lives in `src-tauri/tauri.conf.json`.
- The Rust backend exposes PTY commands and config helpers used by the UI.
- The GUI searches for `kimi`/`kimi-cli` on PATH, then falls back to `python -m kimi_cli`.
- Use the Control Center to configure models, skills, MCP, and login, or set
  `KIMI_GUI_COMMAND` to override the launch command.
