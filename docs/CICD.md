# CI/CD Specification (GitHub Actions)

Status: v1.0 · Source: `pdfcraft/.github/workflows/*` · Related: [SPEC.md](SPEC.md)

Ready-made workflows: [`template/.github/workflows/`](template/.github/workflows/).

## 1. Overview

| Workflow | Triggers | Purpose | Duration |
|---|---|---|---|
| `ci.yml` | push to `main`, PRs, manual | fmt, clippy, tests (Linux + Windows), wasm check, cargo-deny | ~10–25 min |
| `windows-arm64.yml` | push to `main`, PRs touching manifests/packaging, weekly | cross-build the ARM64 MSI → install and run it on `windows-11-arm`, native tests on ARM64 | ~30–60 min |
| `freebsd.yml` | push to `main`, PRs touching manifests | build + test in a FreeBSD 14 VM | up to 5 h |
| `packaging-lint.yml` | changes to `.github/`, `packaging/`, icons | actionlint, shellcheck, PowerShell parser, xmllint, desktop/AppStream | seconds |
| `release.yml` | push to the `release` branch, manual (with `version`) | build, sign and package every platform → **draft** GitHub Release | ~40–90 min |

General rules (MUST):

- `concurrency: { group: <wf>-${{ github.ref }}, cancel-in-progress: true }` for CI;
  `cancel-in-progress: false` for releases (never interrupt notarization or uploads).
- `permissions: contents: read` by default; `contents: write` only for the `release` job.
- `env: CARGO_TERM_COLOR: always, RUST_BACKTRACE: 1`; releases also set `CARGO_INCREMENTAL: 0`.
- Cache: `Swatinem/rust-cache@v2` with a distinct `key` per job/architecture.
- Toolchain: `dtolnay/rust-toolchain@stable` with the needed `components`/`targets`.
- `--locked` in every build and test, so CI uses exactly the committed `Cargo.lock`.
- Third-party actions that run your code (VMs, cross-builds) are pinned **by SHA** with a
  version comment (`vmactions/freebsd-vm@<sha> # v1`).
- `fail-fast: false` in matrices, so one failing platform doesn't hide the others.

## 2. `ci.yml`: every PR

```
test (ubuntu-latest) ─┐
test (windows-latest) ┤
wasm32 check ─────────┼── independent, in parallel
deny ─────────────────┘
```

Job `test` (matrix `ubuntu-latest`, `windows-latest`):

1. checkout
2. Linux: `apt-get install libxkbcommon-dev libwayland-dev libx11-dev libxrandr-dev libxi-dev libgl1-mesa-dev libgtk-3-dev`
3. `dtolnay/rust-toolchain@stable` + `clippy, rustfmt`
4. Windows: `MYAPP_REQUIRE_WINRES=1` in `$GITHUB_ENV` (exe resources are required)
5. `Swatinem/rust-cache@v2`
6. Linux: `cargo fmt --all -- --check`
7. `cargo clippy --workspace --all-targets --locked -- -D warnings`
8. `cargo test --workspace --locked`

macOS is **not** run in CI (expensive runners); `release.yml` covers it. If the budget allows,
add `macos-latest` to the matrix (SHOULD for small projects).

Job `wasm`: target `wasm32-unknown-unknown`, `cargo xtask wasm`.
Job `deny`: `EmbarkStudios/cargo-deny-action@v2` with `command: check`.

MAY: a nightly `schedule` job for long-running checks (pdfcraft runs 15 minutes of mutation
fuzzing there and uploads findings as an artifact).

Branch protection for `main` (SHOULD): required checks `test (ubuntu-latest)`,
`test (windows-latest)`, `wasm32 check`, `dependency licences and advisories`.

## 3. Platform workflows

### `windows-arm64.yml`

1. `package`: on `windows-latest`, cross-compile `aarch64-pc-windows-msvc`, WiX 5.0.2,
   `packaging/windows/package.ps1 -Arch arm64` (unsigned), upload the artifact (7-day retention).
2. `install` (`windows-11-arm`): `msiexec /i /qn` → check the shortcut and PE Machine = `0xAA64`,
   run `myapp.exe --version` → `msiexec /x` → check that the files are gone.
3. `test` (`windows-11-arm`): `cargo test --workspace --locked`.

### `freebsd.yml`

`ubuntu-latest` + `vmactions/freebsd-vm` (release `14.3`, 4 CPUs, 12 GiB, `copyback: false`):
`pkg install` (list in SPEC §4.4), minimal rustup, `cargo build/test --workspace --locked` with
`CARGO_PROFILE_DEV_DEBUG=0`, then `packaging/freebsd/package.sh` with a lighter release profile
(`CARGO_PROFILE_RELEASE_LTO=false CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16`).

### `packaging-lint.yml`

No Rust build: `actionlint` 1.7.12, `shellcheck packaging/*.sh packaging/*/*.sh` (the root
`.shellcheckrc` lets it follow `packaging/env.sh`; without it shellcheck exits 1 on SC1091), parse every
`packaging/windows/*.ps1`, `xmllint` for `.wxs`/`.plist`, a WiX `Icon Id` check (must end in
`.ico`/`.exe`), `desktop-file-validate`, `appstreamcli validate --no-net`. A project that adds a
second, from-source Flatpak manifest (Flathub) SHOULD also check that both agree on
`runtime-version` and `finish-args`, as pdfcraft does.

