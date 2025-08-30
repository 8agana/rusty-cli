use std::collections::{HashMap, HashSet};

use crate::config::Config;

use super::{
    LlmProvider, ProviderError, anthropic::AnthropicProvider,
    cli_passthrough::CliPassthroughProvider, deepseek::DeepSeekProvider, grok::GrokProvider,
    ollama::OllamaProvider, openai::OpenAiProvider,
};

pub struct ProviderRegistry {
    providers: HashMap<String, Box<dyn LlmProvider>>,
    cli_keys: HashSet<String>,
}

impl ProviderRegistry {
    pub fn from_config(cfg: &Config) -> Result<Self, ProviderError> {
        let mut map: HashMap<String, Box<dyn LlmProvider>> = HashMap::new();
        let mut cli: HashSet<String> = HashSet::new();

        if let Some(oc) = &cfg.openai {
            if let Some(key) = oc.effective_api_key() {
                let base = oc
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "https://api.openai.com/v1".into());
                let model = oc
                    .default_model
                    .clone()
                    .unwrap_or_else(|| "gpt-4o-mini".into());
                let p = OpenAiProvider::new(base, key, model);
                map.insert("openai".into(), Box::new(p));
            }
        } else if let Ok(key) = std::env::var("OPENAI_API_KEY") {
            let p = OpenAiProvider::new(
                "https://api.openai.com/v1".into(),
                key,
                "gpt-4o-mini".into(),
            );
            map.insert("openai".into(), Box::new(p));
        }

        if let Some(oc) = &cfg.ollama {
            let base = oc.effective_base_url();
            let model = oc
                .default_model
                .clone()
                .unwrap_or_else(|| "llama3.1".into());
            let p = OllamaProvider::new(base, model);
            map.insert("ollama".into(), Box::new(p));
        } else {
            // Provide sensible default for local dev
            let p = OllamaProvider::new("http://localhost:11434".into(), "llama3.1".into());
            map.insert("ollama".into(), Box::new(p));
        }

