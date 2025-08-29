# Build + Architecture Notes

This document captures build steps and how tool calling works under the hood. It also outlines a plan to add MCP (Model Context Protocol) capabilities.

## Build

- Requirements: Rust 1.85+ (2024 edition)
- Build:
  - `cargo build --release`
- Run examples:
  - `./target/release/rusty-cli providers`
  - `./target/release/rusty-cli chat -p openai --prompt "Hello"`

## Config

- Env vars or `~/.config/rusty-cli/config.toml`.
- Generate a starter config: `rusty-cli init-config`.

## Tool Calling Modes

- `planning`: read-only tools only
- `building`: all tools allowed
- CLI:
  - `--enable-tools` turns on tool calling
  - `--mode planning|building` (default: planning)
  - `--allow-tool <name>` to restrict tools if needed

## Non-Stream Tool Calling Explained

- Definition: The CLI requests model outputs with streaming disabled. The provider returns either a final assistant message or one/more complete tool calls with fully formed JSON arguments.
- Loop (per turn):
  1) Send messages + tool specs with `stream=false`.
  2) If the response includes tool calls, execute allowed tools, append a `role=tool` message containing results, and repeat.
  3) If the response includes final `content`, print it and end the loop.
- Why:
  - Simpler and more reliable across providers. No partial JSON assembly for arguments.
  - Avoids provider-specific streaming state machines (OpenAI deltas, Anthropic event stream, etc.).
- Trade-offs:
  - No token-by-token rendering during tool execution.
  - Output appears after one or more tool cycles finish.
- Future streaming option:
  - Implement provider-specific parsers to accumulate function/tool args mid-stream; run tools when arguments are complete; keep streaming text. This is feasible but adds complexity.

## Supported Providers for Tools

- OpenAI-compatible: `openai`, `grok`, `deepseek` (Chat Completions tools)
- Anthropic: `anthropic` (Messages tools via `tool_use` / `tool_result` blocks)

## Adding Tools

- Tools live under `src/tools/` and implement `Tool`:
  - Provide a JSON Schema `parameters`, `name`, `description`, and `read_only` flag.
  - The CLI enforces `planning` (read-only) vs `building` (all tools) at runtime.
- Included tools:
  - `read_file` (read-only, size-limited)
  - `echo` (read-only)

## Session History

- Stored as JSON at `~/.local/share/rusty-cli/sessions/<session>.json`.
- A future SQLite store can add search and management commands.

## MCP (Model Context Protocol) — Proposed Integration

Goal: Allow the CLI to load external MCP tool servers and expose their tools to the model, similar to local tools.

Approach:
- Add an MCP client runtime that speaks JSON-RPC over stdio to MCP servers.
- Map MCP tools to the internal `Tool` trait:
  - Convert MCP tool descriptions and JSON Schemas into `ToolSpec`.
  - On call, forward to the MCP server and return the JSON result.
- Config:
  - `[mcp.servers.<name>]` with `command`, `args`, `env`, `cwd`.
  - Optional per-provider or global enablement.
- Modes:
  - Respect `planning` vs `building` with a `read_only` flag from config or MCP tool metadata.
- Steps:
  1) `mcp::client` module (process spawn, handshake, heartbeats)
  2) Tool discovery → register in `ToolRegistry`
  3) Call bridge with timeouts and error mapping
  4) CLI flags to enable/disable MCP sets
  5) Docs + examples

This keeps local and MCP tools unified under the same interface and policy controls.
