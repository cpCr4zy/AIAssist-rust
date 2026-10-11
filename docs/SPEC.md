# Technical Specification: Cross-Platform GUI Application in Rust

Status: v1.0 · Source: reference project `pdfcraft` (v0.6.0, Rust 2024, egui/eframe 0.36)
Related: [CICD.md](CICD.md) for CI/CD, [README.md](README.md) for applying the template.

This document states the **requirements** (MUST), **recommendations** (SHOULD) and **options** (MAY)
for a new project. Ready-to-copy files live in [`template/`](template/) and are linked from the
text. Placeholders to replace (`new-project.sh` does it):

| Placeholder | Meaning | Example |
|---|---|---|
| `myapp` | crate/binary name, lowercase | `pdfcraft` |
| `MyApp` | display name | `PdfCraft` |
| `com.example.myapp` | reverse-DNS app ID (Wayland app_id, Flatpak, macOS bundle id) | `ai.storyteller.pdfcraft` |
| `example/myapp` | GitHub `owner/repo` | `storytold/pdfcraft` |
| `Example Org` | publisher (MSI, deb/rpm, AppStream) | `Learning Machines LLC` |

---

## 1. Target platforms

| Platform | Architectures | Minimum OS | Rust target | Delivery format |
|---|---|---|---|---|
| Windows | x64, x86, ARM64 | Windows 10 (ARM64: 11) | `x86_64-pc-windows-msvc`, `i686-pc-windows-msvc`, `aarch64-pc-windows-msvc` | MSI (WiX v5), portable ZIP |
| macOS | universal (arm64 + x86_64) | macOS 11 Big Sur | `aarch64-apple-darwin`, `x86_64-apple-darwin` | DMG with the `.app` (MAY: ZIP with a CLI) |
| Linux | x86_64, aarch64 | glibc ≥ 2.35 (Ubuntu 22.04+) | `*-unknown-linux-gnu` | AppImage (+ .zsync), deb, rpm, tar.gz |
| Linux | riscv64 (cross-compiled) | glibc ≥ 2.39 | `riscv64gc-unknown-linux-gnu` | tar.gz |
| Linux Flatpak | x86_64, aarch64 | runtime `org.freedesktop.Platform//25.08` | — (repackaged tar.gz) | `.flatpak` |
| FreeBSD | x86_64 | 14.x | native, in a VM | tar.gz (`/usr/local`-style tree) |
| Web | wasm32 | modern browser with WebGL2 | `wasm32-unknown-unknown` | ZIP with a static site (trunk) |

Requirements:

- MUST: one source tree builds on every platform; platform code only behind
  `#[cfg(target_os = "...")]` / `[target.'cfg(...)'.dependencies]`.
- MUST: Linux binaries are built on the **oldest** supported image (`ubuntu-22.04`), so they
  need glibc 2.35 and nothing newer.
- MUST: Windows binaries link the CRT statically (`-C target-feature=+crt-static`), so no VC++
  Redistributable is needed.
- MUST: `MACOSX_DEPLOYMENT_TARGET=11.0` and `LSMinimumSystemVersion=11.0` in `Info.plist` match.

## 2. Toolchain

- MUST: Rust **stable**, `edition = "2024"`, `resolver = "3"`.
- MUST: `rust-version` (MSRV) is set in `[workspace.package]` and is not below the chosen eframe's
  MSRV: egui/eframe/egui-wgpu/egui_kittest 0.36 require **1.95** (the template is verified on 1.99.0).
- SHOULD: no `rust-toolchain.toml` pinning a version (CI uses `dtolnay/rust-toolchain@stable`);
  when reproducibility matters, pin `channel = "1.xx"` and bump it deliberately.
- MUST: components `rustfmt`, `clippy`; targets per the table in §1.
- Supporting tools:

