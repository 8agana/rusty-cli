use super::{
    ChatDelta, ChatRequest, ChatResponse, ChatStream, LlmProvider, ProviderError, ToolCall,
};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct OpenAiProvider {
    client: Client,
    base_url: String,
    api_key: String,
    default_model: String,
}

impl OpenAiProvider {
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
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }
    fn default_model(&self) -> &str {
        &self.default_model
    }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
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
            #[serde(skip_serializing_if = "Option::is_none")]
            tools: Option<Vec<ToolWrapper<'a>>>,
        }
        #[derive(Serialize)]
        struct ToolWrapper<'a> {
            r#type: &'a str,
            function: Function<'a>,
        }
        #[derive(Serialize)]
        struct Function<'a> {
            name: &'a str,
            description: &'a str,
            parameters: &'a serde_json::Value,
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

        let tools: Option<Vec<ToolWrapper>> = req.tools.as_ref().map(|ts| {
            ts.iter()
                .map(|t| ToolWrapper {
                    r#type: "function",
                    function: Function {
                        name: &t.name,
                        description: &t.description,
                        parameters: &t.parameters,
                    },
                })
                .collect()
        });

        let body = Body {
            model: &req.model,
            messages,
            temperature: req.temperature,
            max_tokens: req.max_tokens,
            stream: false,
            tools,
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
            #[serde(skip_serializing_if = "Option::is_none")]
            tools: Option<Vec<ToolWrapper<'a>>>,
        }
        #[derive(Serialize)]
        struct ToolWrapper<'a> {
            r#type: &'a str,
            function: Function<'a>,
        }
        #[derive(Serialize)]
        struct Function<'a> {
            name: &'a str,
            description: &'a str,
            parameters: &'a serde_json::Value,
        }
        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct StreamFunction {
            name: Option<String>,
            arguments: Option<String>,
        }
        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct ToolDelta {
            index: usize,
            id: Option<String>,
            r#type: Option<String>,
            function: Option<StreamFunction>,
        }
        #[derive(Deserialize)]
        struct DeltaMsg {
            content: Option<String>,
            #[serde(default)]
            tool_calls: Vec<ToolDelta>,
        }
        #[derive(Deserialize)]
        struct Choice {
            delta: DeltaMsg,
            #[serde(default)]
            finish_reason: Option<String>,
        }
        #[derive(Deserialize)]
        struct Chunk {
            choices: Vec<Choice>,
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
        let tools: Option<Vec<ToolWrapper>> = req.tools.as_ref().map(|ts| {
            ts.iter()
                .map(|t| ToolWrapper {
                    r#type: "function",
                    function: Function {
                        name: &t.name,
                        description: &t.description,
                        parameters: &t.parameters,
                    },
                })
                .collect()
        });

        let body = Body {
            model: &req.model,
            messages,
            temperature: req.temperature,
            max_tokens: req.max_tokens,
            stream: true,
            tools,
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
                // OpenAI streams as SSE: lines starting with "data: " and a final [DONE]
                let mut deltas: Vec<Result<ChatDelta, ProviderError>> = Vec::new();
                let mut tool_triggered = false;
                for line in text.split('\n') {
                    let line = line.trim();
                    if !line.starts_with("data:") {
                        continue;
                    }
                    let data = line.trim_start_matches("data:").trim();
                    if data == "[DONE]" {
                        continue;
                    }
                    // Some gateways wrap with { "choices": [ {"delta": {"content": "..."}}]}
                    match serde_json::from_str::<Chunk>(data) {
                        Ok(c) => {
                            for choice in c.choices {
                                if let Some(fr) = &choice.finish_reason
                                    && fr == "tool_calls"
                                {
                                    tool_triggered = true;
                                }
                                if let Some(content) = choice.delta.content {
                                    deltas.push(Ok(ChatDelta {
                                        delta: Some(content),
                                        tool_calls: None,
                                    }));
                                }
                                if !choice.delta.tool_calls.is_empty() {
                                    tool_triggered = true;
                                }
                            }
                        }
                        Err(e) => {
                            deltas.push(Err(ProviderError::Serde(e)));
                        }
                    }
                }
                // Coalesce current chunk's deltas into a single delta for simplicity
                let merged = deltas.into_iter().collect::<Result<Vec<_>, _>>()?;
                let text = merged
                    .into_iter()
                    .filter_map(|d| d.delta)
                    .collect::<String>();
                Ok(ChatDelta {
                    delta: if text.is_empty() { None } else { Some(text) },
                    tool_calls: if tool_triggered { Some(vec![]) } else { None },
                })
            })
            .filter(|res| {
                let ok = res.as_ref().ok();
                let has_text = ok.and_then(|d| d.delta.as_ref()).is_some();
                let has_tools = ok.and_then(|d| d.tool_calls.as_ref()).is_some();
                futures_util::future::ready(has_text || has_tools)
            })
            .boxed();

        Ok(stream)
    }
}
