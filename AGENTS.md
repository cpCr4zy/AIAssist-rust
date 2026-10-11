# AI Assistant: instructions for AI agents

AI Assistant is a cross-platform GUI application in Rust (egui/eframe 0.36): Windows, macOS, Linux,
FreeBSD and the web (wasm32). It was created from a template; the parameters and the template
commit are in `docs/template-origin.txt`.

## Sources of truth

- `docs/SPEC.md`: stack, dependencies, layout, code rules, application requirements.
- `docs/CICD.md`: GitHub Actions workflows and the release process.
- These documents are shared by every project made from the template: `template/<path>` in them
  means `<path>` at the root of this repository, and the placeholders in the table at the top of
  `docs/SPEC.md` stand for the `crate`, `name`, `app_id`, `repo` and `vendor` values in
  `docs/template-origin.txt`.
- If the code and SPEC disagree, ask the user which is right first, then fix one of them.

## Layout

| Path | What | Rule |
|---|---|---|
| `crates/core` | logic, no GUI | no `egui`/`eframe`/`winit`/`rfd`; builds for wasm32 |
| `crates/ui` | `MainApp: eframe::App`, shared by desktop and web | desktop-only code under `#[cfg(not(target_arch = "wasm32"))]` |
| `apps/aiassist` | desktop binary | thin: arguments, logging, window, wgpu → OpenGL fallback |
| `apps/aiassist-web` | wasm binary (trunk) | no handwritten JS |
| `xtask` | `cargo xtask` with `version`, `ci`, `wasm`, `deny` | automation in Rust only |
| `packaging/` | installers for every platform | artifact names: `aiassist-<version>-<platform>-<arch>.<ext>` |

New library crates go in `crates/` and are added to `cargo xtask wasm` (`xtask/src/main.rs`) if
the browser build uses them.

## Rules

1. **Gates before every commit:** `cargo xtask ci` (fmt, clippy `-D warnings`, tests, wasm32,
   cargo-deny). Commit only green states.
2. **Never panic** in non-test code: no `unwrap`/`expect`/`panic!`/`todo!`/`unreachable!`
   (except provably infallible spots with a comment), no indexing by input-derived positions
   (`get()`), checked/saturating arithmetic, limits on input size and recursion depth. Errors are
   `Result` with a `thiserror` enum. Every crash fix gets a regression test.
3. **`unsafe_code = "forbid"`.** FFI only through vetted safe crates.
4. **Dependencies:** the version is declared once in `[workspace.dependencies]`, crates use
   `{ workspace = true }`; `default-features = false` where a crate pulls in extras. Permissive
   licences only (`deny.toml`); git dependencies with a pinned `rev` and listed in `allow-git`.
   Before using a crate, check its API in `~/.cargo/registry/src/*/<crate>-<ver>/`, not from memory.
5. **MSRV:** `rust-version` is not below eframe's MSRV (0.36 → 1.95). Upgrade the whole
   `egui*`/`eframe` group together.
6. **The version** lives only in `[workspace.package] version`; change it with
   `cargo xtask version set X.Y.Z[-rc.N]`.
7. **Platform code** only behind `#[cfg(...)]` and `[target.'cfg(...)'.dependencies]`. When a
   change affects one OS, make sure the others still build (`cargo xtask wasm` locally,
   Windows/macOS through CI).
8. Change **packaging and workflows** together with their checks (`packaging-lint.yml`, the
   checks in `release.yml`).
9. Never weaken lints, `deny.toml` or CI checks to make something pass: fix the cause.

## Running and looking at the result

```bash
cargo run -p aiassist -- [files…]            # desktop
AIASSIST_RENDERER=gl cargo run -p aiassist      # force OpenGL
cd apps/aiassist-web && trunk serve          # web on http://127.0.0.1:8080
cargo xtask ci                            # every gate
packaging/linux/package.sh --formats tar  # release package, locally
```

Check UI changes visually (run the app and take a screenshot) and with an `egui_kittest` test in
`crates/ui/tests/`.

## Commits and releases

- One logical change per commit, message in the imperative mood.
- Release: `cargo xtask version set X.Y.Z` → PR to `main` → fast-forward `release` → push →
  check the draft release → publish (details in `docs/CICD.md` §5). Never push to `release` or
  publish a release unless the user explicitly asks.
