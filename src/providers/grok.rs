use super::{
    ChatDelta, ChatRequest, ChatResponse, ChatStream, LlmProvider, ProviderError, ToolCall,
};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct GrokProvider {
    client: Client,
    base_url: String,
    api_key: String,
    default_model: String,
}

impl GrokProvider {
    pub fn new(base_url: String, api_key: String, default_model: String) -> Self {
        let client = Client::builder().build().expect("reqwest client");
        Self {
            client,
            base_url,
            api_key,
            default_model,
        }
    }
}

#[async_trait]
impl LlmProvider for GrokProvider {
    fn name(&self) -> &str {
        "grok"
    }
    fn default_model(&self) -> &str {
        &self.default_model
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        // Assume OpenAI-compatible /models
        #[derive(Deserialize)]
        struct Model {
            id: String,
        }
        #[derive(Deserialize)]
        struct Resp {
            data: Vec<Model>,
        }
        let url = format!("{}/models", self.base_url.trim_end_matches('/'));
        let resp: Resp = self
            .client
            .get(url)
            .bearer_auth(&self.api_key)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp.data.into_iter().map(|m| m.id).collect())
    }

    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        #[derive(Serialize)]
        #[serde(tag = "role")]
        enum Msg<'a> {
            #[serde(rename = "system")]
            System { content: &'a str },
            #[serde(rename = "user")]
            User { content: &'a str },
            #[serde(rename = "tool")]
            Tool {
                content: &'a str,
                tool_call_id: &'a str,
                #[serde(skip_serializing_if = "Option::is_none")]
                name: Option<&'a str>,
            },
        }
        #[derive(Serialize)]
        struct Body<'a> {
            model: &'a str,
            messages: Vec<Msg<'a>>,
            temperature: Option<f32>,
            max_tokens: Option<u32>,
            stream: bool,
        }
        #[derive(Deserialize)]
        struct Choice {
            message: ChoiceMsg,
        }
        #[derive(Deserialize)]
        struct ChoiceMsg {
            content: Option<String>,
            #[serde(default)]
            tool_calls: Vec<ChoiceToolCall>,
        }
        #[derive(Deserialize)]
        struct ChoiceToolCall {
            id: String,
            function: ChoiceFunction,
        }
        #[derive(Deserialize)]
        struct ChoiceFunction {
            name: String,
            arguments: String,
        }
        #[derive(Deserialize)]
        struct Usage {
            prompt_tokens: u32,
            completion_tokens: u32,
            total_tokens: u32,
        }
        #[derive(Deserialize)]
        struct Resp {
            choices: Vec<Choice>,
            usage: Option<Usage>,
        }

        let mut messages: Vec<Msg> = Vec::new();
        if let Some(sys) = &req.system {
            messages.push(Msg::System { content: sys });
        }
        for m in &req.messages {
            match m.role.as_str() {
                "user" => messages.push(Msg::User {
                    content: &m.content,
                }),
                "tool" => {
                    if let Some(id) = m.tool_call_id.as_deref() {
                        messages.push(Msg::Tool {
                            content: &m.content,
                            tool_call_id: id,
                            name: m.name.as_deref(),
                        });
                    }
                }
                _ => {}
            }
        }
        let body = Body {
            model: &req.model,
            messages,
            temperature: req.temperature,
            max_tokens: req.max_tokens,
            stream: false,
        };
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let resp: Resp = self
            .client
            .post(url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let usage = resp.usage.map(|u| super::Usage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });
        let message = resp.choices.into_iter().next().map(|c| c.message);
        let content = message.as_ref().and_then(|m| m.content.clone());
        let tool_calls = message
            .map(|m| {
                m.tool_calls
                    .into_iter()
                    .map(|tc| {
                        let args: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                            .unwrap_or(serde_json::Value::Null);
                        ToolCall {
                            id: Some(tc.id),
                            name: tc.function.name,
                            arguments: args,
                        }
                    })
                    .collect()
            })
            .filter(|v: &Vec<_>| !v.is_empty());
        Ok(ChatResponse {
            content,
            tool_calls,
            usage,
        })
    }

    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream, ProviderError> {
        #[derive(Serialize)]
        struct Msg<'a> {
            role: &'a str,
            content: &'a str,
        }
        #[derive(Serialize)]
        struct Body<'a> {
            model: &'a str,
            messages: Vec<Msg<'a>>,
            temperature: Option<f32>,
            max_tokens: Option<u32>,
            stream: bool,
        }
        #[derive(Deserialize)]
        struct DeltaMsg {
            content: Option<String>,
        }
        #[derive(Deserialize)]
        struct Choice {
            delta: DeltaMsg,
        }
        #[derive(Deserialize)]
        struct Chunk {
            choices: Vec<Choice>,
        }

        let mut messages: Vec<Msg> = Vec::new();
        if let Some(sys) = &req.system {
            messages.push(Msg {
                role: "system",
                content: sys,
            });
        }
        for m in &req.messages {
            messages.push(Msg {
                role: &m.role,
                content: &m.content,
            });
        }
        let body = Body {
            model: &req.model,
            messages,
            temperature: req.temperature,
            max_tokens: req.max_tokens,
            stream: true,
        };
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .post(url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?
            .error_for_status()?;

        let stream = resp
            .bytes_stream()
            .map(|chunk_res| {
                let bytes = match chunk_res {
                    Ok(b) => b,
                    Err(e) => return Err(ProviderError::Http(e)),
                };
                let text = String::from_utf8_lossy(&bytes);
                let mut acc = String::new();
                for line in text.split('\n') {
                    let line = line.trim();
                    if !line.starts_with("data:") {
                        continue;
                    }
                    let data = line.trim_start_matches("data:").trim();
                    if data == "[DONE]" {
                        continue;
                    }
                    if let Ok(chunk) = serde_json::from_str::<Chunk>(data) {
                        for c in chunk.choices {
                            if let Some(d) = c.delta.content {
                                acc.push_str(&d);
                            }
                        }
                    }
                }
                Ok(ChatDelta {
                    delta: if acc.is_empty() { None } else { Some(acc) },
                    tool_calls: None,
                })
            })
            .filter(|res| {
                futures_util::future::ready(
                    res.as_ref().ok().and_then(|d| d.delta.as_ref()).is_some(),
                )
            })
            .boxed();

        Ok(stream)
    }
}
