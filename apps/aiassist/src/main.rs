//! AI Assistant desktop app.
//!
//! Usage: `aiassist [--version] [files…]`
//!
//! Environment: `RUST_LOG` (log levels), `AIASSIST_RENDERER` (`gl` starts with OpenGL, `wgpu` never
//! falls back), `WGPU_BACKEND` / `WGPU_POWER_PREF` (wgpu's own choices).

// No console window behind the GUI in Windows release builds.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

/// Wayland app id and Flatpak id: must match packaging/linux/io.github.cpcr4zy.aiassist.desktop.
const APP_ID: &str = "io.github.cpcr4zy.aiassist";
const APP_NAME: &str = "AI Assistant";
const APP_ICON_PNG: &[u8] = include_bytes!("../../../assets/app-icon/icon-256.png");

fn main() -> eframe::Result {
    // This app's crates (`<crate>`, `<crate>_ui`, `<crate>_core`) at info, everything else at warn.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(concat!(env!("CARGO_CRATE_NAME"), "=info,warn"))).init();
    // Last-resort guard: every panic lands in the log, with a backtrace when RUST_BACKTRACE is set.
    std::panic::set_hook(Box::new(|info| {
        let trace = std::backtrace::Backtrace::capture();
        log::error!("internal error: {info}\n{trace}");
    }));

    let mut files = Vec::new();
    for arg in std::env::args_os().skip(1) {
        if arg == "--version" {
            println!("aiassist {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        files.push(PathBuf::from(arg));
    }

    let choice = renderer_choice(std::env::var("AIASSIST_RENDERER").ok().as_deref());
    let first = if choice == RendererChoice::Gl { eframe::Renderer::Glow } else { eframe::Renderer::Wgpu };
    // Set once the app is created, i.e. after the renderer has started.
    let started = Rc::new(Cell::new(false));
    let result = eframe::run_native(APP_NAME, native_options(first), app_creator(files.clone(), Rc::clone(&started)));
    match result {
        // Old GPUs, VMs and remote desktops often can't give wgpu a device; OpenGL usually still works.
        Err(e @ eframe::Error::Wgpu(_)) if choice == RendererChoice::Auto && !started.get() => {
            log::error!("the GPU renderer (wgpu) didn't start: {e}. Starting with OpenGL instead; set AIASSIST_RENDERER=gl to skip wgpu.");
            eframe::run_native(APP_NAME, native_options(eframe::Renderer::Glow), app_creator(files, started))
        }
        other => other,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RendererChoice {
    /// wgpu, then OpenGL if wgpu can't start.
    Auto,
    /// wgpu only: a failure is reported, not worked around.
    Wgpu,
    /// OpenGL only.
    Gl,
}

fn renderer_choice(value: Option<&str>) -> RendererChoice {
    match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("wgpu") => RendererChoice::Wgpu,
        Some("gl" | "opengl" | "glow") => RendererChoice::Gl,
        _ => RendererChoice::Auto,
    }
}

fn native_options(renderer: eframe::Renderer) -> eframe::NativeOptions {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(APP_NAME)
        .with_inner_size([1200.0, 800.0])
        .with_min_inner_size([640.0, 420.0])
        .with_drag_and_drop(true)
        .with_app_id(APP_ID);
    // Dock, taskbar and launcher icon when running unbundled (cargo run, the tar.gz).
    match eframe::icon_data::from_png_bytes(APP_ICON_PNG) {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(e) => log::warn!("app icon: {e}"),
    }
    let persistence_path = settings_dir().map(|d| d.join("app.ron"));
    // Set explicitly, so the renderer never depends on which one eframe defaults to.
    let mut native = eframe::NativeOptions { viewport, persistence_path, renderer, ..Default::default() };
    // Direct3D 12 with an OpenGL fallback on Windows, unless WGPU_BACKEND says otherwise.
    if cfg!(target_os = "windows")
        && std::env::var_os("WGPU_BACKEND").is_none()
        && let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut native.wgpu_options.wgpu_setup
    {
        setup.instance_descriptor.backends = eframe::wgpu::Backends::DX12 | eframe::wgpu::Backends::GL;
    }
    native
}

fn app_creator<'a>(files: Vec<PathBuf>, started: Rc<Cell<bool>>) -> eframe::AppCreator<'a> {
    Box::new(move |cc| {
        started.set(true);
        let mut app = aiassist_ui::MainApp::new(cc);
        for file in &files {
            app.open_path(file);
        }
        Ok(Box::new(app))
    })
}

/// Where settings live: `portable.txt` beside the executable makes it `aiassist-data\` there;
/// otherwise the platform's per-user data folder.
fn settings_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(PathBuf::from))
        && dir.join("portable.txt").is_file()
    {
        return Some(dir.join("aiassist-data"));
    }
    let env = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    if cfg!(target_os = "windows") {
        env("APPDATA").map(|d| d.join(APP_NAME).join("data"))
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|d| d.join("Library/Application Support").join(APP_NAME))
    } else {
        env("XDG_DATA_HOME").or_else(|| env("HOME").map(|h| h.join(".local/share"))).map(|d| d.join("aiassist"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_choice_reads_the_environment_value() {
        assert_eq!(renderer_choice(None), RendererChoice::Auto);
        assert_eq!(renderer_choice(Some(" GL ")), RendererChoice::Gl);
        assert_eq!(renderer_choice(Some("wgpu")), RendererChoice::Wgpu);
        assert_eq!(renderer_choice(Some("vulkan")), RendererChoice::Auto);
    }

    #[test]
    fn app_icon_decodes() {
        assert!(eframe::icon_data::from_png_bytes(APP_ICON_PNG).is_ok());
    }
}
