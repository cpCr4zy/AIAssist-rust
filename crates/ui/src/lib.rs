//! The egui shell of AI Assistant. Shared by `apps/aiassist` (desktop) and `apps/aiassist-web`
//! (wasm32).
//!
//! The window has three tabs: **Main** (pick a prompt, enter text, Send, read and copy the result),
//! **Settings** (API key, balance, theme, the prompt library) and **Log**. Storage and HTTP come
//! from the platform through [`Store`] and [`Http`]; [`testing`] has in-memory doubles.

mod app;
mod dialogs;
pub mod logging;
pub mod services;
pub mod testing;
mod theme;

#[cfg(not(target_arch = "wasm32"))]
mod http_native;
#[cfg(target_arch = "wasm32")]
mod http_web;
#[cfg(not(target_arch = "wasm32"))]
mod store_file;
#[cfg(target_arch = "wasm32")]
mod store_web;

pub use app::{MainApp, Tab};
#[cfg(not(target_arch = "wasm32"))]
pub use http_native::UreqHttp;
#[cfg(target_arch = "wasm32")]
pub use http_web::FetchHttp;
pub use logging::LogBuffer;
pub use services::{Http, Store};
#[cfg(not(target_arch = "wasm32"))]
pub use store_file::FileStore;
#[cfg(target_arch = "wasm32")]
pub use store_web::WebStore;
