# AI Assistant

A cross-platform GUI application in Rust (egui/eframe): Windows, macOS, Linux, FreeBSD and the web.

## Build and run

```sh
cargo run -p aiassist                      # desktop app
cargo xtask ci                          # every quality gate CI runs
cd apps/aiassist-web && trunk serve        # web version on http://127.0.0.1:8080
```

Linux needs the windowing headers first, e.g. on Debian/Ubuntu:

```sh
sudo apt-get install libxkbcommon-dev libwayland-dev libx11-dev libxrandr-dev libxi-dev libgl1-mesa-dev libgtk-3-dev
```

## Layout

| Path | What |
|---|---|
| `crates/core` | engine, no GUI dependencies, builds for wasm32 |
| `crates/ui` | egui shell shared by desktop and web |
| `apps/aiassist` | desktop binary (wgpu, OpenGL fallback) |
| `apps/aiassist-web` | browser binary (trunk) |
| `xtask` | `cargo xtask version`, `ci`, `wasm`, `deny` |
| `packaging/` | installers for every platform |

## Releasing

`cargo xtask version set X.Y.Z`, merge to `main`, fast-forward `release` and push: the Release
workflow drafts a GitHub Release with every installer.

## Licence

MIT OR Apache-2.0.
