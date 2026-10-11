//! Headless UI tests (egui_kittest): no window, no GPU and no network. Requests are answered by
//! [`ScriptedHttp`]; settings live in a [`MemoryStore`].

use aiassist_core::{AppSettings, PromptConfig, Theme};
use aiassist_ui::testing::{MemoryStore, ScriptedHttp};
use aiassist_ui::{LogBuffer, MainApp, Tab};
use egui::Key;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

const KEY: &str = "sk-or-test";
const COMPLETION: &str = r#"{"choices":[{"message":{"role":"assistant","content":"This is a test with a grammar error."}}]}"#;
const MODELS: &str = r#"{"data":[
    {"id":"vendor/paid","name":"Paid Model","description":"A paid one.","context_length":128000,"pricing":{"prompt":"0.000002"}},
    {"id":"google/gemma-4-26b-a4b-it:free","name":"Gemma 4","context_length":1048576,"pricing":{"prompt":"0"}}
]}"#;

fn prompt(title: &str, model: &str) -> PromptConfig {
    PromptConfig { title: title.to_owned(), system_prompt: format!("{title}: "), model: model.to_owned() }
}

fn two_prompts() -> AppSettings {
    AppSettings { prompts: vec![prompt("Correct grammar", "m/a"), prompt("Translate", "m/b")], ..AppSettings::default() }
}

fn harness(store: &MemoryStore, http: &ScriptedHttp) -> Harness<'static, MainApp> {
    let (store, http) = (store.clone(), http.clone());
    let mut h = Harness::builder()
        .with_size(egui::vec2(560.0, 720.0))
        .build_eframe(move |cc| MainApp::new(cc, Box::new(store), Box::new(http), LogBuffer::default()));
    h.run_steps(2);
    h
}

/// Focuses the input labelled `label` and types `text` into it.
fn type_into(h: &mut Harness<'_, MainApp>, label: &str, text: &str) {
    h.get_by_label(label).focus();
    h.run_steps(1);
    h.get_by_label(label).type_text(text);
    h.run_steps(1);
}

fn send_text(h: &mut Harness<'_, MainApp>, text: &str) {
    text.clone_into(h.state_mut().input_mut());
    h.run_steps(1);
    h.get_by_label("Send").click();
    h.run_steps(3);
}

#[test]
fn starts_with_the_remembered_prompt() {
    let mut settings = two_prompts();
    settings.selected_prompt = "Translate".to_owned();
    let store = MemoryStore::new(settings, KEY);
    let h = harness(&store, &ScriptedHttp::default());
    assert_eq!(h.state().tab(), Tab::Main);
    assert_eq!(h.state().selected_prompt().map(|p| p.title.as_str()), Some("Translate"));
    h.get_by_label("Your text:");
    h.get_by_label("Result:");
    h.get_by_label("Copy Result");
}

/// Regression: right-aligned button rows once took all the remaining height, pushing Send far
/// below the text box and Copy Result out of the window.
#[test]
fn buttons_sit_under_their_boxes_inside_the_window() {
    let store = MemoryStore::new(two_prompts(), KEY);
    let h = harness(&store, &ScriptedHttp::default());
    let rect = |label: &str| h.get_all_by_label(label).last().unwrap().rect();
    let (input, send, result, copy) = (rect("Your text:"), rect("Send"), rect("Result:"), rect("Copy Result"));
    assert!(send.top() >= input.bottom() && send.top() - input.bottom() < 16.0, "{input:?} {send:?}");
    assert!(copy.top() >= result.bottom() && copy.top() - result.bottom() < 16.0, "{result:?} {copy:?}");
    assert!(copy.bottom() <= 720.0, "{copy:?}");
    assert!(send.right() > 500.0, "Send is right-aligned: {send:?}");
}

#[test]
fn send_shows_the_answer_and_sends_a_system_message() {
    let store = MemoryStore::new(two_prompts(), KEY);
    let http = ScriptedHttp::default().respond("chat/completions", 200, COMPLETION);
    let mut h = harness(&store, &http);
    send_text(&mut h, "This is a test with grammer error");
    assert_eq!(h.state().output(), "This is a test with a grammar error.");

    let requests = http.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].api_key, KEY);
    let body: serde_json::Value = serde_json::from_str(requests[0].body.as_deref().unwrap()).unwrap();
    assert_eq!(body["model"], "m/a");
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][0]["content"], "Correct grammar: ");
    assert_eq!(body["messages"][1]["content"], "This is a test with grammer error");
}

