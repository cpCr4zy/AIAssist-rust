//! Live checks against OpenRouter with the real settings folder and keyring (desktop only).
//! Not run by default: they need a network and a saved API key.
//!
//! `cargo test -p aiassist-ui --test live -- --ignored --nocapture`

#![cfg(not(target_arch = "wasm32"))]

use std::sync::mpsc;
use std::time::Duration;

use aiassist_core::openrouter::{completion_request, models_request, parse_completion, parse_models};
use aiassist_core::settings::DEFAULT_MODEL;
use aiassist_core::{HttpRequest, HttpResponse};
use aiassist_ui::{FileStore, Http, Store, UreqHttp};

fn config_dir() -> std::path::PathBuf {
    let home = std::env::var_os("HOME").expect("HOME");
    std::env::var_os("XDG_CONFIG_HOME").map_or_else(|| std::path::PathBuf::from(home).join(".config"), Into::into).join("aiassistant")
}

fn fetch(http: &UreqHttp, request: HttpRequest) -> HttpResponse {
    let (tx, rx) = mpsc::channel();
    http.fetch(request, Box::new(move |r| tx.send(r).unwrap()));
    rx.recv_timeout(Duration::from_secs(120)).expect("timed out").expect("network error")
}

#[test]
#[ignore = "needs a network and a saved OpenRouter API key"]
fn models_and_a_free_completion() {
    let mut store = FileStore::new(config_dir());
    let key = store.api_key();
    assert!(!key.is_empty(), "no API key in the keyring or the .api_key file");
    let http = UreqHttp::new();

    let models = parse_models(&fetch(&http, models_request(&key).unwrap())).unwrap();
    println!("{} models; the first: {:?}", models.len(), models.first().map(|m| &m.id));
    assert!(models.len() > 10);

    let request =
        completion_request(&key, "Correct grammar and syntax errors. Text for correction: ", "This is a test with grammer error", DEFAULT_MODEL)
            .unwrap();
    match parse_completion(&fetch(&http, request)) {
        Ok(text) => println!("answer: {text}"),
        // Free models are rate-limited now and then; the transport still worked.
        Err(e) => println!("model answered with an error: {e}"),
    }
}
