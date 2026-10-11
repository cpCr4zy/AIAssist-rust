//! The application state and its three tabs.
//!
//! Edits on the Settings tab change a working copy of the prompt list, which the Main tab already
//! uses; **Save Settings** stores it together with the API key. The theme and the selected prompt
//! are stored as soon as they change. Requests run in the background and report back through a
//! channel, so the window never blocks.

use std::sync::mpsc;

use aiassist_core::openrouter::{balance_request, completion_request, models_request, parse_balance, parse_completion, parse_models};
use aiassist_core::settings::title_exists;
use aiassist_core::{AiError, AppSettings, Balance, HttpRequest, HttpResponse, ModelInfo, PromptConfig, Theme};
use egui::{Key, KeyboardShortcut, Modifiers, RichText};

use crate::dialogs::{EditorAction, Message, MessageKind, ModelPicker, PickerAction, PromptEditor, right_aligned};
use crate::logging::LogBuffer;
use crate::services::{Http, Store};
use crate::theme;

/// How long "Copied!" replaces the Copy Result label.
const COPIED_FEEDBACK_SECS: f64 = 1.5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Main,
    Settings,
    Log,
}

/// Results of background requests, delivered to the UI thread.
enum Event {
    Completion { title: String, result: Result<String, AiError> },
    Balance(Result<Balance, AiError>),
    Models { generation: u64, result: Result<Vec<ModelInfo>, AiError> },
}

pub struct MainApp {
    store: Box<dyn Store>,
    http: Box<dyn Http>,
    logs: LogBuffer,
    events_tx: mpsc::Sender<Event>,
    events_rx: mpsc::Receiver<Event>,

    /// What is stored (prompts as of the last Save Settings).
    settings: AppSettings,
    /// The prompt list being edited; the Main tab uses it too.
    prompts: Vec<PromptConfig>,
    tab: Tab,

    // Main tab
    selected: Option<usize>,
    input: String,
    output: String,
    sending: bool,
    copied_until: Option<f64>,

    // Settings tab
    api_key_input: String,
    show_key: bool,
    checking_balance: bool,
    list_selection: Option<usize>,

    // Dialogs, drawn in this order (later ones on top)
    editor: Option<PromptEditor>,
    picker: Option<ModelPicker>,
    messages: Vec<Message>,
    models_generation: u64,
}

impl MainApp {
    /// Builds the app from the platform's storage and HTTP client, and the log buffer that
    /// [`crate::logging::install`] returned (or an empty one).
    pub fn new(cc: &eframe::CreationContext<'_>, store: Box<dyn Store>, http: Box<dyn Http>, logs: LogBuffer) -> Self {
        Self::with_context(&cc.egui_ctx, store, http, logs)
    }

    pub fn with_context(ctx: &egui::Context, mut store: Box<dyn Store>, http: Box<dyn Http>, logs: LogBuffer) -> Self {
        let settings = store.load();
        let api_key_input = store.api_key();
        theme::apply(ctx, settings.theme);
        let (events_tx, events_rx) = mpsc::channel();
        log::info!("AI Assistant started.");
        Self {
            store,
            http,
            logs,
            events_tx,
            events_rx,
            prompts: settings.prompts.clone(),
            selected: settings.selected_index(),
            settings,
            tab: Tab::Main,
            input: String::new(),
            output: String::new(),
            sending: false,
            copied_until: None,
            api_key_input,
            show_key: false,
            checking_balance: false,
            list_selection: None,
            editor: None,
            picker: None,
            messages: Vec::new(),
            models_generation: 0,
        }
    }

    pub fn tab(&self) -> Tab {
        self.tab
    }

    pub fn set_tab(&mut self, tab: Tab) {
        self.tab = tab;
    }

    pub fn input_mut(&mut self) -> &mut String {
        &mut self.input
    }

    pub fn output(&self) -> &str {
        &self.output
    }

    pub fn prompts(&self) -> &[PromptConfig] {
        &self.prompts
    }

    pub fn selected_prompt(&self) -> Option<&PromptConfig> {
        self.selected.and_then(|i| self.prompts.get(i))
    }

    fn message(&mut self, kind: MessageKind, title: &str, text: impl Into<String>) {
        self.messages.push(Message::new(kind, title, text));
    }

