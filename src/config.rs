use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub openai: Option<OpenAiConfig>,
    pub ollama: Option<OllamaConfig>,
    pub anthropic: Option<AnthropicConfig>,
    pub grok: Option<GrokConfig>,
    pub deepseek: Option<DeepSeekConfig>,
    pub pricing: Option<PricingConfig>,
    pub caching: Option<CachingConfig>,
    pub mcp: Option<McpConfig>,
    pub claude_cli: Option<CliProviderConfig>,
    pub codex_cli: Option<CliProviderConfig>,
    pub gemini_cli: Option<CliProviderConfig>,
    pub custom_cli_providers: Option<std::collections::HashMap<String, CliProviderConfig>>,
    pub fallback: Option<FallbackConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FallbackConfig {
    pub providers: Option<Vec<String>>, // ordered fallback list
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    pub base_url: Option<String>,
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub default_model: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrokConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PricingConfig {
    /// USD per 1K input tokens by provider/model (fallback to provider-wide)
    pub input_usd_per_1k: std::collections::HashMap<String, f32>,
    /// USD per 1K output tokens by provider/model (fallback to provider-wide)
    pub output_usd_per_1k: std::collections::HashMap<String, f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CachingConfig {
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpConfig {
    pub servers: Option<std::collections::HashMap<String, McpServerConfig>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpServerConfig {
    pub command: String,
    pub args: Option<Vec<String>>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CliProviderConfig {
    pub enabled: Option<bool>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub stream_capable: Option<bool>,
    pub cwd: Option<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub prompt_mode: Option<String>, // raw|prefixed
    pub strip_ansi: Option<bool>,
    pub timeout_ms: Option<u64>,
    pub session_arg: Option<String>,
}

impl Config {
    pub fn load(path: Option<&str>) -> Result<Self> {
        if let Some(p) = path {
            let text = fs::read_to_string(p).with_context(|| format!("reading config at {p}"))?;
            return parse(&text).with_context(|| "parsing config");
        }
        let default = Self::default_path()?;
        if default.exists() {
            let text = fs::read_to_string(&default)
                .with_context(|| format!("reading config at {}", default.display()))?;
            parse(&text).with_context(|| "parsing config")
        } else {
            Ok(Self::default())
        }
    }

    pub fn default_path() -> Result<PathBuf> {
        let base = dirs::config_dir().ok_or_else(|| anyhow!("cannot resolve config dir"))?;
        Ok(base.join("rusty-cli").join("config.toml"))
    }

    pub fn write_example_if_absent() -> Result<PathBuf> {
        let path = Self::default_path()?;
        if !path.exists() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let example = r#"# rusty-cli config (TOML)

[openai]
# api_key can be omitted to use env var OPENAI_API_KEY
api_key = ""
base_url = "https://api.openai.com/v1"
default_model = "gpt-4o-mini"

[ollama]
base_url = "http://localhost:11434"
default_model = "llama3.1"

[anthropic]
# api_key can be omitted to use env var ANTHROPIC_API_KEY
api_key = ""
base_url = "https://api.anthropic.com"
version = "2023-06-01"
default_model = "claude-3-5-sonnet-latest"

[grok]
# api_key can be omitted to use env var XAI_API_KEY or GROK_API_KEY
api_key = ""
base_url = "https://api.x.ai/v1"
default_model = "grok-2-latest"

[deepseek]
# api_key can be omitted to use env var DEEPSEEK_API_KEY
api_key = ""
base_url = "https://api.deepseek.com"
default_model = "deepseek-chat"

[pricing]
# Example keys: "openai" or "openai:gpt-4o-mini". Values are USD per 1K tokens.
input_usd_per_1k = { "openai" = 0.005, "anthropic" = 0.008 }
output_usd_per_1k = { "openai" = 0.015, "anthropic" = 0.024 }

[caching]
enabled = true

[mcp]
# Define MCP servers to load. Tools will be exposed to the CLI when enabled.
# [mcp.servers.my_server]
# command = "my-mcp-server"
# args = ["--flag"]

[claude_cli]
enabled = false
stream_capable = true
# command = "claude"
# args = []
prompt_mode = "prefixed"
strip_ansi = true

[codex_cli]
enabled = false
stream_capable = true
# command = "codex"
prompt_mode = "prefixed"
strip_ansi = true

[gemini_cli]
enabled = false
args = ["--model", "gemini-1.5-pro"]
stream_capable = true
prompt_mode = "prefixed"
strip_ansi = true

# Custom CLI providers
# [custom_cli_providers.cursor]
# enabled = true
# command = "cursor"
# args = ["--chat"]
# stream_capable = false
# prompt_mode = "raw"
# strip_ansi = true
"#;
            fs::write(&path, example)?;
            // Create templates dir and a starter template
            if let Some(parent) = path.parent() {
                let tdir = parent.join("templates");
                let _ = std::fs::create_dir_all(&tdir);
                let starter =
                    "Summarize {{topic}} in 5 bullet points focusing on actionable insights.";
                let _ = fs::write(tdir.join("summarize.tmpl"), starter);
            }
        }
        Ok(path)
    }
}

fn parse(text: &str) -> Result<Config> {
    toml::from_str(text).map_err(|e| anyhow!(e))
}

impl OpenAiConfig {
    pub fn effective_api_key(&self) -> Option<String> {
        self.api_key
            .clone()
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
    }
}

impl OllamaConfig {
    pub fn effective_base_url(&self) -> String {
        self.base_url
            .clone()
            .unwrap_or_else(|| "http://localhost:11434".into())
    }
}

impl AnthropicConfig {
    pub fn effective_api_key(&self) -> Option<String> {
        self.api_key
            .clone()
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
    }
    pub fn effective_version(&self) -> String {
        self.version.clone().unwrap_or_else(|| "2023-06-01".into())
    }
}

impl GrokConfig {
    pub fn effective_api_key(&self) -> Option<String> {
        self.api_key
            .clone()
            .or_else(|| std::env::var("XAI_API_KEY").ok())
            .or_else(|| std::env::var("GROK_API_KEY").ok())
    }
}

impl DeepSeekConfig {
    pub fn effective_api_key(&self) -> Option<String> {
        self.api_key
            .clone()
            .or_else(|| std::env::var("DEEPSEEK_API_KEY").ok())
    }
}
