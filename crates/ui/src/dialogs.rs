//! Modal dialogs: message boxes, the prompt editor and the model picker. Each `show` draws one
//! frame and reports what the user did; [`crate::MainApp`] acts on it.

use aiassist_core::format::{format_context, format_price, model_label};
use aiassist_core::{ModelInfo, PromptConfig};
use egui::{Align, Id, Layout, RichText};
use egui_extras::{Column, TableBuilder};

/// Lays out one row of widgets from the right edge. (A plain `with_layout(right_to_left)` would
/// centre them in, and take up, all the remaining height.)
pub fn right_aligned<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<R> {
    let size = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
    ui.allocate_ui_with_layout(size, Layout::right_to_left(Align::Center), add)
}

/// Dialog width: `wanted`, but never wider than the window allows.
fn fit_width(ctx: &egui::Context, wanted: f32) -> f32 {
    wanted.min(ctx.content_rect().width() - 48.0).max(200.0)
}

/// Dialog height: `wanted`, but never taller than the window allows.
fn fit_height(ctx: &egui::Context, wanted: f32) -> f32 {
    wanted.min(ctx.content_rect().height() - 64.0).max(160.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageKind {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug)]
pub struct Message {
    pub kind: MessageKind,
    pub title: String,
    pub text: String,
}

impl Message {
    pub fn new(kind: MessageKind, title: &str, text: impl Into<String>) -> Self {
        Self { kind, title: title.to_owned(), text: text.into() }
    }

    /// Draws the message; `true` once it is dismissed.
    pub fn show(&self, ctx: &egui::Context) -> bool {
        let response = egui::Modal::new(Id::new("message_box")).show(ctx, |ui| {
            ui.set_width(fit_width(ctx, 380.0));
            let title = RichText::new(&self.title).strong().size(16.0);
            ui.label(match self.kind {
                MessageKind::Info => title,
                MessageKind::Warning => title.color(ui.visuals().warn_fg_color),
                MessageKind::Error => title.color(ui.visuals().error_fg_color),
            });
            ui.add_space(6.0);
            ui.label(&self.text);
            ui.add_space(8.0);
            right_aligned(ui, |ui| {
                let ok = ui.add(egui::Button::new("OK").min_size(egui::vec2(80.0, 0.0)));
                ok.request_focus();
                ok.clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter))
            })
            .inner
        });
        response.inner || response.should_close()
    }
}

/// The prompt being added or edited.
pub struct PromptEditor {
    /// Index in the prompt list being edited; `None` when adding.
    pub index: Option<usize>,
    pub title: String,
    pub system_prompt: String,
    pub model: String,
}

pub enum EditorAction {
    None,
    Cancel,
    SelectModel,
    /// OK was pressed: the fields still need validating.
    Accept,
}

impl PromptEditor {
    pub fn add() -> Self {
        Self { index: None, title: String::new(), system_prompt: String::new(), model: String::new() }
    }

    pub fn edit(index: usize, prompt: &PromptConfig) -> Self {
        Self { index: Some(index), title: prompt.title.clone(), system_prompt: prompt.system_prompt.clone(), model: prompt.model.clone() }
    }

