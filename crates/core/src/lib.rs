//! AI Assistant engine. Nothing here depends on egui, eframe, winit or rfd, does I/O or talks to
//! the network, and every module builds for `wasm32-unknown-unknown` (`cargo xtask wasm`).
//!
//! - [`settings`]: the prompts, theme and remembered selection, in the same JSON format as the
//!   Python AiAssistantLinux app (`~/.config/aiassistant/settings.json`).
//! - [`openrouter`]: builds OpenRouter requests and parses the responses; the UI layer sends them.
//! - [`format`]: how prices, context lengths and model names are shown.
//!
//! All input (settings files, API responses) is untrusted: return `Result`, never panic.

pub mod format;
pub mod openrouter;
pub mod settings;

pub use openrouter::{AiError, Balance, HttpRequest, HttpResponse, Method, ModelInfo};
pub use settings::{AppSettings, PromptConfig, Theme};
