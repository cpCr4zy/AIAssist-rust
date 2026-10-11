//! What the UI needs from the platform: somewhere to keep settings and the API key ([`Store`]) and
//! a way to send HTTP requests ([`Http`]). The desktop, the browser and the tests each bring their
//! own implementation.

use aiassist_core::{AppSettings, HttpRequest, HttpResponse};

/// Settings and API-key storage.
pub trait Store {
    /// Reads the settings; problems are logged and answered with defaults, never an error.
    fn load(&mut self) -> AppSettings;
    /// Writes the settings.
    fn save(&mut self, settings: &AppSettings) -> Result<(), String>;
    /// The saved API key, or an empty string.
    fn api_key(&mut self) -> String;
    /// Saves the API key; an empty key removes it.
    fn set_api_key(&mut self, api_key: &str) -> Result<(), String>;
    /// Shown under the key field when the key is kept somewhere the user should know about.
    fn api_key_note(&self) -> Option<&'static str> {
        None
    }
}

/// Called with the response, or with a description of the network failure.
pub type HttpCallback = Box<dyn FnOnce(Result<HttpResponse, String>) + Send + 'static>;

/// Sends a request without blocking the UI and calls `done` when it finishes (possibly from
/// another thread, possibly before `fetch` returns).
pub trait Http {
    fn fetch(&self, request: HttpRequest, done: HttpCallback);
}