#[test]
fn copy_result_gives_feedback() {
    let store = MemoryStore::new(two_prompts(), KEY);
    let http = ScriptedHttp::default().respond("chat/completions", 200, COMPLETION);
    let mut h = harness(&store, &http);
    send_text(&mut h, "text");
    h.get_by_label("Copy Result").click();
    h.run_steps(2);
    h.get_by_label("Copied!");
    let copied = h.output().platform_output.commands.iter().any(|c| matches!(c, egui::OutputCommand::CopyText(t) if t.contains("grammar error")))
        || h.state().output().contains("grammar error");
    assert!(copied);
}

#[test]
fn api_and_network_errors_land_in_the_result() {
    let store = MemoryStore::new(two_prompts(), KEY);
    let http = ScriptedHttp::default().respond("chat/completions", 401, r#"{"error":{"message":"No auth"}}"#);
    let mut h = harness(&store, &http);
    send_text(&mut h, "text");
    assert_eq!(h.state().output(), "Error: API returned 401");

    let http = ScriptedHttp::default().fail("chat/completions", "connection refused");
    let mut h = harness(&store, &http);
    send_text(&mut h, "text");
    assert_eq!(h.state().output(), "Error: Network error - connection refused");
}

#[test]
fn without_an_api_key_nothing_is_sent() {
    let store = MemoryStore::new(two_prompts(), "");
    let http = ScriptedHttp::default();
    let mut h = harness(&store, &http);
    send_text(&mut h, "text");
    assert!(h.state().output().starts_with("Error: AI service is not configured."));
    assert!(http.requests().is_empty());
}

#[test]
fn empty_text_is_refused_with_a_message() {
    let store = MemoryStore::new(two_prompts(), KEY);
    let http = ScriptedHttp::default();
    let mut h = harness(&store, &http);
    send_text(&mut h, "   ");
    h.get_by_label("Please enter some text to process.");
    h.get_by_label("OK").click();
    h.run_steps(2);
    assert!(h.query_by_label("Please enter some text to process.").is_none());
    assert!(http.requests().is_empty());
}

#[test]
fn no_prompts_is_refused_with_a_message() {
    let store = MemoryStore::new(AppSettings::default(), KEY);
    let mut h = harness(&store, &ScriptedHttp::default());
    send_text(&mut h, "text");
    h.get_by_label("No prompts configured. Add one in Settings.");
}

#[test]
fn adding_a_prompt_and_saving_stores_it_with_the_api_key() {
    let store = MemoryStore::new(two_prompts(), "");
    let mut h = harness(&store, &ScriptedHttp::default());
    h.get_by_label("Settings").click();
    h.run_steps(2);

    type_into(&mut h, "OpenRouter API Key:", "sk-or-new");
    h.get_by_label("Add").click();
    h.run_steps(2);
    h.get_by_label("Add Prompt");
    type_into(&mut h, "Titel:", "Summarize");
    type_into(&mut h, "System prompt:", "Summarize this: ");
    h.run_steps(1);
    h.get_by_label("OK").click();
    h.run_steps(2);
    assert_eq!(h.state().prompts().len(), 3);
    // Not stored before Save Settings.
    assert_eq!(store.state().settings.prompts.len(), 2);

    h.get_by_label("Save Settings").click();
    h.run_steps(2);
    h.get_by_label("Settings saved successfully!");
    let state = store.state();
    assert_eq!(state.api_key, "sk-or-new");
    let added = state.settings.prompts.last().unwrap();
    assert_eq!((added.title.as_str(), added.system_prompt.as_str()), ("Summarize", "Summarize this: "));
}

#[test]
fn the_editor_validates_and_rejects_duplicates() {
    let store = MemoryStore::new(two_prompts(), KEY);
    let mut h = harness(&store, &ScriptedHttp::default());
    h.get_by_label("Settings").click();
    h.run_steps(2);
    h.get_by_label("Add").click();
    h.run_steps(2);
    h.get_by_label("OK").click();
    h.run_steps(2);
    h.get_by_label("Please enter a title.");
    h.get_all_by_label("OK").last().unwrap().click();
    h.run_steps(2);

    type_into(&mut h, "Titel:", "translate");
    type_into(&mut h, "System prompt:", "x");
    h.run_steps(1);
    h.get_by_label("OK").click();
    h.run_steps(2);
    h.get_by_label("A prompt named 'translate' already exists.");
    assert_eq!(h.state().prompts().len(), 2);
}

#[test]
fn a_model_is_chosen_from_the_catalogue() {
    let mut settings = two_prompts();
    settings.prompts[0].model = "google/gemma-4-26b-a4b-it:free".to_owned();
    let store = MemoryStore::new(settings, KEY);
    let http = ScriptedHttp::default().respond("/models", 200, MODELS);
    let mut h = harness(&store, &http);
    h.get_by_label("Settings").click();
    h.run_steps(2);
    h.get_by_label("Correct grammar").click();
    h.run_steps(1);
    h.get_by_label("Edit").click();
    h.run_steps(2);
    h.get_by_label("google/gemma-4-26b-a4b-it:free").click();
    h.run_steps(3);

    h.get_by_label("Select AI Model");
    h.get_by_label("2 models:");
    h.get_by_label("Free");
    h.get_by_label("$2.00/M");
    h.get_by_label("128K");
    h.get_by_label("Paid Model (vendor/paid)").click();
    h.run_steps(2);
    h.get_by_label("A paid one.");
    h.get_all_by_label("OK").last().unwrap().click();
    h.run_steps(2);
    assert!(h.query_by_label("Select AI Model").is_none());
    h.get_by_label("vendor/paid");
    h.get_by_label("OK").click();
    h.run_steps(2);
    assert_eq!(h.state().prompts()[0].model, "vendor/paid");
}

#[test]
fn the_balance_is_shown() {
    let store = MemoryStore::new(two_prompts(), KEY);
    let http = ScriptedHttp::default().respond("/credits", 200, r#"{"data":{"total_credits":20,"total_usage":10.32}}"#);
    let mut h = harness(&store, &http);
    h.get_by_label("Settings").click();
    h.run_steps(2);
    h.get_by_label("Balance").click();
    h.run_steps(3);
    h.get_by_label("OpenRouter Balance");
    h.get_by_label("Total credits: $20.00\nUsed: $10.32\nRemaining: $9.68");
}

#[test]
fn changing_the_theme_is_stored_at_once() {
    let store = MemoryStore::new(two_prompts(), KEY);
    let mut h = harness(&store, &ScriptedHttp::default());
    h.get_by_label("Settings").click();
    h.run_steps(2);
    h.get_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some("System")).click();
    h.run_steps(2);
    h.get_by_label("Dark").click();
    h.run_steps(2);
    assert_eq!(store.state().settings.theme, Theme::Dark);
}

#[test]
fn removing_the_selected_prompt_selects_the_first() {
    let mut settings = two_prompts();
    settings.selected_prompt = "Translate".to_owned();
    let store = MemoryStore::new(settings, KEY);
    let mut h = harness(&store, &ScriptedHttp::default());
    h.get_by_label("Settings").click();
    h.run_steps(2);
    h.get_by_label("Translate").click();
    h.run_steps(1);
    h.get_by_label("Remove").click();
    h.run_steps(2);
    assert_eq!(h.state().prompts().len(), 1);
    assert_eq!(h.state().selected_prompt().map(|p| p.title.as_str()), Some("Correct grammar"));

    h.get_by_label("Save Settings").click();
    h.run_steps(2);
    let saved = store.state().settings;
    assert_eq!(saved.prompts.len(), 1);
    assert_eq!(saved.selected_prompt, "", "a deleted prompt is not remembered");
}

#[test]
fn edit_without_a_selection_explains_why() {
    let store = MemoryStore::new(two_prompts(), KEY);
    let mut h = harness(&store, &ScriptedHttp::default());
    h.get_by_label("Settings").click();
    h.run_steps(2);
    h.get_by_label("Edit").click();
    h.run_steps(2);
    h.get_by_label("Please select a prompt to edit.");
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert!(h.query_by_label("Please select a prompt to edit.").is_none());
}

#[test]
fn the_log_tab_shows_lines() {
    let logs = LogBuffer::default();
    logs.push(log::Level::Warn, "Something odd.".to_owned());
    let store = MemoryStore::new(two_prompts(), KEY);
    let logs_for_app = logs.clone();
    let mut h = Harness::builder()
        .with_size(egui::vec2(560.0, 720.0))
        .build_eframe(move |cc| MainApp::new(cc, Box::new(store), Box::new(ScriptedHttp::default()), logs_for_app));
    h.state_mut().set_tab(Tab::Log);
    h.run_steps(2);
    let line = h.get_all_by(|n| n.role() == Role::Label && n.value().is_some_and(|v| v.ends_with("WARNING Something odd."))).count();
    assert_eq!(line, 1);
    h.get_by_label("Clear Log").click();
    h.run_steps(2);
    assert!(logs.snapshot().is_empty());
}
