//! The OpenRouter API (<https://openrouter.ai/api/v1/>): request builders and response parsers.
//!
//! Sending is the caller's job (ureq on the desktop, `fetch` in the browser), so everything here is
//! pure and testable without a network. The error texts match the Python app's.

use std::fmt;

use serde_json::{Value, json};

pub const BASE_URL: &str = "https://openrouter.ai/api/v1/";

/// Response bodies larger than this are refused. The model catalogue is a few MiB at most.
pub const MAX_RESPONSE_BYTES: u64 = 32 * 1024 * 1024;

/// Why a request could not be made or its answer used. `Display` is the text shown to the user.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AiError {
    #[error("Error: AI service is not configured. Please set your API key in settings.")]
    NoApiKey,
    #[error("Error: Model is not configured. Please select a model for this prompt.")]
    NoModel,
    #[error("Error: No text to process.")]
    NoText,
    #[error("Error: API returned {0}")]
    Status(u16),
    #[error("Error: No response from AI service.")]
    EmptyResponse,
    #[error("Error: Unexpected response from OpenRouter.")]
    Unexpected,
    #[error("Error: Network error - {0}")]
    Network(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

/// A request for the transport to send, authorised with `Authorization: Bearer <api_key>`.
#[derive(Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub api_key: String,
    /// JSON body (POST only).
    pub body: Option<String>,
}

impl fmt::Debug for HttpRequest {
    /// Never prints the API key, so a request can be logged.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("api_key", &"<redacted>")
            .field("body_len", &self.body.as_ref().map(String::len))
            .finish()
    }
}

/// What the transport got back. Network failures are reported separately, as text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

impl HttpResponse {
    fn json(&self) -> Result<Value, AiError> {
        if self.status >= 400 {
            log::error!("OpenRouter API error: {} - {}", self.status, excerpt(&self.body));
            return Err(AiError::Status(self.status));
        }
        serde_json::from_str(&self.body).map_err(|e| {
            log::error!("OpenRouter returned invalid JSON: {e}");
            AiError::Unexpected
        })
    }
}

fn get(path: &str, api_key: &str) -> Result<HttpRequest, AiError> {
    if api_key.trim().is_empty() {
        return Err(AiError::NoApiKey);
    }
    Ok(HttpRequest { method: Method::Get, url: format!("{BASE_URL}{path}"), api_key: api_key.trim().to_owned(), body: None })
}

/// `POST chat/completions`: the system prompt as a real `system` message (left out when blank),
/// then the user's text.
pub fn completion_request(api_key: &str, system_prompt: &str, user_text: &str, model: &str) -> Result<HttpRequest, AiError> {
    if api_key.trim().is_empty() {
        return Err(AiError::NoApiKey);
    }
    if model.trim().is_empty() {
        return Err(AiError::NoModel);
    }
    if user_text.is_empty() {
        return Err(AiError::NoText);
    }
    let mut messages = Vec::with_capacity(2);
    if !system_prompt.trim().is_empty() {
        messages.push(json!({ "role": "system", "content": system_prompt }));
    }
    messages.push(json!({ "role": "user", "content": user_text }));
    let body = json!({ "model": model.trim(), "messages": messages }).to_string();
    Ok(HttpRequest { method: Method::Post, url: format!("{BASE_URL}chat/completions"), api_key: api_key.trim().to_owned(), body: Some(body) })
}

/// `GET models`: the model catalogue.
pub fn models_request(api_key: &str) -> Result<HttpRequest, AiError> {
    get("models", api_key)
}

/// `GET credits`: the account balance.
pub fn balance_request(api_key: &str) -> Result<HttpRequest, AiError> {
    get("credits", api_key)
}

/// The text of the first choice. A missing or `null` content is an empty answer, not an error.
pub fn parse_completion(response: &HttpResponse) -> Result<String, AiError> {
    let value = response.json()?;
    let Some(first) = value.get("choices").and_then(Value::as_array).and_then(|c| c.first()) else {
        if let Some(error) = value.get("error") {
            log::error!("OpenRouter reported an error: {}", excerpt(&error.to_string()));
        }
        log::warn!("OpenRouter returned empty response.");
        return Err(AiError::EmptyResponse);
    };
    Ok(first.pointer("/message/content").and_then(Value::as_str).unwrap_or_default().to_owned())
}

