//! Desktop HTTP: ureq over rustls with the operating system's root certificates, one short-lived
//! thread per request.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use aiassist_core::openrouter::MAX_RESPONSE_BYTES;
use aiassist_core::{HttpRequest, HttpResponse, Method};
use ureq::tls::{Certificate, RootCerts, TlsConfig, TlsProvider};

use crate::services::{Http, HttpCallback};

/// Generous for slow models, but a hung connection still ends.
const TIMEOUT: Duration = Duration::from_secs(120);

pub struct UreqHttp {
    agent: ureq::Agent,
}

impl Default for UreqHttp {
    fn default() -> Self {
        Self::new()
    }
}

impl UreqHttp {
    pub fn new() -> Self {
        let found = rustls_native_certs::load_native_certs();
        for e in &found.errors {
            log::warn!("System certificate store: {e}");
        }
        if found.certs.is_empty() {
            log::error!("No system root certificates found; HTTPS requests will fail.");
        }
        let roots: Vec<Certificate<'static>> = found.certs.iter().map(|c| Certificate::from_der(c.as_ref()).to_owned()).collect();
        let tls = TlsConfig::builder().provider(TlsProvider::Rustls).root_certs(RootCerts::new_with_certs(&roots)).build();
        let config = ureq::Agent::config_builder()
            .tls_config(tls)
            .timeout_global(Some(TIMEOUT))
            // 4xx/5xx are answers to show, not transport failures.
            .http_status_as_error(false)
            .build();
        Self { agent: config.into() }
    }
}

impl Http for UreqHttp {
    fn fetch(&self, request: HttpRequest, done: HttpCallback) {
        let agent = self.agent.clone();
        // Shared so the callback still runs if the thread can't be started.
        let done = Arc::new(Mutex::new(Some(done)));
        let in_thread = Arc::clone(&done);
        let spawned = std::thread::Builder::new().name("http".to_owned()).spawn(move || {
            let result = send(&agent, &request);
            if let Some(done) = in_thread.lock().unwrap_or_else(PoisonError::into_inner).take() {
                done(result);
            }
        });
        if let Err(e) = spawned
            && let Some(done) = done.lock().unwrap_or_else(PoisonError::into_inner).take()
        {
            done(Err(format!("could not start a request thread: {e}")));
        }
    }
}

fn send(agent: &ureq::Agent, request: &HttpRequest) -> Result<HttpResponse, String> {
    let auth = format!("Bearer {}", request.api_key);
    let response = match request.method {
        Method::Get => agent.get(&request.url).header("Authorization", &auth).call(),
        Method::Post => {
            agent.post(&request.url).header("Authorization", &auth).content_type("application/json").send(request.body.as_deref().unwrap_or_default())
        }
    }
    .map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    let body = response.into_body().with_config().limit(MAX_RESPONSE_BYTES).read_to_string().map_err(|e| e.to_string())?;
    Ok(HttpResponse { status, body })
}
