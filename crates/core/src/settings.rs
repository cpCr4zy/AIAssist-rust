//! Application settings: the prompt library, the theme and the prompt selected last.
//!
//! The JSON layout is shared with the Python AiAssistantLinux app, so both read and write the same
//! `settings.json`:
//!
//! ```json
//! { "prompts": [{ "title": "…", "system_prompt": "…", "model": "…" }],
//!   "theme": "system", "selected_prompt": "…" }
//! ```
//!
//! The API key is never part of this file; the UI layer keeps it in the OS keyring.

use serde::{Deserialize, Serialize};

/// The model of the default prompt: free on OpenRouter and known to respond.
pub const DEFAULT_MODEL: &str = "google/gemma-4-26b-a4b-it:free";

/// Settings files larger than this are refused instead of read.
pub const MAX_SETTINGS_BYTES: usize = 16 * 1024 * 1024;

/// Errors reading a settings file.
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("the settings file is {len} bytes, more than the {max}-byte limit")]
    TooLarge { len: usize, max: usize },
    #[error("the settings file is not valid: {0}")]
    Invalid(#[from] serde_json::Error),
}

/// A reusable prompt: a name, the system instruction and the model that runs it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PromptConfig {
    pub title: String,
    /// Kept exactly as typed: trailing spaces matter ("Text for correction: ").
    pub system_prompt: String,
    /// OpenRouter model id, e.g. `google/gemma-4-26b-a4b-it:free`; may be empty.
    pub model: String,
}

impl PromptConfig {
    pub fn is_valid(&self) -> bool {
        !self.title.trim().is_empty() && !self.system_prompt.trim().is_empty()
    }
}

/// The colour theme. Unknown values in a settings file read as [`Theme::System`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum Theme {
    /// Catppuccin Mocha.
    Dark,
    /// Catppuccin Latte.
    Light,
    /// Follows the operating system.
    #[default]
    System,
}

impl Theme {
    pub const ALL: [Self; 3] = [Self::Dark, Self::Light, Self::System];

    /// The value stored in the settings file.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::System => "system",
        }
    }

    /// The name shown in the UI.
    pub fn label(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
            Self::System => "System",
        }
    }
}

impl From<String> for Theme {
    fn from(value: String) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "dark" => Self::Dark,
            "light" => Self::Light,
            "system" => Self::System,
            other => {
                log::warn!("Unknown theme '{other}', using the system theme.");
                Self::System
            }
        }
    }
}

impl From<Theme> for String {
    fn from(value: Theme) -> Self {
        value.as_str().to_owned()
    }
}

/// Everything stored in `settings.json`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub prompts: Vec<PromptConfig>,
    pub theme: Theme,
    /// Title of the prompt selected on the Main tab, restored on the next start.
    pub selected_prompt: String,
}

impl AppSettings {
    /// What a first start (no settings file) begins with.
    pub fn create_default() -> Self {
        Self {
            prompts: vec![PromptConfig {
                title: "Correct grammar".to_owned(),
                system_prompt: "Correct grammar and syntax errors. Text for correction: ".to_owned(),
                model: DEFAULT_MODEL.to_owned(),
            }],
            theme: Theme::System,
            selected_prompt: String::new(),
        }
    }

    /// Reads a settings file. Missing fields take their defaults and unknown ones are ignored.
    pub fn from_json(json: &str) -> Result<Self, SettingsError> {
        if json.len() > MAX_SETTINGS_BYTES {
            return Err(SettingsError::TooLarge { len: json.len(), max: MAX_SETTINGS_BYTES });
        }
        Ok(serde_json::from_str(json)?)
    }

    /// Writes the settings file: indented by two spaces, as the Python app writes it.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Index of the prompt to select on start: the remembered one if it still exists, otherwise
    /// the first; `None` when there are no prompts.
    pub fn selected_index(&self) -> Option<usize> {
        if self.prompts.is_empty() {
            return None;
        }
        Some(self.prompts.iter().position(|p| p.title == self.selected_prompt).unwrap_or(0))
    }

    /// Forgets the remembered selection when that prompt no longer exists.
    pub fn drop_stale_selection(&mut self) {
        if !self.prompts.iter().any(|p| p.title == self.selected_prompt) {
            self.selected_prompt.clear();
        }
    }
}

