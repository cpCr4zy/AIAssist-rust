//! The egui shell of AI Assistant. Shared by `apps/aiassist` (desktop) and `apps/aiassist-web` (wasm32).

use serde::{Deserialize, Serialize};

/// Key under which the settings are stored by eframe's persistence.
const STORAGE_KEY: &str = "aiassist";

/// Settings that survive a restart (eframe persistence: `app.ron` on the desktop, local storage on the web).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub dark: bool,
}

/// The application state.
#[derive(Default)]
pub struct MainApp {
    pub settings: Settings,
    pub status: String,
    pub summary: Option<aiassist_core::Summary>,
}

impl MainApp {
    /// Builds the app, restoring saved settings when there are any.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self::default();
        if let Some(json) = cc.storage.and_then(|s| s.get_string(STORAGE_KEY)) {
            // Settings files are untrusted input: a broken one falls back to the defaults.
            match serde_json::from_str(&json) {
                Ok(settings) => app.settings = settings,
                Err(e) => log::warn!("ignoring unreadable settings: {e}"),
            }
        }
        apply_theme(&cc.egui_ctx, app.settings.dark);
        app
    }

    /// Opens a file by bytes (from a dialog, the command line, drag and drop or the web).
    pub fn open_bytes(&mut self, name: &str, data: &[u8]) {
        match aiassist_core::summarize(data) {
            Ok(summary) => {
                self.status = format!("{name}: {} bytes, {} lines", summary.bytes, summary.lines);
                self.summary = Some(summary);
            }
            Err(e) => {
                self.status = format!("{name}: {e}");
                self.summary = None;
            }
        }
    }

    /// Opens a file by path (desktop only). Reads at most one byte past the engine's limit, so a
    /// huge file is refused without loading it into memory.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_path(&mut self, path: &std::path::Path) {
        use std::io::Read as _;
        let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
        let limit = u64::try_from(aiassist_core::MAX_INPUT).unwrap_or(u64::MAX).saturating_add(1);
        let mut data = Vec::new();
        match std::fs::File::open(path).and_then(|f| f.take(limit).read_to_end(&mut data)) {
            Ok(_) => self.open_bytes(&name, &data),
            Err(e) => self.status = format!("{name}: {e}"),
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("menu_bar").show(ui, |ui| {
            ui.horizontal(|ui| {
                // The browser has no file paths and no window to close: no File menu there.
                #[cfg(not(target_arch = "wasm32"))]
                ui.menu_button("File", |ui| {
                    if ui.button("Open…").clicked() {
                        // Synchronous dialogs run on the UI thread, which macOS requires.
                        if let Some(path) = rfd::FileDialog::new().pick_file() {
                            self.open_path(&path);
                        }
                    }
                    if ui.button("Quit").clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui.checkbox(&mut self.settings.dark, "Dark theme").changed() {
                        apply_theme(ui.ctx(), self.settings.dark);
                    }
                });
            });
        });
    }
}

fn apply_theme(ctx: &egui::Context, dark: bool) {
    ctx.set_visuals(if dark { egui::Visuals::dark() } else { egui::Visuals::light() });
}

impl eframe::App for MainApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Files dropped on the window. On the web they can only be read asynchronously
        // (`DroppedFile::bytes_async`), so the browser build leaves drops to the host page.
        #[cfg(not(target_arch = "wasm32"))]
        for file in ui.ctx().input(|i| i.raw.dropped_files.clone()) {
            self.open_path(file.path());
        }
        self.menu_bar(ui);
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("AI Assistant");
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
            ui.separator();
            if self.status.is_empty() {
                if cfg!(target_arch = "wasm32") {
                    ui.label("Running in the browser.");
                } else {
                    ui.label("Open a file or drop one on the window.");
                }
            } else {
                ui.label(&self.status);
            }
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        match serde_json::to_string(&self.settings) {
            Ok(json) => storage.set_string(STORAGE_KEY, json),
            Err(e) => log::warn!("settings not saved: {e}"),
        }
    }
}
