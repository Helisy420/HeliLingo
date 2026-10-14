//! OpenRouter: any chat model by the user's key, streamed (SSE).
//!
//! The stream is `data: {json}` events, `: OPENROUTER PROCESSING` comment
//! lines while the model warms up, and `data: [DONE]` at the end. An error
//! can come mid-stream as a chunk with an `error` object. Text goes to the
//! sink as it arrives; the cleaned full answer is the output. Cancelling =
//! dropping the future, which closes the connection (OpenRouter stops the
//! generation and billing on disconnect). No model is chosen by default.

use std::sync::{Arc, Mutex};

use eventsource_stream::Eventsource;
use futures_util::future::BoxFuture;
use futures_util::{FutureExt, StreamExt};
use hl_core::prompt::{self, Glossary};
use hl_core::{Engine, EngineError, EngineId, Job, Output, Sink};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::http;

const CHAT_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const MODELS_URL: &str = "https://openrouter.ai/api/v1/models";
const REFERER: &str = "https://github.com/Helisy420/HeliLingo";

/// Tokens used by the last answer (for statistics).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Usage {
    pub prompt: u64,
    pub completion: u64,
}

/// Told the tokens of every answer (Settings → Статистика).
pub type OnUsage = Arc<dyn Fn(Usage) + Send + Sync>;

pub struct OpenRouter {
    http: Client,
    key: Option<String>,
    model: Option<String>,
    glossary: Option<Arc<Glossary>>,
    /// Running total since start.
    pub usage: Mutex<Usage>,
    on_usage: Option<OnUsage>,
}

impl OpenRouter {
    pub fn new(http: Client, key: Option<String>, model: Option<String>) -> Self {
        Self {
            http,
            key: key.filter(|k| !k.trim().is_empty()),
            model: model.filter(|m| !m.trim().is_empty()),
            glossary: None,
            usage: Mutex::default(),
            on_usage: None,
        }
    }

    pub fn with_glossary(mut self, g: Arc<Glossary>) -> Self {
        self.glossary = Some(g);
        self
    }

    pub fn on_usage(mut self, f: OnUsage) -> Self {
        self.on_usage = Some(f);
        self
    }

    async fn run(&self, key: &str, model: &str, job: &Job, sink: &Sink) -> Result<Output, EngineError> {
        let terms = self.glossary.as_ref().map(|g| g.terms()).unwrap_or_default();
        let messages: Vec<serde_json::Value> = prompt::chat(&job.source, &job.target, &job.text, &job.context, &terms)
            .into_iter()
            .map(|(role, content)| serde_json::json!({ "role": role, "content": content }))
            .collect();
        let body = serde_json::json!({
            "model": model,
            "messages": messages,
            "stream": true,
            "temperature": 0.2,
            "usage": { "include": true },
        });
        let resp = self
            .http
            .post(CHAT_URL)
            .bearer_auth(key)
            .header("HTTP-Referer", REFERER)
            .header("X-Title", "HeliLingo")
            .json(&body)
            .send()
            .await
            .map_err(http::network)?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(error_from(status.as_u16(), &text));
        }

        let mut events = resp.bytes_stream().eventsource();
        let mut full = String::new();
        let mut leading = true;
        while let Some(ev) = events.next().await {
            let ev = ev.map_err(|e| EngineError::Network(e.to_string()))?;
            match parse_chunk(&ev.data)? {
                Chunk::Done => break,
                Chunk::Text(t) => {
                    // Models often start with a newline or a space.
                    let t = if leading { t.trim_start().to_owned() } else { t };
                    if !t.is_empty() {
                        leading = false;
                        sink.send(t.clone());
                        full.push_str(&t);
                    }
                }
                Chunk::Usage(u) => {
                    {
                        let mut total = self.usage.lock().unwrap();
                        total.prompt += u.prompt;
                        total.completion += u.completion;
                    }
                    if let Some(f) = &self.on_usage {
                        f(u);
                    }
                }
                Chunk::Nothing => {}
            }
        }
        let text = prompt::clean_output(&job.text, &full);
        if text.is_empty() {
            return Err(EngineError::Server("empty answer".into()));
        }
        Ok(Output { text, ..Output::default() })
    }
}

impl Engine for OpenRouter {
    fn id(&self) -> EngineId {
        EngineId::new("openrouter")
    }

    fn translate(self: Arc<Self>, job: Arc<Job>, sink: Sink) -> BoxFuture<'static, Result<Output, EngineError>> {
        async move {
            let Some(key) = self.key.clone() else { return Err(EngineError::NoKey) };
            let Some(model) = self.model.clone() else { return Err(EngineError::Other("no model chosen".into())) };
            self.run(&key, &model, &job, &sink).await
        }
        .boxed()
    }
}

#[derive(Debug, PartialEq)]
enum Chunk {
    Text(String),
    Usage(Usage),
    Done,
    Nothing,
}

fn parse_chunk(data: &str) -> Result<Chunk, EngineError> {
    let data = data.trim();
    if data == "[DONE]" {
        return Ok(Chunk::Done);
    }
    if data.is_empty() {
        return Ok(Chunk::Nothing);
    }
    let v: serde_json::Value = serde_json::from_str(data).map_err(|_| EngineError::Server("unexpected answer".into()))?;
    if let Some(err) = v.get("error") {
        let code = err["code"].as_u64().unwrap_or(500) as u16;
        return Err(error_from(code, &v.to_string()));
    }
    let choice = &v["choices"][0];
    if choice["finish_reason"].as_str() == Some("error") {
        return Err(EngineError::Server("the model stopped with an error".into()));
    }
    if let Some(t) = choice["delta"]["content"].as_str().filter(|t| !t.is_empty()) {
        return Ok(Chunk::Text(t.to_owned()));
    }
    if let Some(u) = v.get("usage").filter(|u| u.is_object()) {
        return Ok(Chunk::Usage(Usage {
            prompt: u["prompt_tokens"].as_u64().unwrap_or(0),
            completion: u["completion_tokens"].as_u64().unwrap_or(0),
        }));
    }
    Ok(Chunk::Nothing)
}

