//! AI Assistant in the browser. Build: `cd apps/aiassist-web && trunk build --release`
//! (trunk generates the JS loader; no handwritten JS).
//!
//! Settings and the API key live in the site's local storage; requests go straight to OpenRouter.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast;

    // The browser console gets info and up; the Log tab also shows this app's debug lines.
    let logs = aiassist_ui::logging::install(Some(Box::new(eframe::WebLogger::new(log::LevelFilter::Info))), log::LevelFilter::Info);
    // WebGL2: the most widely supported browser renderer.
    let options = eframe::WebOptions { renderer: eframe::Renderer::Glow, ..Default::default() };
    wasm_bindgen_futures::spawn_local(async move {
        let Some(canvas) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("aiassist"))
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        else {
            log::error!("AI Assistant: no <canvas id=\"aiassist\"> in the page");
            return;
        };
        let started = eframe::WebRunner::new()
            .start(
                canvas,
                options,
                Box::new(|cc| {
                    let store = Box::new(aiassist_ui::WebStore);
                    let http = Box::new(aiassist_ui::FetchHttp);
                    Ok(Box::new(aiassist_ui::MainApp::new(cc, store, http, logs)))
                }),
            )
            .await;
        if let Err(e) = started {
            log::error!("AI Assistant failed to start: {e:?}");
        }
    });
}

/// `cargo build --workspace` and clippy compile this crate for the host too.
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("aiassist-web runs in the browser: cd apps/aiassist-web && trunk serve");
}