    /// Stores the settings, telling the user when that fails.
    fn persist(&mut self) {
        if let Err(e) = self.store.save(&self.settings) {
            log::error!("Error saving settings: {e}");
            self.message(MessageKind::Error, "Error", format!("Error saving settings: {e}"));
        }
    }

    /// Sends `request` in the background; `parse` and `wrap` turn the answer into an [`Event`].
    fn send<T: Send + 'static>(
        &self,
        ctx: &egui::Context,
        request: HttpRequest,
        parse: fn(&HttpResponse) -> Result<T, AiError>,
        wrap: impl FnOnce(Result<T, AiError>) -> Event + Send + 'static,
    ) {
        let tx = self.events_tx.clone();
        let ctx = ctx.clone();
        self.http.fetch(
            request,
            Box::new(move |answer| {
                let result = answer
                    .map_err(|e| {
                        log::error!("HTTP error while calling OpenRouter API: {e}");
                        AiError::Network(e)
                    })
                    .and_then(|response| parse(&response));
                // The window may be gone by now; then nobody needs the answer.
                if tx.send(wrap(result)).is_ok() {
                    ctx.request_repaint();
                }
            }),
        );
    }

    fn handle_events(&mut self) {
        while let Ok(event) = self.events_rx.try_recv() {
            match event {
                Event::Completion { title, result } => {
                    self.sending = false;
                    match result {
                        Ok(text) => {
                            log::info!("Response for '{title}' received ({} chars).", text.chars().count());
                            self.output = text;
                        }
                        Err(e) => {
                            log::warn!("Prompt '{title}' failed: {e}");
                            self.output = e.to_string();
                        }
                    }
                }
                Event::Balance(result) => {
                    self.checking_balance = false;
                    match result {
                        Ok(balance) => {
                            log::info!("Balance check succeeded.");
                            self.message(MessageKind::Info, "OpenRouter Balance", balance.to_string());
                        }
                        Err(e) => {
                            log::warn!("Balance check failed: {e}");
                            self.message(MessageKind::Error, "Balance Error", e.to_string());
                        }
                    }
                }
                Event::Models { generation, result } => {
                    let Some(picker) = self.picker.as_mut().filter(|p| p.generation == generation) else {
                        continue;
                    };
                    let models = result.unwrap_or_default();
                    log::debug!("Fetched {} models.", models.len());
                    picker.loaded(models);
                }
            }
        }
    }

    // --- Main tab -------------------------------------------------------------------------

    fn select_prompt(&mut self, index: Option<usize>) {
        self.selected = index;
        let Some(title) = self.selected_prompt().map(|p| p.title.clone()) else {
            return;
        };
        if self.settings.selected_prompt != title {
            log::debug!("Selected prompt '{title}' saved.");
            self.settings.selected_prompt = title;
            self.persist();
        }
    }

    fn start_send(&mut self, ctx: &egui::Context) {
        let Some(prompt) = self.selected_prompt().cloned() else {
            self.message(MessageKind::Warning, "No Prompt", "No prompts configured. Add one in Settings.");
            return;
        };
        if self.input.trim().is_empty() {
            self.message(MessageKind::Warning, "Empty Text", "Please enter some text to process.");
            return;
        }
        let request = match completion_request(&self.store.api_key(), &prompt.system_prompt, &self.input, &prompt.model) {
            Ok(request) => request,
            Err(e) => {
                log::warn!("Prompt '{}' failed: {e}", prompt.title);
                self.output = e.to_string();
                return;
            }
        };
        log::info!("Sending prompt '{}' ({} chars) to OpenRouter.", prompt.title, self.input.chars().count());
        log::debug!("Sending request to OpenRouter. Model={}", prompt.model);
        self.sending = true;
        let title = prompt.title;
        self.send(ctx, request, parse_completion, move |result| Event::Completion { title, result });
    }

    fn main_tab(&mut self, ui: &mut egui::Ui) {
        let send_shortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Enter);
        if !self.sending && self.messages.is_empty() && ui.input_mut(|i| i.consume_shortcut(&send_shortcut)) {
            self.start_send(ui.ctx());
        }

        ui.horizontal(|ui| {
            ui.label("Prompt:");
            let current = self.selected_prompt().map_or_else(String::new, |p| p.title.clone());
            let mut choice = self.selected;
            ui.add_enabled_ui(!self.sending, |ui| {
                egui::ComboBox::from_id_salt("prompt").selected_text(current).width(ui.available_width()).truncate().show_ui(ui, |ui| {
                    for (i, prompt) in self.prompts.iter().enumerate() {
                        ui.selectable_value(&mut choice, Some(i), &prompt.title);
                    }
                });
            });
            if choice != self.selected {
                self.select_prompt(choice);
            }
        });

        // Two labels and two button rows besides the text boxes, which share the rest.
        let spacing = ui.spacing().item_spacing.y;
        let row = ui.spacing().interact_size.y;
        let box_height = ((ui.available_height() - 4.0 * row - 8.0 * spacing) / 2.0).max(60.0);

        let label = ui.label("Your text:");
        let input = egui::TextEdit::multiline(&mut self.input).hint_text("Type or paste the text to process…");
        ui.add_sized([ui.available_width(), box_height], input).labelled_by(label.id);

        right_aligned(ui, |ui| {
            let text = if self.sending { "Sending…" } else { "Send" };
            let send = ui.add_enabled(!self.sending, egui::Button::new(text).min_size(egui::vec2(100.0, 0.0)));
            if send.on_hover_text("Send the text with the selected prompt (Ctrl+Enter)").clicked() {
                self.start_send(ui.ctx());
            }
        });

        let label = ui.label("Result:");
        let mut output = self.output.as_str();
        // Read-only but selectable. egui draws read-only fields without a fill, so give it the input
        // box's look explicitly.
        let filled = egui::Frame::new()
            .fill(ui.visuals().text_edit_bg_color())
            .corner_radius(ui.visuals().widgets.inactive.corner_radius)
            .stroke(ui.visuals().widgets.inactive.bg_stroke)
            .inner_margin(4);
        let result = egui::TextEdit::multiline(&mut output).hint_text("The result will appear here…").frame(filled);
        ui.add_sized([ui.available_width(), box_height], result).labelled_by(label.id);

        right_aligned(ui, |ui| {
            let copied = self.copied_until.is_some();
            let text = if copied { "Copied!" } else { "Copy Result" };
            let copy = ui.add_enabled(!self.output.trim().is_empty(), egui::Button::new(text).min_size(egui::vec2(100.0, 0.0)));
            if copy.on_hover_text("Copy the result to the clipboard").clicked() {
                ui.ctx().copy_text(self.output.clone());
                log::info!("Result copied to clipboard ({} chars).", self.output.chars().count());
                self.copied_until = Some(ui.input(|i| i.time) + COPIED_FEEDBACK_SECS);
            }
        });
    }

    // --- Settings tab ---------------------------------------------------------------------

    fn start_balance_check(&mut self, ctx: &egui::Context) {
        let typed = self.api_key_input.trim().to_owned();
        let api_key = if typed.is_empty() { self.store.api_key() } else { typed };
        let Ok(request) = balance_request(&api_key) else {
            self.message(MessageKind::Warning, "No API Key", "Enter your OpenRouter API key first.");
            return;
        };
        self.checking_balance = true;
        self.send(ctx, request, parse_balance, Event::Balance);
    }

    fn set_theme(&mut self, ctx: &egui::Context, theme: Theme) {
        theme::apply(ctx, theme);
        self.settings.theme = theme;
        self.persist();
    }

    fn save_settings(&mut self) {
        if let Err(e) = self.store.set_api_key(&self.api_key_input) {
            log::error!("Error saving the API key: {e}");
            self.message(MessageKind::Error, "Error", format!("Error saving settings: {e}"));
            return;
        }
        self.settings.prompts = self.prompts.clone();
        self.settings.drop_stale_selection();
        match self.store.save(&self.settings) {
            Ok(()) => self.message(MessageKind::Info, "Success", "Settings saved successfully!"),
            Err(e) => {
                log::error!("Error saving settings: {e}");
                self.message(MessageKind::Error, "Error", format!("Error saving settings: {e}"));
            }
        }
    }

    fn edit_selected(&mut self) {
        match self.list_selection.and_then(|i| self.prompts.get(i).map(|p| (i, p))) {
            Some((i, prompt)) => self.editor = Some(PromptEditor::edit(i, prompt)),
            None => self.message(MessageKind::Warning, "No Selection", "Please select a prompt to edit."),
        }
    }

    fn remove_selected(&mut self) {
        let Some(index) = self.list_selection.filter(|&i| i < self.prompts.len()) else {
            self.message(MessageKind::Warning, "No Selection", "Please select a prompt to remove.");
            return;
        };
        let removed = self.prompts.remove(index);
        self.list_selection = None;
        // As in a refilled combo box: the selection moves to the first prompt if it was removed.
        self.selected = match self.selected {
            Some(s) if s == index => (!self.prompts.is_empty()).then_some(0),
            Some(s) if s > index => Some(s - 1),
            other => other,
        };
        log::info!("Removed prompt '{}'.", removed.title);
    }

    fn settings_tab(&mut self, ui: &mut egui::Ui) {
        let label = ui.label("OpenRouter API Key:");
        ui.horizontal(|ui| {
            let field_width = (ui.available_width() - 150.0).max(80.0);
            let field = egui::TextEdit::singleline(&mut self.api_key_input)
                .password(!self.show_key)
                .hint_text("Enter your OpenRouter API key")
                .desired_width(field_width);
            ui.add(field).labelled_by(label.id);
            let (text, tip) = if self.show_key { ("Hide", "Hide API Key") } else { ("Show", "Show API Key") };
            if ui.button(text).on_hover_text(tip).clicked() {
                self.show_key = !self.show_key;
            }
            let balance_text = if self.checking_balance { "Checking…" } else { "Balance" };
            let balance = ui.add_enabled(!self.checking_balance, egui::Button::new(balance_text));
            if balance.on_hover_text("Check OpenRouter account balance").clicked() {
                self.start_balance_check(ui.ctx());
            }
        });
        if let Some(note) = self.store.api_key_note() {
            ui.label(RichText::new(note).small().weak());
        }

        ui.add_space(8.0);
        ui.label("Theme:");
        let mut theme = self.settings.theme;
        egui::ComboBox::from_id_salt("theme").selected_text(theme.label()).width(ui.available_width()).show_ui(ui, |ui| {
            for option in Theme::ALL {
                ui.selectable_value(&mut theme, option, option.label());
            }
        });
        if theme != self.settings.theme {
            self.set_theme(ui.ctx(), theme);
        }

        ui.add_space(8.0);
        ui.label("Prompts:");
        let spacing = ui.spacing().item_spacing.y;
        let row = ui.spacing().interact_size.y;
        let list_height = (ui.available_height() - 2.0 * row - 6.0 * spacing - 12.0).max(60.0);
        let mut edit_index = None;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::vertical().id_salt("prompt_list").auto_shrink(false).max_height(list_height).show(ui, |ui| {
                for (i, prompt) in self.prompts.iter().enumerate() {
                    let item = ui.selectable_label(self.list_selection == Some(i), &prompt.title);
                    if item.clicked() {
                        self.list_selection = Some(i);
                    }
                    if item.double_clicked() {
                        edit_index = Some(i);
                    }
                }
            });
        });
        if let Some(i) = edit_index {
            self.list_selection = Some(i);
            self.edit_selected();
        }

        ui.horizontal(|ui| {
            if ui.button("Add").clicked() {
                self.editor = Some(PromptEditor::add());
            }
            if ui.button("Edit").clicked() {
                self.edit_selected();
            }
            if ui.button("Remove").clicked() {
                self.remove_selected();
            }
        });
        if ui.add_sized([ui.available_width(), row], egui::Button::new("Save Settings")).clicked() {
            self.save_settings();
        }
    }

    // --- Log tab --------------------------------------------------------------------------

    fn log_tab(&mut self, ui: &mut egui::Ui) {
        let lines = self.logs.snapshot();
        let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
        let spacing = ui.spacing().item_spacing.y;
        let height = ui.available_height() - ui.spacing().interact_size.y - 2.0 * spacing;
        egui::ScrollArea::both().id_salt("log").auto_shrink(false).stick_to_bottom(true).max_height(height).show_rows(
            ui,
            row_height,
            lines.len(),
            |ui, range| {
                for line in lines.get(range).unwrap_or_default() {
                    let (level, color) = match line.level {
                        log::Level::Error => ("ERROR", ui.visuals().error_fg_color),
                        log::Level::Warn => ("WARNING", ui.visuals().warn_fg_color),
                        log::Level::Info => ("INFO", ui.visuals().text_color()),
                        log::Level::Debug => ("DEBUG", ui.visuals().weak_text_color()),
                        log::Level::Trace => ("TRACE", ui.visuals().weak_text_color()),
                    };
                    let text = RichText::new(format!("{} {level} {}", line.time, line.text)).monospace().color(color);
                    ui.add(egui::Label::new(text).extend());
                }
            },
        );
        right_aligned(ui, |ui| {
            if ui.button("Clear Log").clicked() {
                self.logs.clear();
            }
        });
    }

    // --- Dialogs --------------------------------------------------------------------------

    fn open_model_picker(&mut self, ctx: &egui::Context, current: &str) {
        let mut picker = ModelPicker::new(current);
        self.models_generation = self.models_generation.wrapping_add(1);
        picker.start_loading(self.models_generation);
        match models_request(&self.store.api_key()) {
            Ok(request) => {
                log::debug!("Fetching available models from OpenRouter.");
                let generation = self.models_generation;
                self.send(ctx, request, parse_models, move |result| Event::Models { generation, result });
            }
            Err(e) => {
                log::error!("Cannot load models: {e}");
                picker.loaded(Vec::new());
            }
        }
        self.picker = Some(picker);
    }

    fn accept_editor(&mut self, editor: PromptEditor) -> Option<PromptEditor> {
        let prompt = match editor.validate() {
            Ok(prompt) => prompt,
            Err(problem) => {
                self.message(MessageKind::Warning, "Validation Error", problem);
                return Some(editor);
            }
        };
        if title_exists(&self.prompts, &prompt.title, editor.index) {
            self.message(MessageKind::Warning, "Duplicate Title", format!("A prompt named '{}' already exists.", prompt.title));
            return Some(editor);
        }
        match editor.index.filter(|&i| i < self.prompts.len()) {
            Some(index) => {
                log::info!("Updated prompt '{}'.", prompt.title);
                let title = prompt.title.clone();
                if let Some(slot) = self.prompts.get_mut(index) {
                    *slot = prompt;
                }
                // The edited prompt stays selected under its new name, remembered at once.
                if self.selected == Some(index) && self.settings.selected_prompt != title {
                    self.settings.selected_prompt = title;
                    self.persist();
                }
            }
            None => {
                log::info!("Added prompt '{}'.", prompt.title);
                self.prompts.push(prompt);
                if self.selected.is_none() {
                    self.selected = Some(0);
                }
            }
        }
        None
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(mut editor) = self.editor.take() {
            self.editor = match editor.show(ctx) {
                EditorAction::None => Some(editor),
                EditorAction::Cancel => None,
                EditorAction::SelectModel => {
                    let current = editor.model.clone();
                    self.open_model_picker(ctx, &current);
                    Some(editor)
                }
                EditorAction::Accept => self.accept_editor(editor),
            };
        }
        if let Some(mut picker) = self.picker.take() {
            match picker.show(ctx) {
                PickerAction::None => self.picker = Some(picker),
                PickerAction::Cancel => {}
                PickerAction::Refresh => {
                    let current = picker.selected.and_then(|i| picker.models.get(i)).map_or(picker.current_id, |m| m.id.clone());
                    self.open_model_picker(ctx, &current);
                }
                PickerAction::Accept(id) => {
                    if let Some(editor) = self.editor.as_mut() {
                        editor.model = id;
                    }
                }
            }
        }
        if let Some(message) = self.messages.first()
            && message.show(ctx)
        {
            self.messages.remove(0);
        }
    }
}

impl eframe::App for MainApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_events();
        let now = ui.input(|i| i.time);
        if let Some(until) = self.copied_until {
            if now >= until {
                self.copied_until = None;
            } else {
                ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(until - now));
            }
        }

        egui::Panel::top("tabs").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                for (tab, name) in [(Tab::Main, "Main"), (Tab::Settings, "Settings"), (Tab::Log, "Log")] {
                    ui.selectable_value(&mut self.tab, tab, name);
                }
            });
            ui.add_space(2.0);
        });
        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::Main => self.main_tab(ui),
            Tab::Settings => self.settings_tab(ui),
            Tab::Log => self.log_tab(ui),
        });
        self.dialogs(ui.ctx());
    }
}
