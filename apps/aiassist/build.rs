//! Windows only: embed the app icon, version info (VERSIONINFO) and a per-monitor-DPI manifest
//! into `aiassist.exe`, so it shows in Explorer, the taskbar and Alt-Tab and renders sharply.
//!
//! On every other target this does nothing. A missing resource compiler is a warning, so a
//! cross-compile still links, unless `AIASSIST_REQUIRE_WINRES=1` turns it into an error (CI, releases).

const WINDOWS_MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0" xmlns:asmv3="urn:schemas-microsoft-com:asm.v3">
  <asmv3:application>
    <asmv3:windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2, PerMonitor</dpiAwareness>
    </asmv3:windowsSettings>
  </asmv3:application>
</assembly>
"#;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/app-icon/aiassist.ico");
    println!("cargo:rerun-if-env-changed=AIASSIST_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/app-icon/aiassist.ico")
        .set_manifest(WINDOWS_MANIFEST)
        .set("ProductName", "AI Assistant")
        .set("FileDescription", "AI Assistant")
        .set("LegalCopyright", "Copyright (c) the AI Assistant contributors. MIT OR Apache-2.0.")
        .set("OriginalFilename", "aiassist.exe")
        .set("InternalName", "aiassist");
    if let Err(e) = res.compile() {
        if std::env::var_os("AIASSIST_REQUIRE_WINRES").is_some() {
            println!("cargo::error=embedding Windows resources failed: {e}");
            return;
        }
        println!("cargo:warning=aiassist.exe built without icon/version resources: {e}");
    }
}
