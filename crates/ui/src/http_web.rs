//! Browser HTTP: the `fetch` API. OpenRouter answers CORS requests, so no proxy is needed.

use aiassist_core::openrouter::MAX_RESPONSE_BYTES;
use aiassist_core::{HttpRequest, HttpResponse, Method};
use wasm_bindgen::{JsCast as _, JsValue};
use wasm_bindgen_futures::JsFuture;

use crate::services::{Http, HttpCallback};

#[derive(Default)]
pub struct FetchHttp;

impl Http for FetchHttp {
    fn fetch(&self, request: HttpRequest, done: HttpCallback) {
        wasm_bindgen_futures::spawn_local(async move { done(send(request).await) });
    }
}

fn js_error(value: JsValue) -> String {
    value.as_string().unwrap_or_else(|| format!("{value:?}"))
}

async fn send(request: HttpRequest) -> Result<HttpResponse, String> {
    let headers = web_sys::Headers::new().map_err(js_error)?;
    headers.set("Authorization", &format!("Bearer {}", request.api_key)).map_err(js_error)?;
    let init = web_sys::RequestInit::new();
    match request.method {
        Method::Get => init.set_method("GET"),
        Method::Post => {
            init.set_method("POST");
            headers.set("Content-Type", "application/json").map_err(js_error)?;
            init.set_body(&JsValue::from_str(request.body.as_deref().unwrap_or_default()));
        }
    }
    init.set_headers(&headers);
    let js_request = web_sys::Request::new_with_str_and_init(&request.url, &init).map_err(js_error)?;
    let window = web_sys::window().ok_or("no browser window")?;
    let response: web_sys::Response = JsFuture::from(window.fetch_with_request(&js_request)).await.map_err(js_error)?.dyn_into().map_err(js_error)?;
    let body = JsFuture::from(response.text().map_err(js_error)?).await.map_err(js_error)?.as_string().unwrap_or_default();
    if u64::try_from(body.len()).unwrap_or(u64::MAX) > MAX_RESPONSE_BYTES {
        return Err(format!("response larger than {MAX_RESPONSE_BYTES} bytes"));
    }
    Ok(HttpResponse { status: response.status(), body })
}
