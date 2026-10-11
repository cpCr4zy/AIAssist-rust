//! In-memory doubles for [`Store`] and [`Http`], for UI tests (`crates/ui/tests/`) and demos.
//! Both are cheap handles: keep a clone to inspect what the app did.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use aiassist_core::{AppSettings, HttpRequest, HttpResponse};

use crate::services::{Http, HttpCallback, Store};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a [`MemoryStore`] holds.
#[derive(Clone, Debug, Default)]
pub struct MemoryState {
    pub settings: AppSettings,
    pub api_key: String,
    /// How many times `save` was called.
    pub saves: usize,
}

#[derive(Clone, Default)]
pub struct MemoryStore(Arc<Mutex<MemoryState>>);

impl MemoryStore {
    pub fn new(settings: AppSettings, api_key: &str) -> Self {
        Self(Arc::new(Mutex::new(MemoryState { settings, api_key: api_key.to_owned(), saves: 0 })))
    }

    pub fn state(&self) -> MemoryState {
        lock(&self.0).clone()
    }
}

impl Store for MemoryStore {
    fn load(&mut self) -> AppSettings {
        lock(&self.0).settings.clone()
    }

    fn save(&mut self, settings: &AppSettings) -> Result<(), String> {
        let mut state = lock(&self.0);
        state.settings = settings.clone();
        state.saves = state.saves.saturating_add(1);
        Ok(())
    }

    fn api_key(&mut self) -> String {
        lock(&self.0).api_key.clone()
    }

    fn set_api_key(&mut self, api_key: &str) -> Result<(), String> {
        api_key.trim().clone_into(&mut lock(&self.0).api_key);
        Ok(())
    }
}

/// A scripted answer: the URL suffix it applies to, and the response or network error.
type Rule = (String, Result<HttpResponse, String>);

/// Answers requests from a script: the first rule whose URL suffix matches wins; unmatched
/// requests fail like a network error. Answers are delivered at once, inside `fetch`.
#[derive(Clone, Default)]
pub struct ScriptedHttp {
    rules: Arc<Mutex<Vec<Rule>>>,
    requests: Arc<Mutex<Vec<HttpRequest>>>,
}

impl ScriptedHttp {
    /// Answers requests to URLs ending in `url_suffix` with `status` and `body`.
    pub fn respond(self, url_suffix: &str, status: u16, body: &str) -> Self {
        lock(&self.rules).push((url_suffix.to_owned(), Ok(HttpResponse { status, body: body.to_owned() })));
        self
    }

    /// Fails requests to URLs ending in `url_suffix` with a network error.
    pub fn fail(self, url_suffix: &str, error: &str) -> Self {
        lock(&self.rules).push((url_suffix.to_owned(), Err(error.to_owned())));
        self
    }

    /// Every request sent so far.
    pub fn requests(&self) -> Vec<HttpRequest> {
        lock(&self.requests).clone()
    }
}

impl Http for ScriptedHttp {
    fn fetch(&self, request: HttpRequest, done: HttpCallback) {
        let answer = lock(&self.rules)
            .iter()
            .find(|(suffix, _)| request.url.ends_with(suffix.as_str()))
            .map_or_else(|| Err(format!("no scripted answer for {}", request.url)), |(_, answer)| answer.clone());
        lock(&self.requests).push(request);
        done(answer);
    }
}