| Tool | Version (as in pdfcraft) | Purpose |
|---|---|---|
| `cargo-deny` | latest (`EmbarkStudios/cargo-deny-action@v2`) | licences, advisories, sources |
| `trunk` | 0.21.14 (prebuilt binary from GitHub Releases) | web/wasm build; downloads wasm-bindgen and wasm-opt itself |
| `zip` | from the package manager | web archive |
| WiX Toolset | 5.0.2 (`dotnet tool install --global wix --version 5.0.2`) | MSI |
| `nfpm` | 2.47.0 | deb/rpm |
| `appimagetool` | continuous | AppImage |
| `zsync` (`zsyncmake`) | from apt | AppImage delta updates |
| `flatpak-builder` | ≥ 1.4 (ubuntu-24.04) | Flatpak bundle |
| `actionlint` | 1.7.12 | workflow lint |
| `shellcheck`, `desktop-file-validate`, `appstreamcli`, `xmllint` | from apt | packaging lint |
| Windows SDK (`rc.exe`, `signtool.exe`) | preinstalled on `windows-latest` | exe resources, signing |
| Xcode CLT (`codesign`, `notarytool`, `lipo`, `hdiutil`) | preinstalled on `macos-*` | signing, notarization, DMG |

## 3. Repository layout (Cargo workspace)

```
.
├── Cargo.toml              # workspace: members, [workspace.package], [workspace.dependencies], lints, profiles
├── Cargo.lock              # MUST be committed (an application, not a library)
├── rustfmt.toml  clippy.toml  deny.toml
├── .shellcheckrc            # lets shellcheck follow packaging/env.sh from every script
├── .cargo/config.toml      # `cargo xtask` alias
├── .gitattributes          # `* -text`: no line-ending conversion, ever
├── AGENTS.md  CLAUDE.md    # rules for AI agents (CLAUDE.md imports AGENTS.md)
├── apps/
│   ├── myapp/              # desktop GUI (eframe, wgpu + glow), build.rs for Windows resources
│   ├── myapp-cli/          # (MAY) headless CLI on the same engine
│   └── myapp-web/          # wasm32 (eframe web, trunk)
├── crates/
│   ├── core/               # logic without GUI: MUST build for wasm32 and not depend on egui
│   └── ui/                 # egui shell (eframe::App) shared by desktop and web
├── xtask/                  # automation in Rust: version, ci, wasm, deny
├── packaging/
│   ├── env.sh              # shared variables: ROOT, VERSION, DIST, BUILD_SHA, BUILD_DATE
│   ├── linux/  macos/  windows/  freebsd/  web/
├── assets/app-icon/        # .ico, .icns, hicolor/*/apps/<app-id>.png|svg
├── docs/                   # SPEC.md, CICD.md, template-origin.txt (copied by new-project.sh)
└── .github/workflows/      # ci.yml, release.yml, freebsd.yml, windows-arm64.yml, packaging-lint.yml
```

Layering rules (MUST):

- `crates/*` below the UI layer do **not** depend on `egui`, `eframe`, `winit` or `rfd`.
- Every crate the web app uses passes `cargo check --target wasm32-unknown-unknown` (CI job `wasm`).
- Binaries (`apps/*`) stay thin: argument parsing, logging, the window; logic lives in `crates/`.

## 4. Dependencies

### 4.1 Workspace (`[workspace.dependencies]`)

Versions are declared **once** in the root `Cargo.toml`; crates refer to them with
`{ workspace = true }`. Reference set (versions proven in pdfcraft):

| Crate | Version | Features / notes | Purpose |
|---|---|---|---|
| `egui` | `0.36` | + `bytemuck` in the UI crate if it hands pixels to egui | immediate-mode GUI |
| `eframe` | `0.36` | `default-features = false`; each binary picks features (§4.2) | window, event loop, renderers |
| `egui_extras` | `0.36` | `default-features = false`, `svg` | image loaders, tables |
| `egui_kittest` | `0.36` | dev; `eframe` (add `wgpu` for snapshot images) | UI tests and snapshots |
| `rfd` | `0.17` | on wasm32: `file-handle-inner` | native open/save dialogs |
| `serde` | `1` | `derive`, `rc` | settings serialization |
| `serde_json` | `1` | | |
| `thiserror` | `2` | | error enums in libraries |
| `anyhow` | `1` | only in `xtask` and binaries | top-level errors |
| `log` | `0.4` | | logging facade |
| `env_logger` | `0.11` | `default-features = false`, `auto-color`, `humantime` | (MAY) simple logger; pdfcraft has its own with file rotation |
| `proptest` | `1` | dev | property tests for parsers |

