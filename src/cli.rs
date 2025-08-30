use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(name = "rusty-cli", author, version, about = "Rusty CLI for multi-LLM chat", long_about = None)]
pub struct Cli {
    /// Optional path to a config file (toml/json)
    #[arg(short, long)]
    pub config: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[allow(clippy::large_enum_variant)]
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Chat with a model
    Chat(ChatArgs),
    /// List models for a provider
    ListModels(ListModelsArgs),
    /// Show available providers
    Providers,
    /// Print the default config path
    ConfigPath,
    /// Create an example config file if missing
    InitConfig,
    /// Manage session history
    History(HistoryArgs),
    /// Manage templates
    Templates(TemplatesArgs),
}

#[derive(Args, Debug)]
pub struct ChatArgs {
    /// Provider key, e.g. openai, ollama
    #[arg(short, long, default_value = "openai")]
    pub provider: String,

    /// Model name; if not given, provider default is used
    #[arg(short, long)]
    pub model: Option<String>,

    /// Prompt text (user message). Optional if --template is used
    #[arg(short, long)]
    pub prompt: Option<String>,

    /// Optional system message
    #[arg(long)]
    pub system: Option<String>,

    /// Stream tokens as they arrive
    #[arg(long)]
    pub stream: bool,

    /// Temperature (0.0 - 2.0)
    #[arg(long)]
    pub temperature: Option<f32>,

    /// Max output tokens
    #[arg(long)]
    pub max_tokens: Option<u32>,

    /// Optional session id to persist and load history
    #[arg(long)]
    pub session: Option<String>,

    /// Attach one or more files (text) as context
    #[arg(long = "file", num_args = 1.., value_delimiter = ' ')]
    pub files: Vec<String>,

    /// Enable experimental function/tool calling (OpenAI-compatible providers)
    #[arg(long)]
    pub enable_tools: bool,

    /// Limit allowed tools by name (default: all built-in)
    #[arg(long = "allow-tool", num_args = 1.., value_delimiter = ' ')]
    pub allow_tools: Vec<String>,

    /// Tool mode: planning (read-only) or building (all tools)
    #[arg(long, value_parser = clap::value_parser!(Mode), default_value_t = Mode::Planning)]
    pub mode: Mode,

    /// Max context tokens (rough estimate)
    #[arg(long, value_name = "TOKENS")]
    pub max_context: Option<u32>,

    /// Reserve this many tokens for the model's output
    #[arg(long, default_value_t = 1024)]
    pub reserve_output: u32,

    /// Disable reading/writing the response cache
    #[arg(long)]
    pub no_cache: bool,

    /// Export the conversation to this file (md|json|html by extension)
    #[arg(long)]
    pub export: Option<String>,

    /// Explicitly allow passthrough CLI providers for this run
    #[arg(long)]
    pub enable_passthrough: bool,

    /// Enable specific MCP servers by name (omit to use all configured)
    #[arg(long = "enable-mcp", num_args = 1.., value_delimiter = ' ')]
    pub enable_mcp: Vec<String>,

    /// Disable loading MCP servers
    #[arg(long)]
    pub no_mcp: bool,

    /// Render prompt from template name (in ~/.config/rusty-cli/templates/<name>.tmpl)
    #[arg(long)]
    pub template: Option<String>,

    /// Key=val variables for template rendering
    #[arg(long = "var", num_args = 1.., value_delimiter = ' ')]
    pub vars: Vec<String>,

    /// Allow specific passthrough providers by name for this run
    #[arg(long = "allow-passthrough", num_args = 1.., value_delimiter = ' ')]
    pub allow_passthrough: Vec<String>,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum HistoryAction {
    List,
    Show,
    Clear,
    ClearAll,
    Export,
}

#[derive(Args, Debug)]
pub struct HistoryArgs {
    /// Action to perform: list | show | clear | clear-all | export
    #[arg(value_enum)]
    pub action: HistoryAction,

    /// Session id (for show/clear)
    #[arg(long)]
    pub session: Option<String>,

    /// Output path for export
    #[arg(long)]
    pub out: Option<String>,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum TemplateAction {
    List,
    Show,
}

#[derive(Args, Debug)]
pub struct TemplatesArgs {
    /// Action to perform: list | show
    #[arg(value_enum)]
    pub action: TemplateAction,

    /// Template name (for show)
    #[arg(long)]
    pub name: Option<String>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Mode {
    Planning,
    Building,
}

impl std::str::FromStr for Mode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "planning" => Ok(Mode::Planning),
            "building" => Ok(Mode::Building),
            other => Err(format!("invalid mode: {} (use planning|building)", other)),
        }
    }
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Planning => write!(f, "planning"),
            Mode::Building => write!(f, "building"),
        }
    }
}

#[derive(Args, Debug)]
pub struct ListModelsArgs {
    /// Provider key, e.g. openai, ollama
    #[arg(short, long, default_value = "openai")]
    pub provider: String,
}

impl Cli {
    pub fn parse() -> Self {
        <Self as Parser>::parse()
    }
}