/// A model from the catalogue, reduced to what the UI shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub context_length: Option<u64>,
    /// Dollars per prompt token (OpenRouter sends it as a string, sometimes as a number).
    pub prompt_price: Option<f64>,
}

impl ModelInfo {
    pub fn is_valid(&self) -> bool {
        !self.id.trim().is_empty() && !self.name.trim().is_empty()
    }

    fn from_value(value: &Value) -> Self {
        let text = |key: &str| value.get(key).and_then(Value::as_str).unwrap_or_default().to_owned();
        Self {
            id: text("id"),
            name: text("name"),
            description: text("description"),
            context_length: value.get("context_length").and_then(as_count),
            prompt_price: value.pointer("/pricing/prompt").and_then(as_number),
        }
    }

    /// Whether `filter` (lower case) occurs in the name or the id.
    pub fn matches(&self, filter: &str) -> bool {
        filter.is_empty() || self.name.to_lowercase().contains(filter) || self.id.to_lowercase().contains(filter)
    }
}

/// The catalogue: valid models only, cheapest first (unknown prices last), then by name.
pub fn parse_models(response: &HttpResponse) -> Result<Vec<ModelInfo>, AiError> {
    let value = response.json()?;
    let Some(data) = value.get("data").and_then(Value::as_array) else {
        log::warn!("OpenRouter returned empty models list.");
        return Err(AiError::EmptyResponse);
    };
    let mut models: Vec<ModelInfo> = data.iter().map(ModelInfo::from_value).filter(ModelInfo::is_valid).collect();
    models.sort_by(|a, b| {
        let price = |m: &ModelInfo| m.prompt_price.unwrap_or(f64::INFINITY);
        price(a).total_cmp(&price(b)).then_with(|| a.name.cmp(&b.name))
    });
    Ok(models)
}

/// The account balance in dollars.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Balance {
    pub total: f64,
    pub used: f64,
}

impl Balance {
    pub fn remaining(&self) -> f64 {
        self.total - self.used
    }
}

impl fmt::Display for Balance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Total credits: ${:.2}\nUsed: ${:.2}\nRemaining: ${:.2}", self.total, self.used, self.remaining())
    }
}

/// `{"data": {"total_credits": …, "total_usage": …}}`; missing values count as zero.
pub fn parse_balance(response: &HttpResponse) -> Result<Balance, AiError> {
    let value = response.json()?;
    let field = |key: &str| match value.pointer(&format!("/data/{key}")) {
        None | Some(Value::Null) => Some(0.0),
        Some(v) => as_number(v),
    };
    match (field("total_credits"), field("total_usage")) {
        (Some(total), Some(used)) => Ok(Balance { total, used }),
        _ => {
            log::warn!("Unexpected balance payload: {}", excerpt(&value.to_string()));
            Err(AiError::Unexpected)
        }
    }
}

/// A finite number, given as a JSON number or a numeric string.
fn as_number(value: &Value) -> Option<f64> {
    let n = match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }?;
    n.is_finite().then_some(n)
}

/// A non-negative whole number, given as a JSON number or a numeric string.
fn as_count(value: &Value) -> Option<u64> {
    if let Some(n) = value.as_u64() {
        return Some(n);
    }
    let n = as_number(value)?;
    // `as` saturates at u64::MAX; the fractional part is irrelevant for a token count.
    (n >= 0.0).then_some(n as u64)
}

