use super::{ChatDelta, ChatRequest, ChatResponse, ChatStream, LlmProvider, ProviderError};
use async_trait::async_trait;
use futures_util::StreamExt;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio_stream::wrappers::LinesStream;
use std::process::Stdio;

#[derive(Clone)]
pub struct CliPassthroughProvider {
    pub name_: String,
    pub command: String,
    pub args: Vec<String>,
    pub stream_capable: bool,
    pub prompt_mode: PromptMode,
    pub strip_ansi: bool,
    pub timeout_ms: Option<u64>,
    pub cwd: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub session_arg: Option<String>,
}

#[derive(Clone, Copy)]
pub enum PromptMode { Raw, Prefixed }

impl CliPassthroughProvider {
    pub fn claude() -> Self {
        Self { name_: "claude-cli".into(), command: "claude".into(), args: vec![], stream_capable: true, prompt_mode: PromptMode::Prefixed, strip_ansi: true, timeout_ms: None, cwd: None, env: None, session_arg: None }
    }
    pub fn codex() -> Self {
        Self { name_: "codex-cli".into(), command: "codex".into(), args: vec![], stream_capable: true, prompt_mode: PromptMode::Prefixed, strip_ansi: true, timeout_ms: None, cwd: None, env: None, session_arg: None }
    }
    pub fn gemini_with_model(model: Option<String>) -> Self {
        let mut args = vec![];
        if let Some(m) = model { args.push("--model".into()); args.push(m); }
        Self { name_: "gemini-cli".into(), command: "gemini".into(), args, stream_capable: true, prompt_mode: PromptMode::Prefixed, strip_ansi: true, timeout_ms: None, cwd: None, env: None, session_arg: None }
    }
    pub fn custom(name: String, command: String, args: Vec<String>, stream_capable: bool, prompt_mode: PromptMode, strip_ansi: bool, timeout_ms: Option<u64>, cwd: Option<String>, env: Option<std::collections::HashMap<String, String>>, session_arg: Option<String>) -> Self {
        Self { name_: name, command, args, stream_capable, prompt_mode, strip_ansi, timeout_ms, cwd, env, session_arg }
    }
}

fn build_prompt(req: &ChatRequest, mode: PromptMode) -> String {
    match mode {
        PromptMode::Raw => {
            // Raw: just use the last user message or concatenate messages
            if let Some(last) = req.messages.iter().rev().find(|m| m.role == "user") {
                last.content.clone()
            } else {
                req.messages.iter().map(|m| m.content.clone()).collect::<Vec<_>>().join("\n\n")
            }
        }
        PromptMode::Prefixed => {
            let mut s = String::new();
            if let Some(sys) = &req.system { s.push_str(&format!("System: {}\n\n", sys)); }
            for m in &req.messages {
                match m.role.as_str() {
                    "user" => s.push_str(&format!("User: {}\n", m.content)),
                    "assistant" => s.push_str(&format!("Assistant: {}\n", m.content)),
                    _ => {}
                }
            }
            s
        }
    }
}

fn strip_ansi_if(text: String, enabled: bool) -> String {
    if !enabled { return text; }
    let bytes = strip_ansi_escapes::strip(text);
    String::from_utf8_lossy(&bytes).into_owned()
}

#[async_trait]
impl LlmProvider for CliPassthroughProvider {
    fn name(&self) -> &str { &self.name_ }
    fn default_model(&self) -> &str { "default" }

    async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        Ok(vec!["default".to_string()])
    }

    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, ProviderError> {
        let mut cmd = Command::new(&self.command);
        let mut args = self.args.clone();
        if let (Some(flag), Some(id)) = (&self.session_arg, &req.session_id) { args.push(flag.clone()); args.push(id.clone()); }
        cmd.args(&args);
        if let Some(cwd) = &self.cwd { cmd.current_dir(cwd); }
        if let Some(env) = &self.env { for (k,v) in env { cmd.env(k, v); } }
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| ProviderError::Other(format!("spawn {}: {}", self.command, e)))?;

        // Write prompt
        if let Some(mut stdin) = child.stdin.take() {
            let prompt = build_prompt(&req, self.prompt_mode);
            stdin.write_all(prompt.as_bytes()).await.map_err(|e| ProviderError::Other(format!("stdin write: {}", e)))?;
            let _ = stdin.shutdown().await;
        }

        let fut = child.wait_with_output();
        let out = if let Some(ms) = self.timeout_ms { tokio::time::timeout(std::time::Duration::from_millis(ms), fut).await.map_err(|_| ProviderError::Other("timeout".into()))?? } else { fut.await? };

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(ProviderError::Other(format!("{} failed: {}", self.command, stderr)));
        }

        let mut response = String::from_utf8_lossy(&out.stdout).to_string();
        response = strip_ansi_if(response, self.strip_ansi);

        Ok(ChatResponse { content: Some(response), tool_calls: None, usage: None })
    }

    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream, ProviderError> {
        if !self.stream_capable { // fallback
            let resp = self.chat(req).await?;
            let text = resp.content.unwrap_or_default();
            let stream = futures_util::stream::once(async move { Ok(ChatDelta { delta: Some(text), tool_calls: None }) }).boxed();
            return Ok(stream);
        }

        let mut cmd = Command::new(&self.command);
        let mut args = self.args.clone();
        if let (Some(flag), Some(id)) = (&self.session_arg, &req.session_id) { args.push(flag.clone()); args.push(id.clone()); }
        cmd.args(&args);
        if let Some(cwd) = &self.cwd { cmd.current_dir(cwd); }
        if let Some(env) = &self.env { for (k,v) in env { cmd.env(k, v); } }
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| ProviderError::Other(format!("spawn {}: {}", self.command, e)))?;

        if let Some(mut stdin) = child.stdin.take() {
            let prompt = build_prompt(&req, self.prompt_mode);
            let _ = stdin.write_all(prompt.as_bytes()).await;
            let _ = stdin.shutdown().await;
        }

        let stdout = child.stdout.take().ok_or_else(|| ProviderError::Other("capture stdout".into()))?;
        let reader = BufReader::new(stdout);
        let lines = reader.lines();
        let strip = self.strip_ansi;
        let stream = LinesStream::new(lines).map(move |line_res| {
            match line_res {
                Ok(line) => {
                    let delta = strip_ansi_if(line, strip);
                    Ok(ChatDelta { delta: Some(delta + "\n"), tool_calls: None })
                }
                Err(e) => Err(ProviderError::Other(format!("stream: {}", e))),
            }
        }).boxed();

        Ok(stream)
    }
}