    /// The prompt, or the validation message to show. The title is trimmed; the system prompt is
    /// kept exactly as typed, because trailing spaces matter.
    pub fn validate(&self) -> Result<PromptConfig, &'static str> {
        if self.title.trim().is_empty() {
            return Err("Please enter a title.");
        }
        if self.system_prompt.trim().is_empty() {
            return Err("Please enter a system prompt.");
        }
        Ok(PromptConfig { title: self.title.trim().to_owned(), system_prompt: self.system_prompt.clone(), model: self.model.trim().to_owned() })
    }

    pub fn show(&mut self, ctx: &egui::Context) -> EditorAction {
        let response = egui::Modal::new(Id::new("prompt_editor")).show(ctx, |ui| {
            ui.set_width(fit_width(ctx, 520.0));
            ui.heading(if self.index.is_some() { "Edit Prompt" } else { "Add Prompt" });
            ui.add_space(6.0);

            let label = ui.label("Titel:");
            ui.add(egui::TextEdit::singleline(&mut self.title).desired_width(f32::INFINITY)).labelled_by(label.id);
            ui.add_space(4.0);

            let label = ui.label("System prompt:");
            egui::ScrollArea::vertical().id_salt("system_prompt").max_height(fit_height(ctx, 260.0)).show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.system_prompt)
                        .hint_text("e.g. Correct grammar and syntax errors. Text for correction: ")
                        .desired_rows(8)
                        .desired_width(f32::INFINITY),
                )
                .labelled_by(label.id);
            });
            ui.add_space(4.0);

            ui.label("Model:");
            let model_text = if self.model.is_empty() { "Select Model…" } else { self.model.as_str() };
            let select = ui
                .add_sized([ui.available_width(), ui.spacing().interact_size.y], egui::Button::new(model_text).truncate())
                .on_hover_text("Choose the model from the OpenRouter catalogue");
            ui.add_space(8.0);
            ui.separator();

            let mut action = if select.clicked() { EditorAction::SelectModel } else { EditorAction::None };
            right_aligned(ui, |ui| {
                if ui.add(egui::Button::new("OK").min_size(egui::vec2(80.0, 0.0))).clicked() {
                    action = EditorAction::Accept;
                }
                if ui.add(egui::Button::new("Cancel").min_size(egui::vec2(80.0, 0.0))).clicked() {
                    action = EditorAction::Cancel;
                }
            });
            action
        });
        if response.should_close() { EditorAction::Cancel } else { response.inner }
    }
}

/// The model catalogue, for choosing a prompt's model.
pub struct ModelPicker {
    /// Answers from older loads (before a refresh) are ignored.
    pub generation: u64,
    pub loading: bool,
    pub status: String,
    pub models: Vec<ModelInfo>,
    /// Index into `models`.
    pub selected: Option<usize>,
    pub filter: String,
    /// The prompt's model when the picker opened; selected once the catalogue arrives.
    pub current_id: String,
    scroll_to_selected: bool,
}

pub enum PickerAction {
    None,
    Cancel,
    Refresh,
    Accept(String),
}

impl ModelPicker {
    pub fn new(current_id: &str) -> Self {
        Self {
            generation: 0,
            loading: false,
            status: String::new(),
            models: Vec::new(),
            selected: None,
            filter: String::new(),
            current_id: current_id.to_owned(),
            scroll_to_selected: false,
        }
    }

    pub fn start_loading(&mut self, generation: u64) {
        self.generation = generation;
        self.loading = true;
        "Loading models…".clone_into(&mut self.status);
        self.models.clear();
        self.selected = None;
    }

    /// The catalogue arrived (empty on failure).
    pub fn loaded(&mut self, models: Vec<ModelInfo>) {
        self.loading = false;
        self.status = if models.is_empty() { "No models available or no API key set.".to_owned() } else { format!("{} models:", models.len()) };
        self.selected = models.iter().position(|m| m.id == self.current_id);
        self.scroll_to_selected = self.selected.is_some();
        self.models = models;
    }

    pub fn show(&mut self, ctx: &egui::Context) -> PickerAction {
        let response = egui::Modal::new(Id::new("model_picker")).show(ctx, |ui| self.contents(ctx, ui));
        if response.should_close() { PickerAction::Cancel } else { response.inner }
    }

