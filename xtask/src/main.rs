//! Project automation in Rust: `cargo xtask <command>` (alias in .cargo/config.toml).

use anyhow::{Context, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

type Cmd = fn(&[String]) -> anyhow::Result<()>;

const COMMANDS: &[(&str, &str, Cmd)] = &[
    ("version", "Print the workspace version, or `version set X.Y.Z[-pre]` to change it and refresh Cargo.lock", version),
    ("wasm", "cargo check --target wasm32-unknown-unknown for every crate in crates/ and the web app", wasm),
    ("deny", "Dependency licences, bans, sources and advisories (deny.toml; needs cargo-deny)", deny),
    ("ci", "fmt --check, clippy -D warnings, test, wasm, deny (stops at the first failure)", ci),
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((name, rest)) = args.split_first() else {
        usage();
        std::process::exit(2);
    };
    let Some((_, _, run)) = COMMANDS.iter().find(|(n, _, _)| n == name) else {
        eprintln!("unknown command `{name}`\n");
        usage();
        std::process::exit(2);
    };
    if let Err(e) = run(rest) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn usage() {
    eprintln!("usage: cargo xtask <command>\n");
    for (name, help, _) in COMMANDS {
        eprintln!("  {name:<10} {help}");
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}

fn cargo(args: &[&str]) -> anyhow::Result<()> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    eprintln!("$ cargo {}", args.join(" "));
    let status = Command::new(cargo).args(args).current_dir(root()).status().context("running cargo")?;
    if !status.success() {
        bail!("cargo {} failed ({status})", args.join(" "));
    }
    Ok(())
}

/// Line index and value of `version = "..."` inside `[workspace.package]`.
fn find_version(manifest: &str) -> Option<(usize, String)> {
    let mut in_pkg = false;
    for (i, line) in manifest.lines().enumerate() {
        let t = line.trim();
        if t.starts_with('[') {
            in_pkg = t == "[workspace.package]";
            continue;
        }
        if in_pkg && let Some(rest) = t.strip_prefix("version") {
            let value = rest.trim_start().strip_prefix('=')?.trim().trim_matches('"');
            return Some((i, value.to_string()));
        }
    }
    None
}

fn is_semver(v: &str) -> bool {
    let (core, pre) = v.split_once('-').map_or((v, None), |(c, p)| (c, Some(p)));
    let numeric = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) && (s == "0" || !s.starts_with('0'));
    let parts: Vec<&str> = core.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|p| numeric(p))
        && pre.is_none_or(|p| p.split('.').all(|id| !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')))
}

/// The manifest with the workspace version (at `line`) set to `new`, and the internal path
/// dependencies in [workspace.dependencies] that carry `current` moved along with it. Registry
/// dependencies that happen to share the version are left alone.
fn bump(manifest: &str, line: usize, current: &str, new: &str) -> String {
    let old_req = format!("version = \"{current}\"");
    let new_req = format!("version = \"{new}\"");
    let mut out: Vec<String> = Vec::new();
    for (i, l) in manifest.lines().enumerate() {
        if i == line {
            out.push(new_req.clone());
        } else if l.contains("path = \"") && l.contains(&old_req) {
            out.push(l.replace(&old_req, &new_req));
        } else {
            out.push(l.to_string());
        }
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}

fn version(args: &[String]) -> anyhow::Result<()> {
    let path = root().join("Cargo.toml");
    let manifest = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let (line, current) = find_version(&manifest).context("no version in [workspace.package]")?;
    match args {
        [] => {
            println!("{current}");
            Ok(())
        }
        [set, new] if set == "set" => {
            let new = new.trim_start_matches('v');
            if !is_semver(new) {
                bail!("`{new}` is not a version like 1.2.3 or 1.2.3-rc.1");
            }
            std::fs::write(&path, bump(&manifest, line, &current, new))?;
            cargo(&["update", "--workspace"])?;
            println!("{current} -> {new}");
            Ok(())
        }
        _ => bail!("usage: cargo xtask version [set X.Y.Z[-pre]]"),
    }
}

fn wasm(_: &[String]) -> anyhow::Result<()> {
    cargo(&["check", "--target", "wasm32-unknown-unknown", "-p", "aiassist-core", "-p", "aiassist-ui", "-p", "aiassist-web"])
}

/// Skipped with a hint when cargo-deny is missing, except in CI (`CI` set), where it is required.
fn deny(_: &[String]) -> anyhow::Result<()> {
    let installed = Command::new("cargo-deny").arg("--version").output().is_ok_and(|o| o.status.success());
    if !installed {
        if std::env::var_os("CI").is_some() {
            bail!("cargo-deny is required in CI: cargo install cargo-deny --locked");
        }
        eprintln!("deny: SKIPPED, cargo-deny is not installed (cargo install cargo-deny --locked)");
        return Ok(());
    }
    cargo(&["deny", "--log-level", "error", "check"])
}

fn ci(_: &[String]) -> anyhow::Result<()> {
    let steps: [(&str, &dyn Fn() -> anyhow::Result<()>); 5] = [
        ("fmt", &|| cargo(&["fmt", "--all", "--", "--check"])),
        ("clippy", &|| cargo(&["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"])),
        ("test", &|| cargo(&["test", "--workspace"])),
        ("wasm", &|| wasm(&[])),
        ("deny", &|| deny(&[])),
    ];
    for (i, (name, step)) in steps.iter().enumerate() {
        eprintln!("\n=== ci: {name} ===");
        if let Err(e) = step() {
            bail!("CI: {i} passed, FAILED at `{name}`: {e:#}");
        }
    }
    println!("\nCI: all {} steps passed", steps.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_workspace_version_only() {
        let m = "[package]\nversion = \"9.9.9\"\n[workspace.package]\nedition = \"2024\"\nversion = \"0.3.0\"\n";
        assert_eq!(find_version(m), Some((4, "0.3.0".into())));
    }

    #[test]
    fn bump_moves_internal_crates_only() {
        let m = "[workspace.package]\nversion = \"0.1.0\"\n[workspace.dependencies]\n\
                 aiassist-core = { path = \"crates/core\", version = \"0.1.0\" }\n\
                 other = { version = \"0.1.0\" }\n";
        let (line, current) = find_version(m).unwrap();
        let out = bump(m, line, &current, "0.2.0-rc.1");
        assert!(out.contains("version = \"0.2.0-rc.1\"\n[workspace.dependencies]"));
        assert!(out.contains("aiassist-core = { path = \"crates/core\", version = \"0.2.0-rc.1\" }"));
        assert!(out.contains("other = { version = \"0.1.0\" }"));
    }

    #[test]
    fn semver() {
        assert!(is_semver("1.2.3"));
        assert!(is_semver("0.2.0-rc.1"));
        assert!(!is_semver("1.2"));
        assert!(!is_semver("01.2.3"));
        assert!(!is_semver("1.2.3-"));
    }
}