No needless default features: `default-features = false` wherever a crate pulls in extras
(e.g. `flate2 = { version = "1.1", default-features = false, features = ["rust_backend"] }`:
pure Rust instead of C zlib).

### 4.2 eframe features per target

| Target | `eframe` features |
|---|---|
| Desktop | `default_fonts`, `wgpu`, `glow`, `accesskit`, `wayland`, `x11`, `persistence`, `links` |
| Web | `default_fonts`, `glow`, `accesskit`, `persistence` |

- MUST: the desktop app has **two** renderers: `wgpu` (primary; DX12 on Windows, Metal on macOS,
  Vulkan/GL on Linux) and `glow` (OpenGL) as the **fallback** when wgpu fails to start (old GPUs,
  VMs, RDP). `MYAPP_RENDERER=gl|wgpu` overrides the choice.
  Implementation: [`template/apps/myapp/src/main.rs`](template/apps/myapp/src/main.rs).
- MUST: `wayland` **and** `x11`, so one Linux binary works in both sessions.
- MUST: `accesskit` for accessibility (screen readers) on every OS.

### 4.3 Platform dependencies

```toml
[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
arboard  = { version = "3.6", default-features = false, features = ["image-data"] }  # clipboard with images
pollster = "1.0"                                                                      # wait for async rfd dialogs off the UI thread

[target.'cfg(target_os = "windows")'.dependencies]
sys-locale = "0.3"        # MAY: the OS interface language
winsafe    = { version = "0.0.29", default-features = false, features = ["user"] }  # MAY: safe Win32 wrappers

[target.'cfg(target_os = "macos")'.dependencies]
objc2            = "0.6"   # MAY: safe AppKit calls only
objc2-app-kit    = { version = "0.3", default-features = false, features = ["std", "NSEvent", "NSScreen"] }
objc2-foundation = { version = "0.3", default-features = false, features = ["std", "NSArray", "NSGeometry"] }

[target.'cfg(target_os = "linux")'.dependencies]
zbus = "5"                # MAY: the desktop portal's light/dark preference over D-Bus

[target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen         = "0.2"
wasm-bindgen-futures = "0.4"
js-sys               = "0.3"
web-sys = { version = "0.3", features = ["Window", "Document", "Element", "HtmlCanvasElement", "HtmlElement"] }

[build-dependencies]
winresource = "0.1"       # icon + VERSIONINFO + manifest in the .exe (used only when target_os = windows)
```