/// Whether `prompts` already has a prompt called `title` (trimmed, case-insensitive), not counting
/// the one at `exclude` (the prompt being edited).
pub fn title_exists(prompts: &[PromptConfig], title: &str, exclude: Option<usize>) -> bool {
    let wanted = title.trim().to_lowercase();
    prompts.iter().enumerate().any(|(i, p)| Some(i) != exclude && p.title.trim().to_lowercase() == wanted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// A file written by the Python AiAssistantLinux app.
    const PYTHON_FILE: &str = r#"{
  "prompts": [
    {
      "title": "Correct grammar",
      "system_prompt": "Correct grammar and syntax errors. Text for correction: ",
      "model": "google/gemma-4-26b-a4b-it:free"
    },
    {
      "title": "(EN) 1-Correct grammar and style",
      "system_prompt": "You are a text editor.\n\nText to correct:\n",
      "model": "~anthropic/claude-opus-latest"
    }
  ],
  "theme": "system",
  "selected_prompt": "(EN) 1-Correct grammar and style"
}"#;

    #[test]
    fn reads_the_python_apps_file() {
        let s = AppSettings::from_json(PYTHON_FILE).unwrap();
        assert_eq!(s.prompts.len(), 2);
        assert_eq!(s.prompts[1].model, "~anthropic/claude-opus-latest");
        assert_eq!(s.prompts[1].system_prompt, "You are a text editor.\n\nText to correct:\n");
        assert_eq!(s.theme, Theme::System);
        assert_eq!(s.selected_index(), Some(1));
    }

    #[test]
    fn writes_the_same_layout_the_python_app_reads() {
        let s = AppSettings::from_json(PYTHON_FILE).unwrap();
        assert_eq!(s.to_json().unwrap(), PYTHON_FILE);
    }

    #[test]
    fn missing_fields_take_defaults() {
        let s = AppSettings::from_json("{}").unwrap();
        assert_eq!(s, AppSettings::default());
        let s = AppSettings::from_json(r#"{"prompts":[{"title":"t"}],"extra":1}"#).unwrap();
        assert_eq!(s.prompts[0].title, "t");
        assert_eq!(s.prompts[0].model, "");
    }

    #[test]
    fn an_unknown_theme_reads_as_system() {
        let s = AppSettings::from_json(r#"{"theme":"solarized"}"#).unwrap();
        assert_eq!(s.theme, Theme::System);
        let s = AppSettings::from_json(r#"{"theme":"Dark"}"#).unwrap();
        assert_eq!(s.theme, Theme::Dark);
    }

    #[test]
    fn a_wrongly_typed_field_is_an_error_not_a_panic() {
        assert!(AppSettings::from_json(r#"{"prompts": 5}"#).is_err());
        assert!(AppSettings::from_json("not json").is_err());
    }

    #[test]
    fn the_default_has_a_valid_prompt() {
        let s = AppSettings::create_default();
        assert!(s.prompts.iter().all(PromptConfig::is_valid));
        assert_eq!(s.selected_index(), Some(0));
    }

    #[test]
    fn a_stale_selection_falls_back_to_the_first_prompt_and_can_be_dropped() {
        let mut s = AppSettings::create_default();
        s.selected_prompt = "gone".to_owned();
        assert_eq!(s.selected_index(), Some(0));
        s.drop_stale_selection();
        assert_eq!(s.selected_prompt, "");
        assert_eq!(AppSettings::default().selected_index(), None);
    }

    #[test]
    fn title_exists_ignores_case_spaces_and_the_edited_prompt() {
        let prompts = AppSettings::create_default().prompts;
        assert!(title_exists(&prompts, "  correct GRAMMAR ", None));
        assert!(!title_exists(&prompts, "correct grammar", Some(0)));
        assert!(!title_exists(&prompts, "Translate", None));
    }

    #[test]
    fn validity_needs_a_title_and_a_system_prompt() {
        let p = PromptConfig { title: "t".into(), system_prompt: " ".into(), model: String::new() };
        assert!(!p.is_valid());
        let p = PromptConfig { title: "t".into(), system_prompt: "s".into(), model: String::new() };
        assert!(p.is_valid());
    }

    proptest! {
        #[test]
        fn parsing_never_panics(json in ".{0,512}") {
            let _ = AppSettings::from_json(&json);
        }

        #[test]
        fn round_trips(title in ".{0,40}", prompt in ".{0,80}", model in ".{0,40}", selected in ".{0,40}") {
            let s = AppSettings {
                prompts: vec![PromptConfig { title, system_prompt: prompt, model }],
                theme: Theme::Light,
                selected_prompt: selected,
            };
            prop_assert_eq!(AppSettings::from_json(&s.to_json().unwrap()).unwrap(), s);
        }
    }
}