        // Anthropic
        if let Some(ac) = &cfg.anthropic {
            if let Some(key) = ac.effective_api_key() {
                let base = ac
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "https://api.anthropic.com".into());
                let version = ac.effective_version();
                let model = ac
                    .default_model
                    .clone()
                    .unwrap_or_else(|| "claude-3-5-sonnet-latest".into());
                let p = AnthropicProvider::new(base, key, version, model);
                map.insert("anthropic".into(), Box::new(p));
            }
        } else if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            let p = AnthropicProvider::new(
                "https://api.anthropic.com".into(),
                key,
                "2023-06-01".into(),
                "claude-3-5-sonnet-latest".into(),
            );
            map.insert("anthropic".into(), Box::new(p));
        }

        // Grok (xAI) - OpenAI compatible
        if let Some(gc) = &cfg.grok {
            if let Some(key) = gc.effective_api_key() {
                let base = gc
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "https://api.x.ai/v1".into());
                let model = gc
                    .default_model
                    .clone()
                    .unwrap_or_else(|| "grok-2-latest".into());
                let p = GrokProvider::new(base, key, model);
                map.insert("grok".into(), Box::new(p));
            }
        } else if let Ok(key) =
            std::env::var("XAI_API_KEY").or_else(|_| std::env::var("GROK_API_KEY"))
        {
            let p = GrokProvider::new("https://api.x.ai/v1".into(), key, "grok-2-latest".into());
            map.insert("grok".into(), Box::new(p));
        }

        // DeepSeek - OpenAI compatible
        if let Some(dc) = &cfg.deepseek {
            if let Some(key) = dc.effective_api_key() {
                let base = dc
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "https://api.deepseek.com".into());
                let model = dc
                    .default_model
                    .clone()
                    .unwrap_or_else(|| "deepseek-chat".into());
                let p = DeepSeekProvider::new(base, key, model);
                map.insert("deepseek".into(), Box::new(p));
            }
        } else if let Ok(key) = std::env::var("DEEPSEEK_API_KEY") {
            let p = DeepSeekProvider::new(
                "https://api.deepseek.com".into(),
                key,
                "deepseek-chat".into(),
            );
            map.insert("deepseek".into(), Box::new(p));
        }

        // CLI passthrough providers (disabled by default)
        if let Some(c) = &cfg.claude_cli
            && c.enabled.unwrap_or(false)
        {
            let prov = if let Some(cmd) = &c.command {
                CliPassthroughProvider::custom(
                    "claude-cli".into(),
                    cmd.clone(),
                    c.args.clone().unwrap_or_default(),
                    c.stream_capable.unwrap_or(true),
                    match c.prompt_mode.as_deref() {
                        Some("raw") => super::cli_passthrough::PromptMode::Raw,
                        _ => super::cli_passthrough::PromptMode::Prefixed,
                    },
                    c.strip_ansi.unwrap_or(true),
                    c.timeout_ms,
                    c.cwd.clone(),
                    c.env.clone(),
                    c.session_arg.clone(),
                )
            } else {
                CliPassthroughProvider::claude()
            };
            map.insert("claude-cli".into(), Box::new(prov));
            cli.insert("claude-cli".into());
        }
        if let Some(c) = &cfg.codex_cli
            && c.enabled.unwrap_or(false)
        {
            let prov = if let Some(cmd) = &c.command {
                CliPassthroughProvider::custom(
                    "codex-cli".into(),
                    cmd.clone(),
                    c.args.clone().unwrap_or_default(),
                    c.stream_capable.unwrap_or(true),
                    match c.prompt_mode.as_deref() {
                        Some("raw") => super::cli_passthrough::PromptMode::Raw,
                        _ => super::cli_passthrough::PromptMode::Prefixed,
                    },
                    c.strip_ansi.unwrap_or(true),
                    c.timeout_ms,
                    c.cwd.clone(),
                    c.env.clone(),
                    c.session_arg.clone(),
                )
            } else {
                CliPassthroughProvider::codex()
            };
            map.insert("codex-cli".into(), Box::new(prov));
            cli.insert("codex-cli".into());
        }
        if let Some(c) = &cfg.gemini_cli
            && c.enabled.unwrap_or(false)
        {
            let prov = if let Some(cmd) = &c.command {
                CliPassthroughProvider::custom(
                    "gemini-cli".into(),
                    cmd.clone(),
                    c.args.clone().unwrap_or_default(),
                    c.stream_capable.unwrap_or(true),
                    match c.prompt_mode.as_deref() {
                        Some("raw") => super::cli_passthrough::PromptMode::Raw,
                        _ => super::cli_passthrough::PromptMode::Prefixed,
                    },
                    c.strip_ansi.unwrap_or(true),
                    c.timeout_ms,
                    c.cwd.clone(),
                    c.env.clone(),
                    c.session_arg.clone(),
                )
            } else {
                CliPassthroughProvider::gemini_with_model(None)
            };
            map.insert("gemini-cli".into(), Box::new(prov));
            cli.insert("gemini-cli".into());
        }
        if let Some(custom) = &cfg.custom_cli_providers {
            for (name, c) in custom {
                if c.enabled.unwrap_or(false)
                    && let Some(cmd) = &c.command
                {
                    let prov = CliPassthroughProvider::custom(
                        name.clone(),
                        cmd.clone(),
                        c.args.clone().unwrap_or_default(),
                        c.stream_capable.unwrap_or(false),
                        match c.prompt_mode.as_deref() {
                            Some("raw") => super::cli_passthrough::PromptMode::Raw,
                            _ => super::cli_passthrough::PromptMode::Prefixed,
                        },
                        c.strip_ansi.unwrap_or(true),
                        c.timeout_ms,
                        c.cwd.clone(),
                        c.env.clone(),
                        c.session_arg.clone(),
                    );
                    map.insert(name.clone(), Box::new(prov));
                    cli.insert(name.clone());
                }
            }
        }

        Ok(Self {
            providers: map,
            cli_keys: cli,
        })
    }

    pub fn get(&self, key: &str) -> Result<&dyn LlmProvider, ProviderError> {
        self.providers
            .get(key)
            .map(|b| b.as_ref())
            .ok_or_else(|| ProviderError::Config(format!("unknown provider: {key}")))
    }

    pub fn list(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.providers.keys().cloned().collect();
        keys.sort();
        keys
    }

    pub fn is_cli_key(&self, key: &str) -> bool {
        self.cli_keys.contains(key)
    }
}
