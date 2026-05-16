# Death By MPV

A simple video player based on mpv, built with Tauri.

## Building from source

### Prerequisites

Follow the platform-specific setup in the Tauri v2 docs: <https://v2.tauri.app/start/prerequisites/>

In short, you'll need:

- **Rust** (stable toolchain) — install via [rustup](https://rustup.rs/)
- **Tauri system dependencies** — on Windows that's the Microsoft C++ Build Tools and WebView2 (preinstalled on Windows 11)
- **Node.js** — only needed for installing the Tauri CLI via npm; you can skip it if you install the CLI through cargo

Install the Tauri CLI:

```sh
cargo install tauri-cli --version "^2.0.0"
```

(Or via npm: `npm install -g @tauri-apps/cli`.)

### Build

Clone the repo and run:

```sh
cargo tauri build
```

The bundled installer/executable will be in `src-tauri/target/release/` (and `src-tauri/target/release/bundle/` for the installer).

### Run in development

```sh
cargo tauri dev
```

### mpv

A prebuilt `libmpv-2.dll` is checked in under `src-tauri/lib/` and bundled with the app — no separate mpv install is required.