/// OpenRouter's status codes: 401 bad key, 402 no credits, 429 limits,
/// 400 bad model/request, 502/503 the model's provider is down.
fn error_from(status: u16, body: &str) -> EngineError {
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_owned))
        .unwrap_or_default();
    match status {
        401 | 403 => EngineError::Unauthorized,
        402 => EngineError::PaymentRequired,
        429 => EngineError::RateLimited,
        408 | 504 => EngineError::Timeout,
        400 | 404 if !message.is_empty() => EngineError::Other(message),
        s => EngineError::Server(if message.is_empty() { format!("HTTP {s}") } else { message }),
    }
}

/// A model in the picker.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub context: u64,
    /// US dollars per million tokens (prompt, completion).
    pub price_in: f64,
    pub price_out: f64,
}

/// The public model list (no key needed), text-out models only, sorted by name.
pub async fn models(http: &Client) -> Result<Vec<ModelInfo>, EngineError> {
    let resp = http.get(MODELS_URL).send().await.map_err(http::network)?;
    if !resp.status().is_success() {
        return Err(http::status(resp.status(), false));
    }
    parse_models(&resp.text().await.map_err(http::network)?)
}

fn parse_models(body: &str) -> Result<Vec<ModelInfo>, EngineError> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|_| EngineError::Server("unexpected answer".into()))?;
    let per_million = |p: &serde_json::Value| p.as_str().and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0) * 1e6;
    let mut list: Vec<ModelInfo> = v["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| {
            let out = &m["architecture"]["output_modalities"];
            !out.is_array() || out.as_array().unwrap().iter().any(|x| x == "text")
        })
        .filter_map(|m| {
            Some(ModelInfo {
                id: m["id"].as_str()?.to_owned(),
                name: m["name"].as_str().unwrap_or_default().to_owned(),
                context: m["context_length"].as_u64().unwrap_or(0),
                price_in: per_million(&m["pricing"]["prompt"]),
                price_out: per_million(&m["pricing"]["completion"]),
            })
        })
        // "openrouter/auto" etc. report -1 prices: drop negative ones.
        .filter(|m| m.price_in >= 0.0 && m.price_out >= 0.0)
        .collect();
    list.sort_by_key(|m| m.name.to_lowercase());
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_chunks() {
        assert_eq!(parse_chunk("[DONE]").unwrap(), Chunk::Done);
        assert_eq!(
            parse_chunk(r#"{"id":"x","choices":[{"index":0,"delta":{"role":"assistant","content":"При"},"finish_reason":null}]}"#).unwrap(),
            Chunk::Text("При".into())
        );
        assert_eq!(
            parse_chunk(r#"{"choices":[{"delta":{"content":""},"finish_reason":"stop"}],"usage":{"prompt_tokens":40,"completion_tokens":5}}"#).unwrap(),
            Chunk::Usage(Usage { prompt: 40, completion: 5 })
        );
        assert_eq!(parse_chunk(r#"{"choices":[{"delta":{}}]}"#).unwrap(), Chunk::Nothing);
        // Mid-stream errors.
        assert_eq!(parse_chunk(r#"{"error":{"code":402,"message":"Insufficient credits"}}"#).unwrap_err(), EngineError::PaymentRequired);
        assert!(parse_chunk(r#"{"choices":[{"delta":{"content":""},"finish_reason":"error"}]}"#).is_err());
    }

    #[test]
    fn status_errors() {
        assert_eq!(error_from(401, ""), EngineError::Unauthorized);
        assert_eq!(error_from(400, r#"{"error":{"message":"x/y is not a valid model ID"}}"#), EngineError::Other("x/y is not a valid model ID".into()));
        assert_eq!(error_from(503, ""), EngineError::Server("HTTP 503".into()));
    }

    #[test]
    fn model_list() {
        let body = r#"{"data":[
            {"id":"openai/gpt-5-mini","name":"OpenAI: GPT-5 Mini","context_length":400000,"pricing":{"prompt":"0.00000025","completion":"0.000002"},"architecture":{"output_modalities":["text"]}},
            {"id":"x/image","name":"Image gen","pricing":{"prompt":"0","completion":"0"},"architecture":{"output_modalities":["image"]}},
            {"id":"openrouter/auto","name":"Auto Router","pricing":{"prompt":"-1","completion":"-1"}},
            {"id":"google/gemma-3-27b-it:free","name":"Google: Gemma 3 27B (free)","context_length":96000,"pricing":{"prompt":"0","completion":"0"}}]}"#;
        let m = parse_models(body).unwrap();
        assert_eq!(m.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["google/gemma-3-27b-it:free", "openai/gpt-5-mini"]);
        assert!((m[1].price_in - 0.25).abs() < 1e-9 && (m[1].price_out - 2.0).abs() < 1e-9);
    }

    /// `cargo test -p hl-providers -- --ignored openrouter_models`
    #[tokio::test]
    #[ignore]
    async fn live_openrouter_models() {
        let m = models(&crate::client()).await.unwrap();
        println!("{} models, first {:?}", m.len(), m.first());
        assert!(m.len() > 50);
    }
}