For HTTPS (update checks and the like): `ureq = { version = "3", default-features = false, features = ["rustls-no-provider", "_ring"] }` +
`rustls-native-certs = "0.8"` (the OS's root certificates, no CDLA-licensed `webpki-roots`).

### 4.4 Build-time system packages

| OS | Packages |
|---|---|
| Ubuntu/Debian (CI) | `libxkbcommon-dev libwayland-dev libx11-dev libxrandr-dev libxi-dev libgl1-mesa-dev libgtk-3-dev` |
| Fedora | `libxkbcommon-devel wayland-devel libX11-devel libXrandr-devel libXi-devel mesa-libGL-devel gtk3-devel` |
| Arch/CachyOS | `libxkbcommon wayland libx11 libxrandr libxi mesa gtk3` |
| FreeBSD | `bash git curl pkgconf cmake python3 libxkbcommon wayland libX11 libXcursor libXrandr libXi libxcb mesa-libs vulkan-loader gtk3 fontconfig freetype2` |
| Windows | MSVC Build Tools + Windows SDK; for ARM64 the "MSVC ARM64 build tools" component |
| macOS | Xcode Command Line Tools |

`libgtk-3-dev` is only for tests and rfd's GTK backend. Release Linux binaries link only `glibc`,
`libm` and `libgcc_s` directly; X11/Wayland/xkbcommon/Vulkan/EGL are loaded at runtime via
`dlopen`. That is why deb/rpm list them under `depends`/`recommends`, not as ELF `NEEDED` entries.

## 5. Cargo configuration

Template: [`template/Cargo.toml`](template/Cargo.toml).

```toml
[workspace.package]
version = "0.1.0"             # the ONLY place for the version; change it only with `cargo xtask version set X.Y.Z[-pre]`
edition = "2024"
license = "MIT OR Apache-2.0"
rust-version = "1.95"

[workspace.lints.rust]
unsafe_code = "forbid"

[workspace.lints.clippy]
dbg_macro = "warn"
todo = "warn"

[profile.dev]
opt-level = 1                 # a debug GUI is too slow otherwise

[profile.dev.package."*"]
opt-level = 2                 # dependencies (egui, wgpu, parsers) are optimised
debug = "line-tables-only"    # backtraces with file/line, without huge PDBs on Windows

[profile.release]
lto = "thin"
codegen-units = 1
```

- MUST: every crate has `[lints] workspace = true`.
- MUST: `version.workspace = true`, `edition.workspace = true`, `license.workspace = true`,
  `rust-version.workspace = true`.
- MAY: `[patch.crates-io]` pointing at `vendor/<crate>` for temporary patches of third-party
  crates (pdfcraft patches `winit` and `egui-wgpu`). Each patch is marked with a comment and
  described in `vendor/README.md`; `vendor` is in the workspace `exclude`.
- MUST: git dependencies only with a pinned `rev`, and the repository listed in `deny.toml`
  `[sources] allow-git`.

## 6. Code quality

Configs: [`rustfmt.toml`](template/rustfmt.toml), [`clippy.toml`](template/clippy.toml).

- MUST (quality gates, locally and in CI):
  1. `cargo fmt --all -- --check`
  2. `cargo clippy --workspace --all-targets -- -D warnings`
  3. `cargo test --workspace`
  4. `cargo xtask wasm`: the crates the web app uses build for wasm32
  5. `cargo deny check`: licences, advisories, sources
  All at once: `cargo xtask ci` (stops at the first failure).
- MUST "never panic" in non-test code (every file, input and setting is untrusted):
  - no `unwrap`/`expect`/`panic!`/`unreachable!`/`todo!`, except provably infallible spots with
    a comment saying why;
  - no indexing or slicing at input-derived positions: use `get()`, slice strings only at char
    boundaries;
  - checked/saturating arithmetic on input-derived numbers; guard against division by zero and NaN/inf;
  - cap allocations and recursion depth;
  - handle poisoned mutexes;
  - `unsafe_code = "forbid"`; FFI only through vetted safe wrappers (objc2, winsafe);
  - every crash fix gets a regression test.
  Web binaries can be stricter: `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`.
- MUST: a panic hook logs every panic (with a backtrace when `RUST_BACKTRACE=1`) before exit.
- SHOULD: UI tests with `egui_kittest` (headless); tests that need a GPU skip themselves when no
  adapter is available (FreeBSD VM, CI without a GPU).

## 7. Licence and dependency policy

Template: [`template/deny.toml`](template/deny.toml).

- MUST: only permissive licences: MIT, MIT-0, Apache-2.0 (and WITH LLVM-exception),
  BSD-2/3-Clause, ISC, Zlib, Unicode-3.0, BSL-1.0, CC0-1.0.
- MUST: GPL/AGPL/LGPL are never linked. Exceptions only per crate (`[[licenses.exceptions]]`),
  e.g. egui's fonts `epaint_default_fonts` → `OFL-1.1`, `Ubuntu-font-1.0`.
- MUST: `wildcards = "deny"`, `unknown-registry = "deny"`, `unknown-git = "deny"`.
- MUST: an ignored advisory always has a `reason`.
- SHOULD: project licence `MIT OR Apache-2.0` (`LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE`).
- SHOULD: assets (icons, fonts) only under open licences, recorded in `ATTRIBUTION.toml/md`.

## 8. Desktop application requirements

### 8.1 Startup and window

- MUST: `#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]`, so no
  console opens in Windows release builds; a CLI stays console-subsystem (packaging checks the PE
  header: GUI = 2, CLI = 3).
- MUST: `--version` prints `myapp X.Y.Z` and exits (used by CI smoke tests).
- MUST: `ViewportBuilder::with_app_id("com.example.myapp")`, matching the `.desktop` file (Wayland).
- MUST: a window icon via `with_icon(...)` from an embedded PNG (for unbundled runs).
- MUST: `persistence_path` in the OS settings folder:
  Linux/FreeBSD `$XDG_DATA_HOME/myapp` (`~/.local/share/myapp`), macOS
  `~/Library/Application Support/MyApp`, Windows `%APPDATA%\MyApp\data`.
  SHOULD: portable mode: a `portable.txt` beside the exe puts settings in `myapp-data\` beside it.
- MUST: the renderer is set explicitly; wgpu → glow fallback (§4.2).
- SHOULD: on Windows default to `Backends::DX12 | Backends::GL` unless `WGPU_BACKEND` is set.

### 8.2 Logging

- MUST: `log` with a logger writing to stderr; level from `RUST_LOG`.
- SHOULD: also log to `<settings>/logs/myapp.log`, rotated at startup (`.1.log`, `.2.log`) and
  capped in size (16 MiB), since apps started from a desktop menu have no terminal. Never log
  passwords or tokens.

### 8.3 OS integration

| OS | What is needed | Template files |
|---|---|---|
| Windows | icon, VERSIONINFO, PerMonitorV2 DPI manifest via `winresource` in `build.rs`; `MYAPP_REQUIRE_WINRES=1` turns a missing `rc.exe` into an error (releases) | `apps/myapp/build.rs` |
| Windows | per-machine MSI, Start Menu + desktop shortcut (`INSTALLDESKTOPSHORTCUT=0` skips it), App Paths, MajorUpgrade; MAY: "Open with" registration for the app's file types (ProgID, OpenWithProgids, Capabilities; a comment in the `.wxs` marks the place) | `packaging/windows/myapp.wxs` |
| macOS | `.app` with `Info.plist` (bundle id, `NSHighResolutionCapable`, `LSMinimumSystemVersion`, `CFBundleDocumentTypes`), `.icns`, hardened runtime, empty entitlements | `packaging/macos/` |
| macOS | files opened from Finder arrive as Apple events, not arguments: MAY use the `fmv-macos-events` crate | — |
| Linux | `.desktop`, AppStream `metainfo.xml`, MIME XML, `hicolor` icons, postinst refreshes caches | `packaging/linux/` |
| Flatpak | `--socket=wayland --socket=fallback-x11 --share=ipc --device=dri`, files through the portal, no network/home by default | `packaging/linux/flatpak/` |

### 8.4 Icons (`assets/app-icon/`)

- `myapp.ico` (16…256 px, Windows), `myapp.icns` (macOS),
  `hicolor/{16x16,32x32,48x48,64x64,128x128,256x256,512x512}/apps/com.example.myapp.png`,
  `hicolor/scalable/apps/com.example.myapp.svg`, `icon-256.png` (embedded in the binary).

## 9. Web (wasm32)

- MUST: a separate binary `apps/myapp-web`, built with `trunk build --release`, `public_url = "./"`
  (the site works from any sub-path and inside an iframe), `data-wasm-opt="z"`.
- MUST: renderer `Renderer::Glow` (WebGL2), the most compatible.
- Dropped files in egui 0.36 are the `DroppedFile` trait: `path()`/`bytes()` on the desktop,
  only `bytes_async()` in the browser; wasm has no file paths and no `rfd::FileDialog`.
- MUST: no handwritten JS: trunk generates the loader.
- MUST: `Content-Type: application/wasm`, immutable caching for `*.wasm`/`*.js`, `no-cache` for
  `index.html` (`packaging/web/_headers`).
- SHOULD: watch the `.wasm` size (Cloudflare Pages: ≤ 25 MiB per file).

## 10. Versioning and build metadata

- MUST: SemVer `X.Y.Z` or `X.Y.Z-rc.N`; the only source is `[workspace.package] version`.
- MUST: `cargo xtask version` prints it, `cargo xtask version set X.Y.Z` changes `Cargo.toml` and
  refreshes `Cargo.lock`.
- MUST: packaging uses `MYAPP_VERSION` (override), `MYAPP_BUILD_SHA`, `MYAPP_BUILD_DATE`
  (UTC `YYYY-MM-DD`); they end up in `Info.plist`, AppStream and artifact names.
- MUST: artifact names `myapp-<version>-<platform>-<arch>.<ext>`
  (`platform` ∈ `windows|macos|linux|freebsd`, `arch` ∈ `x64|x86|arm64|universal|x86_64|aarch64|riscv64`),
  a CLI `myapp-cli-<version>-…`, the web build `myapp-web-<version>.zip`.
- MSI: `ProductVersion` is numeric, so the `-rc.N` suffix is dropped; `AllowSameVersionUpgrades="yes"`.
- deb/rpm: `version_schema: semver` turns `0.2.0-rc.1` into `0.2.0~rc.1`.

## 11. Signing and notarization (optional)

Every secret is **optional**: without it the build still passes, the artifact is unsigned and a
`::warning::` is logged. Signing jobs run in the GitHub Environment `release`, which only the
`release` branch can use.

| Platform | Mechanism | Secrets |
|---|---|---|
| macOS | Developer ID Application + hardened runtime + `notarytool` + `stapler`; without a certificate, ad-hoc (`-`) | `APPLE_CERTIFICATE` (base64 .p12), `APPLE_CERTIFICATE_PASSWORD`, `KEYCHAIN_PASSWORD`, `APPLE_ID`, `APPLE_PASSWORD` (app-specific), `APPLE_TEAM_ID` |
| Windows | `signtool` SHA-256 + RFC 3161 timestamp: a .pfx **or** Azure Trusted Signing | `WINDOWS_CERTIFICATE` (base64 .pfx), `WINDOWS_CERTIFICATE_PASSWORD` **or** `AZURE_TENANT_ID`, `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_SIGNING_ENDPOINT`, `AZURE_SIGNING_ACCOUNT`, `AZURE_CERT_PROFILE` |
| Linux / FreeBSD / Web | not signed; integrity via `SHA256SUMS.txt` in the release | — |

macOS order: sign inside-out (binary → `.app`), `codesign --verify --strict --deep`, notarize the
`.app` (zipped) → staple → build the DMG → sign → notarize the DMG → staple.
The DMG is made with `hdiutil create -srcfolder` (not `makehybrid`), with retries.

## 12. New project checklist

- [ ] `./new-project.sh <dir> <crate> "<Name>" <app.id> <owner/repo> "<Vendor>"`
- [ ] Icons in `assets/app-icon/` (§8.4)
- [ ] `git init`, first commit including `Cargo.lock` (`cargo generate-lockfile`)
- [ ] `cargo xtask ci` green locally
- [ ] GitHub: `release` branch, `release` Environment (deployment branch: `release`), secrets (§11)
- [ ] Fresh `UpgradeCode` and component GUIDs in `myapp.wxs` (the script generates them)
- [ ] `metainfo.xml.in`: description, URLs, screenshots; `.desktop`: categories, MIME types
- [ ] First release per [CICD.md §5](CICD.md#5-release-process)