## 4. `release.yml`: building a release

```
version ─┬─ macos (universal, sign+notarize)      ── environment: release
         ├─ windows [x64, x86, arm64] (sign)       ── environment: release
         ├─ linux [x86_64, aarch64] ──── flatpak [x86_64, aarch64]
         ├─ linux-riscv64 (cross)
         ├─ freebsd (VM)
         └─ web (trunk)
                     └──────────── release (draft, SHA256SUMS) ── environment: release, contents: write
```

| Job | Runner | Output |
|---|---|---|
| `version` | `ubuntu-latest` | `version`, `date`, `prerelease` (SemVer validated; a `-…` suffix → prerelease) |
| `macos` | `macos-15` (or `macos-15-xlarge`) | `myapp-<v>-macos-universal.dmg` |
| `windows` | `windows-latest` × {x64, x86, arm64} | `myapp-<v>-windows-<arch>.msi`, `…-portable.zip` |
| `linux` | `ubuntu-22.04`, `ubuntu-22.04-arm` | `.AppImage`, `.AppImage.zsync`, `.deb`, `.rpm`, `.tar.gz` |
| `linux-riscv64` | `ubuntu-24.04` (cross, ports.ubuntu.com, QEMU smoke test) | `.tar.gz` |
| `flatpak` | `ubuntu-24.04`, `ubuntu-24.04-arm` | `.flatpak` (repackages the `linux` tarball, no compilation) |
| `freebsd` | `ubuntu-latest` + FreeBSD VM | `myapp-<v>-freebsd-x86_64.tar.gz` |
| `web` | `ubuntu-latest` | `myapp-web-<v>.zip` |
| `release` | `ubuntu-latest` | draft Release `MyApp v<v>` + `SHA256SUMS.txt` |

Key requirements:

- MUST: every job uploads an artifact `release-<platform>[-<arch>]` with `if-no-files-found: error`;
  `release` downloads `pattern: release-*` with `merge-multiple: true`.
- MUST: every packaging script checks its own output (PE Machine/Subsystem,
  `codesign --verify --strict`, `dpkg-deb --contents`, the AppImage's `--version`, `file` for riscv64).
- MUST: the draft release is created or updated; if the tag is already published, fail with
  "bump the version".
- MUST: `--generate-notes`, so the notes come from merged PRs.
- MUST: secrets go only into the `env:` of the step that needs them, never the whole job.
- A manual run with `version: 0.3.0-rc.1` applies `cargo xtask version set` in every job; on any
  branch other than `release` it is a **dry run**: the Environment rule refuses the signing jobs
  and no draft is created.

## 5. Release process

1. On `main`: `cargo xtask version set 0.3.0` (or `0.3.0-rc.1`), a PR "Release: MyApp v0.3.0", merge.
2. `git checkout release && git merge --ff-only main && git push` → `release.yml` starts.
3. Wait for the draft `MyApp v0.3.0` (notarization is the slow part). Read the job summaries:
   a `::warning::` means an unsigned artifact.
4. Download one or two installers and try them.
5. Publish the draft in the GitHub UI → this creates the tag `v0.3.0`. A `-rc.N` suffix → pre-release.

Pushing to `release` again before publishing rebuilds the same draft. After publishing, only a new
version can be released.

Dry run on a branch: `gh workflow run release.yml --ref <branch> -f version=0.3.0-rc.1`.

## 6. GitHub repository setup

| What | Where | Value |
|---|---|---|
| `release` branch | Branches | protected, push for maintainers only |
| `release` Environment | Settings ▸ Environments | Deployment branches: `release` only; (MAY) required reviewers |
| Signing secrets | Environment `release` ▸ Secrets | see SPEC §11 (all optional) |
| Actions permissions | Settings ▸ Actions | Workflow permissions: read; releases are created only via `permissions:` |
| ARM runners | — | `ubuntu-22.04-arm`, `ubuntu-24.04-arm`, `windows-11-arm` are available to public repos; for private repos check the plan or remove them from the matrices |
| Dependabot | `.github/dependabot.yml` | weekly `cargo` and `github-actions` |

Preparing secrets:

```bash
base64 -w0 developer-id.p12 > APPLE_CERTIFICATE.txt
```

```bash
base64 -w0 codesign.pfx > WINDOWS_CERTIFICATE.txt
```

```bash
gh secret set APPLE_CERTIFICATE --env release < APPLE_CERTIFICATE.txt
```

## 7. Reproducing locally

| Action | Command |
|---|---|
| all gates | `cargo xtask ci` |
| Linux packages | `packaging/linux/package.sh --formats "tar deb"` |
| macOS DMG (ad-hoc) | `packaging/macos/package.sh --arch aarch64` |
| Windows MSI | `pwsh packaging/windows/package.ps1 -Arch x64` |
| Web | `packaging/web/package.sh` |
| workflow lint | `actionlint` |
