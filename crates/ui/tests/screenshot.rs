//! Renders the README screenshot (`docs/screenshot.png`) from demo data: the real UI, no network.
//! Needs a GPU (or a software Vulkan driver), so it does not run by default:
//!
//! `cargo test -p aiassist-ui --test screenshot -- --ignored`

use aiassist_core::{AppSettings, PromptConfig, Theme};
use aiassist_ui::testing::{MemoryStore, ScriptedHttp};
use aiassist_ui::{LogBuffer, MainApp};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

const INPUT: &str = "Hi Anna,\n\nthanks for you're quick reply. I have went through the draft and \
I think its almost ready, we just need to fix the numbers in table 2 before we sends it to the client.\n\n\
Best regards,\nAlex";

const RESULT: &str = "Hi Anna,\n\nThanks for your quick reply. I have gone through the draft and \
I think it's almost ready; we just need to fix the numbers in Table 2 before we send it to the client.\n\n\
Best regards,\nAlex";

fn demo_settings() -> AppSettings {
    let prompt = |title: &str, model: &str| PromptConfig { title: title.into(), system_prompt: format!("{title}: "), model: model.into() };
    AppSettings {
        prompts: vec![
            prompt("Correct grammar and style", "google/gemma-4-26b-a4b-it:free"),
            prompt("Translate to German", "google/gemma-4-26b-a4b-it:free"),
            prompt("Summarize in three sentences", "anthropic/claude-sonnet"),
        ],
        theme: Theme::Dark,
        selected_prompt: "Correct grammar and style".into(),
    }
}

#[test]
#[ignore = "needs a GPU; writes docs/screenshot.png"]
fn readme_screenshot() {
    let completion = serde_json::json!({"choices": [{"message": {"role": "assistant", "content": RESULT}}]}).to_string();
    let store = MemoryStore::new(demo_settings(), "sk-or-demo");
    let http = ScriptedHttp::default().respond("chat/completions", 200, &completion);
    let mut h = Harness::builder()
        .with_size(egui::vec2(560.0, 640.0))
        .with_pixels_per_point(1.5)
        .wgpu()
        .build_eframe(move |cc| MainApp::new(cc, Box::new(store), Box::new(http), LogBuffer::default()));
    h.run_steps(2);
    INPUT.clone_into(h.state_mut().input_mut());
    h.run_steps(1);
    h.get_by_label("Send").click();
    h.run_steps(4);
    assert_eq!(h.state().output(), RESULT);

    // No pointer hovering over Send in the picture.
    h.remove_cursor();
    h.run_steps(1);
    let image = h.render().expect("rendering needs a GPU or a software Vulkan driver");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/screenshot.png");
    image.save(&path).expect("write docs/screenshot.png");
    println!("wrote {}", path.display());
}