/// The first 500 characters, for logs.
fn excerpt(text: &str) -> &str {
    match text.char_indices().nth(500) {
        Some((end, _)) => text.get(..end).unwrap_or(text),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn ok(body: &str) -> HttpResponse {
        HttpResponse { status: 200, body: body.to_owned() }
    }

    #[test]
    fn completion_request_sends_a_real_system_message() {
        let r = completion_request(" key ", "Fix: ", "teh text", "m/x").unwrap();
        assert_eq!(r.method, Method::Post);
        assert_eq!(r.url, "https://openrouter.ai/api/v1/chat/completions");
        assert_eq!(r.api_key, "key");
        let body: Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(
            body,
            json!({"model": "m/x", "messages": [
                {"role": "system", "content": "Fix: "},
                {"role": "user", "content": "teh text"}]})
        );
    }

    #[test]
    fn a_blank_system_prompt_is_left_out() {
        let r = completion_request("k", "  ", "text", "m").unwrap();
        let body: Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn completion_request_checks_its_inputs_in_order() {
        assert_eq!(completion_request("", "s", "t", "m"), Err(AiError::NoApiKey));
        assert_eq!(completion_request("k", "s", "t", " "), Err(AiError::NoModel));
        assert_eq!(completion_request("k", "s", "", "m"), Err(AiError::NoText));
    }

    #[test]
    fn the_debug_output_hides_the_key() {
        let r = models_request("sk-or-secret").unwrap();
        assert!(!format!("{r:?}").contains("secret"));
    }

    #[test]
    fn parses_a_completion() {
        let body = r#"{"id":"x","choices":[{"message":{"role":"assistant","content":"Fixed."},"finish_reason":"stop"}]}"#;
        assert_eq!(parse_completion(&ok(body)), Ok("Fixed.".to_owned()));
        let null_content = r#"{"choices":[{"message":{"role":"assistant","content":null}}]}"#;
        assert_eq!(parse_completion(&ok(null_content)), Ok(String::new()));
    }

    #[test]
    fn completion_errors() {
        assert_eq!(parse_completion(&HttpResponse { status: 401, body: "{}".into() }), Err(AiError::Status(401)));
        assert_eq!(parse_completion(&ok(r#"{"choices":[]}"#)), Err(AiError::EmptyResponse));
        assert_eq!(parse_completion(&ok(r#"{"error":{"message":"rate limited"}}"#)), Err(AiError::EmptyResponse));
        assert_eq!(parse_completion(&ok("<html>")), Err(AiError::Unexpected));
        assert_eq!(AiError::Status(429).to_string(), "Error: API returned 429");
    }

    #[test]
    fn parses_and_sorts_models() {
        let body = r#"{"data":[
            {"id":"b/paid","name":"Paid","context_length":128000,"pricing":{"prompt":"0.000002"}},
            {"id":"a/free","name":"Free one","description":"d","context_length":1048576,"pricing":{"prompt":"0"}},
            {"id":"c/unknown","name":"Unknown price"},
            {"id":"","name":"no id"},
            {"id":"d/num","name":"Numeric","pricing":{"prompt":0.000001}}
        ]}"#;
        let models = parse_models(&ok(body)).unwrap();
        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["a/free", "d/num", "b/paid", "c/unknown"]);
        assert_eq!(models[0].context_length, Some(1_048_576));
        assert_eq!(models[0].description, "d");
        assert_eq!(models[2].prompt_price, Some(0.000002));
        assert_eq!(models[3].prompt_price, None);
    }

    #[test]
    fn model_filter_matches_name_or_id() {
        let m = ModelInfo { id: "google/gemma".into(), name: "Gemma 4".into(), ..Default::default() };
        assert!(m.matches(""));
        assert!(m.matches("gemma 4"));
        assert!(m.matches("google/"));
        assert!(!m.matches("claude"));
    }

    #[test]
    fn parses_the_balance() {
        let b = parse_balance(&ok(r#"{"data":{"total_credits":20,"total_usage":"10.32"}}"#)).unwrap();
        assert_eq!(b.to_string(), "Total credits: $20.00\nUsed: $10.32\nRemaining: $9.68");
        assert_eq!(parse_balance(&ok(r#"{"data":{}}"#)).unwrap(), Balance { total: 0.0, used: 0.0 });
        assert_eq!(parse_balance(&ok(r#"{"data":{"total_credits":"lots"}}"#)), Err(AiError::Unexpected));
        assert_eq!(parse_balance(&HttpResponse { status: 403, body: String::new() }), Err(AiError::Status(403)));
    }

    #[test]
    fn excerpt_cuts_at_a_char_boundary() {
        let long = "ж".repeat(600);
        assert_eq!(excerpt(&long).chars().count(), 500);
        assert_eq!(excerpt("short"), "short");
    }

    proptest! {
        #[test]
        fn parsers_never_panic(status in 0u16..600, body in ".{0,256}") {
            let r = HttpResponse { status, body };
            let _ = parse_completion(&r);
            let _ = parse_models(&r);
            let _ = parse_balance(&r);
        }

        #[test]
        fn model_parsing_never_panics_on_odd_values(price in any::<f64>(), ctx in any::<f64>(), s in ".{0,20}") {
            let body = json!({"data": [
                {"id": s, "name": "n", "context_length": ctx, "pricing": {"prompt": price}},
                {"id": "x", "name": "n", "context_length": s, "pricing": {"prompt": s}},
            ]}).to_string();
            let _ = parse_models(&HttpResponse { status: 200, body });
        }
    }
}