    fn contents(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) -> PickerAction {
        let width = fit_width(ctx, 760.0);
        let height = fit_height(ctx, 560.0);
        ui.set_width(width);
        ui.heading("Select AI Model");
        let mut action = PickerAction::None;

        ui.horizontal(|ui| {
            ui.label(&self.status);
            right_aligned(ui, |ui| {
                if ui.add_enabled(!self.loading, egui::Button::new("Refresh")).on_hover_text("Refresh models list").clicked() {
                    action = PickerAction::Refresh;
                }
                ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Filter…").desired_width(180.0));
            });
        });

        let filter = self.filter.trim().to_lowercase();
        let visible: Vec<usize> = (0..self.models.len()).filter(|&i| self.models.get(i).is_some_and(|m| m.matches(&filter))).collect();
        let row_height = ui.text_style_height(&egui::TextStyle::Body) + 4.0;
        let table_height = (height - 190.0).max(120.0);

        ui.push_id("models_table", |ui| {
            let mut table = TableBuilder::new(ui)
                .striped(true)
                .sense(egui::Sense::click())
                .min_scrolled_height(table_height)
                .max_scroll_height(table_height)
                .column(Column::remainder().at_least(200.0).clip(true))
                .column(Column::auto().at_least(90.0))
                .column(Column::auto().at_least(90.0));
            if self.scroll_to_selected
                && let Some(row) = self.selected.and_then(|s| visible.iter().position(|&i| i == s))
            {
                table = table.scroll_to_row(row, Some(Align::Center));
                self.scroll_to_selected = false;
            }
            table
                .header(row_height, |mut header| {
                    for title in ["Model", "Prompt Price", "Context Length"] {
                        header.col(|ui| {
                            ui.strong(title);
                        });
                    }
                })
                .body(|body| {
                    body.rows(row_height, visible.len(), |mut row| {
                        let Some((index, model)) = visible.get(row.index()).and_then(|&i| self.models.get(i).map(|m| (i, m))) else {
                            return;
                        };
                        row.set_selected(self.selected == Some(index));
                        let label = model_label(model);
                        row.col(|ui| {
                            ui.add(egui::Label::new(&label).truncate().selectable(false));
                        });
                        row.col(|ui| {
                            ui.add(egui::Label::new(format_price(model.prompt_price)).selectable(false));
                        });
                        row.col(|ui| {
                            ui.add(egui::Label::new(format_context(model.context_length)).selectable(false));
                        });
                        let response = row.response();
                        if response.clicked() {
                            self.selected = Some(index);
                        }
                        if response.double_clicked() {
                            action = PickerAction::Accept(model.id.clone());
                        }
                    });
                });
        });

        ui.add_space(6.0);
        egui::ScrollArea::vertical().id_salt("model_description").max_height(80.0).auto_shrink([false, true]).show(ui, |ui| {
            if let Some(model) = self.selected.and_then(|i| self.models.get(i)) {
                ui.strong(model_label(model));
                if !model.description.is_empty() {
                    ui.label(&model.description);
                }
            }
        });
        ui.separator();
        right_aligned(ui, |ui| {
            let chosen = self.selected.and_then(|i| self.models.get(i)).map(|m| m.id.clone());
            if ui.add_enabled(chosen.is_some(), egui::Button::new("OK").min_size(egui::vec2(80.0, 0.0))).clicked()
                && let Some(id) = chosen
            {
                action = PickerAction::Accept(id);
            }
            if ui.add(egui::Button::new("Cancel").min_size(egui::vec2(80.0, 0.0))).clicked() {
                action = PickerAction::Cancel;
            }
        });
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_trims_the_title_but_not_the_prompt() {
        let mut e = PromptEditor::add();
        assert_eq!(e.validate().unwrap_err(), "Please enter a title.");
        e.title = "  Fix  ".into();
        e.system_prompt = "\n".into();
        assert_eq!(e.validate().unwrap_err(), "Please enter a system prompt.");
        e.system_prompt = "Text for correction: ".into();
        let p = e.validate().unwrap();
        assert_eq!(p.title, "Fix");
        assert_eq!(p.system_prompt, "Text for correction: ");
    }

    #[test]
    fn the_picker_preselects_the_current_model() {
        let mut p = ModelPicker::new("b");
        p.start_loading(1);
        assert_eq!(p.status, "Loading models…");
        let model = |id: &str| ModelInfo { id: id.into(), name: id.into(), ..Default::default() };
        p.loaded(vec![model("a"), model("b")]);
        assert_eq!(p.selected, Some(1));
        assert_eq!(p.status, "2 models:");
        p.loaded(Vec::new());
        assert_eq!(p.status, "No models available or no API key set.");
    }
}
