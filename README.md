# rusty-cli

A Rust-based CLI to chat with LLMs from multiple providers (OpenAI, Ollama, Anthropic, Grok/xAI, DeepSeek).

## Install

- Requires Rust 1.85+ (Rust 2024 edition).
- Build:

```
cargo build --release
```

## Configure

- Env vars:
  - `OPENAI_API_KEY` for OpenAI.
- Or config file at `~/.config/rusty-cli/config.toml`:

```toml
[openai]
api_key = "sk-..." # or leave blank to use env var
base_url = "https://api.openai.com/v1"
default_model = "gpt-4o-mini"

[ollama]
base_url = "http://localhost:11434"
default_model = "llama3.1"

[anthropic]
api_key = "..." # or env ANTHROPIC_API_KEY
base_url = "https://api.anthropic.com"
version = "2023-06-01"
default_model = "claude-3-5-sonnet-latest"

[grok]
api_key = "..." # or env XAI_API_KEY or GROK_API_KEY
base_url = "https://api.x.ai/v1"
default_model = "grok-2-latest"

[deepseek]
api_key = "..." # or env DEEPSEEK_API_KEY
base_url = "https://api.deepseek.com"
default_model = "deepseek-chat"
```

Generate an example file:

```
rusty-cli init-config
```

## Usage

- Show providers:

```
rusty-cli providers
```

- List models for a provider:

```
rusty-cli list-models --provider openai
rusty-cli list-models --provider ollama
```

- Chat (non-streaming):

```
rusty-cli chat -p openai -m gpt-4o-mini --prompt "Write a haiku about Rust."
```

- Chat (streaming):

```
rusty-cli chat -p ollama -m llama3.1 --prompt "Summarize Tokio" --stream
```

- Chat with session history and file attachments:

```
rusty-cli chat -p anthropic -m claude-3-5-sonnet-latest \
  --session my-notes \
  --file README.md --file ./docs/plan.txt \
  --prompt "Continue the previous discussion and incorporate the attached notes."
```

## Notes

- OpenAI/Grok/DeepSeek use OpenAI-compatible Chat Completions; Anthropic uses Messages API; Ollama uses local NDJSON.
- Providers are loaded from config/env; unknown providers will error.
- Session history is saved under `~/.local/share/rusty-cli/sessions/<session>.json`.
- File attachments are inlined as system context; keep file sizes reasonable.
- This is an MVP; feel free to request additional providers or features.
