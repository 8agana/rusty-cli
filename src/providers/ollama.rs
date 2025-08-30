use super::{ChatDelta, ChatRequest, ChatResponse, ChatStream, LlmProvider, ProviderError};
use async_trait::async_trait;
use futures_util::{StreamExt, TryStreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct OllamaProvider {
    client: Client,
    base_url: String,
    default_model: String,
}

impl OllamaProvider {
    pub fn new(base_url: String, default_model: String) -> Self {
        let client = Client::builder().build().expect("reqwest client");
        Self { client, base_url, default_model }
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    fn name(&self) -> &str { "ollama" }
    fn default_model(&self) -> &str { &self.default_model }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        #[derive(Deserialize)]
        struct Model { name: String }
        #[derive(Deserialize)]
        struct Resp { models: Vec<Model> }
        let url = format!("{}/api/tags", self.base_url.trim_end_matches('/'));
        let resp: Resp = self.client
            .get(url)
            .send().await?
            .error_for_status()?
            .json().await?;
        Ok(resp.models.into_iter().map(|m| m.name).collect())
    }

    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        #[derive(Serialize)]
        struct Msg<'a> { role: &'a str, content: &'a str }
        #[derive(Serialize)]
        struct Body<'a> {
            model: &'a str,
            messages: Vec<Msg<'a>>,
            stream: bool,
            options: Options,
        }
        #[derive(Serialize, Default)]
        struct Options { temperature: Option<f32>, num_predict: Option<u32> }
        #[derive(Deserialize)]
        struct RespMsg { content: String }
        #[derive(Deserialize)]
        struct Resp { message: RespMsg }

        let mut messages: Vec<Msg> = Vec::new();
        if let Some(sys) = &req.system { messages.push(Msg{ role: "system", content: sys }); }
        for m in &req.messages { messages.push(Msg { role: &m.role, content: &m.content }); }

        let body = Body {
            model: &req.model,
            messages,
            stream: false,
            options: Options { temperature: req.temperature, num_predict: req.max_tokens },
        };

        let url = format!("{}/api/chat", self.base_url.trim_end_matches('/'));
        let resp: Resp = self.client
            .post(url)
            .json(&body)
            .send().await?
            .error_for_status()?
            .json().await?;
        Ok(ChatResponse { content: Some(resp.message.content), tool_calls: None, usage: None })
    }

    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream, ProviderError> {
        #[derive(Serialize)]
        struct Msg<'a> { role: &'a str, content: &'a str }
        #[derive(Serialize)]
        struct Body<'a> {
            model: &'a str,
            messages: Vec<Msg<'a>>,
            stream: bool,
            options: Options,
        }
        #[derive(Serialize, Default)]
        struct Options { temperature: Option<f32>, num_predict: Option<u32> }
        #[derive(Deserialize)]
        struct ChunkMsg { content: String }
        #[derive(Deserialize)]
        struct Chunk { done: bool, message: Option<ChunkMsg> }

        let mut messages: Vec<Msg> = Vec::new();
        if let Some(sys) = &req.system { messages.push(Msg{ role: "system", content: sys }); }
        for m in &req.messages { messages.push(Msg { role: &m.role, content: &m.content }); }

        let body = Body {
            model: &req.model,
            messages,
            stream: true,
            options: Options { temperature: req.temperature, num_predict: req.max_tokens },
        };

        let url = format!("{}/api/chat", self.base_url.trim_end_matches('/'));
        let resp = self.client
            .post(url)
            .json(&body)
            .send().await?
            .error_for_status()?;

        let stream = resp.bytes_stream().map(|res| {
            let bytes = match res { Ok(b) => b, Err(e) => return Err(ProviderError::Http(e)) };
            let text = String::from_utf8_lossy(&bytes);
            // Ollama streams NDJSON lines
            let mut acc = String::new();
            for line in text.split('\n') {
                let l = line.trim();
                if l.is_empty() { continue; }
                if let Ok(chunk) = serde_json::from_str::<Chunk>(l) {
                    if let Some(msg) = chunk.message { acc.push_str(&msg.content); }
                }
            }
            Ok(ChatDelta { delta: if acc.is_empty() { None } else { Some(acc) }, tool_calls: None })
        })
        .filter(|res| futures_util::future::ready(res.as_ref().ok().and_then(|d| d.delta.as_ref()).is_some()))
        .boxed();

        Ok(stream)
    }
}
