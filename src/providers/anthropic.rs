use super::{ChatDelta, ChatMessage, ChatRequest, ChatResponse, ChatStream, LlmProvider, ProviderError, ToolCall, ToolSpec};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct AnthropicProvider {
    client: Client,
    base_url: String,
    api_key: String,
    version: String,
    default_model: String,
}

impl AnthropicProvider {
    pub fn new(base_url: String, api_key: String, version: String, default_model: String) -> Self {
        let client = Client::builder().build().expect("reqwest client");
        Self { client, base_url, api_key, version, default_model }
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str { "anthropic" }
    fn default_model(&self) -> &str { &self.default_model }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        // Anthropic doesn't provide a public list models endpoint without enterprise; return common defaults
        Ok(vec![self.default_model.clone()])
    }

    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        #[derive(Serialize)]
        struct Text { r#type: &'static str, text: String }
        #[derive(Serialize)]
        struct ToolResult { r#type: &'static str, tool_use_id: String, content: String }
        #[derive(Serialize)]
        struct ReqMsg { role: &'static str, content: serde_json::Value }
        #[derive(Serialize)]
        struct Tool<'a> { name: &'a str, description: &'a str, input_schema: &'a serde_json::Value }
        #[derive(Serialize)]
        struct Body<'a> {
            model: &'a str,
            messages: Vec<ReqMsg>,
            #[serde(skip_serializing_if = "Option::is_none")] system: Option<&'a str>,
            max_tokens: u32,
            #[serde(skip_serializing_if = "Option::is_none")] temperature: Option<f32>,
            stream: bool,
            #[serde(skip_serializing_if = "Option::is_none")] tools: Option<Vec<Tool<'a>>>,
        }
        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum RespContent {
            Text { text: String },
            ToolUse { id: String, name: String, input: serde_json::Value },
        }
        #[derive(Deserialize)]
        struct Resp { content: Vec<RespContent> }

        let mut messages: Vec<ReqMsg> = Vec::new();
        for m in &req.messages {
            if m.role == "tool" {
                if let Some(id) = &m.tool_call_id {
                    let block = ToolResult { r#type: "tool_result", tool_use_id: id.clone(), content: m.content.clone() };
                    let content = serde_json::json!([block]);
                    messages.push(ReqMsg { role: "user", content });
                }
            } else {
                let block = Text { r#type: "text", text: m.content.clone() };
                let content = serde_json::json!([block]);
                messages.push(ReqMsg { role: "user", content });
            }
        }
        let tools: Option<Vec<Tool>> = req.tools.as_ref().map(|ts| ts.iter().map(|t| Tool { name: &t.name, description: &t.description, input_schema: &t.parameters }).collect());
        let max_tokens = req.max_tokens.unwrap_or(1024);
        let body = Body { model: &req.model, messages, system: req.system.as_deref(), max_tokens, temperature: req.temperature, stream: false, tools };

        let url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));
        let resp: Resp = self.client
            .post(url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", &self.version)
            .json(&body)
            .send().await?
            .error_for_status()?
            .json().await?;

        // If any tool_use blocks appear, return tool_calls; otherwise return text
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut text_acc = String::new();
        for c in resp.content.into_iter() {
            match c {
                RespContent::Text { text } => text_acc.push_str(&text),
                RespContent::ToolUse { id, name, input } => tool_calls.push(ToolCall { id: Some(id), name, arguments: input }),
            }
        }
        if !tool_calls.is_empty() {
            Ok(ChatResponse { content: None, tool_calls: Some(tool_calls), usage: None })
        } else {
            Ok(ChatResponse { content: Some(text_acc), tool_calls: None, usage: None })
        }
    }

    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream, ProviderError> {
        #[derive(Serialize)]
        struct Text { r#type: &'static str, text: String }
        #[derive(Serialize)]
        struct ToolResult { r#type: &'static str, tool_use_id: String, content: String }
        #[derive(Serialize)]
        struct ReqMsg { role: &'static str, content: serde_json::Value }
        #[derive(Serialize)]
        struct Tool<'a> { name: &'a str, description: &'a str, input_schema: &'a serde_json::Value }
        #[derive(Serialize)]
        struct Body<'a> {
            model: &'a str,
            messages: Vec<ReqMsg>,
            #[serde(skip_serializing_if = "Option::is_none")] system: Option<&'a str>,
            max_tokens: u32,
            #[serde(skip_serializing_if = "Option::is_none")] temperature: Option<f32>,
            stream: bool,
            #[serde(skip_serializing_if = "Option::is_none")] tools: Option<Vec<Tool<'a>>>,
        }
        #[derive(Deserialize)]
        struct Delta { r#type: String, #[serde(default)] delta: Option<TextDelta> }
        #[derive(Deserialize)]
        struct TextDelta { #[serde(default)] text: String }

        let mut messages: Vec<ReqMsg> = Vec::new();
        for m in &req.messages {
            if m.role == "tool" {
                if let Some(id) = &m.tool_call_id {
                    let block = ToolResult { r#type: "tool_result", tool_use_id: id.clone(), content: m.content.clone() };
                    let content = serde_json::json!([block]);
                    messages.push(ReqMsg { role: "user", content });
                }
            } else {
                let block = Text { r#type: "text", text: m.content.clone() };
                let content = serde_json::json!([block]);
                messages.push(ReqMsg { role: "user", content });
            }
        }
        let tools: Option<Vec<Tool>> = req.tools.as_ref().map(|ts| ts.iter().map(|t| Tool { name: &t.name, description: &t.description, input_schema: &t.parameters }).collect());
        let max_tokens = req.max_tokens.unwrap_or(1024);
        let body = Body { model: &req.model, messages, system: req.system.as_deref(), max_tokens, temperature: req.temperature, stream: true, tools };

        let url = format!("{}/v1/messages", self.base_url.trim_end_matches('/'));
        let resp = self.client
            .post(url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", &self.version)
            .json(&body)
            .send().await?
            .error_for_status()?;

        let stream = resp.bytes_stream().map(|chunk_res| {
            let bytes = match chunk_res { Ok(b) => b, Err(e) => return Err(ProviderError::Http(e)) };
            let text = String::from_utf8_lossy(&bytes);
            let mut out = String::new();
            for line in text.split('\n') {
                let line = line.trim();
                if !line.starts_with("data:") { continue; }
                let data = line.trim_start_matches("data:").trim();
                if data.is_empty() || data == "[DONE]" { continue; }
                if let Ok(ev) = serde_json::from_str::<Delta>(data) {
                    if ev.r#type == "content_block_delta" {
                        if let Some(d) = ev.delta { out.push_str(&d.text); }
                    }
                }
            }
            Ok(super::ChatDelta { delta: if out.is_empty() { None } else { Some(out) }, tool_calls: None })
        }).filter(|res| futures_util::future::ready(res.as_ref().ok().and_then(|d| d.delta.as_ref()).is_some())).boxed();

        Ok(stream)
    }
}
