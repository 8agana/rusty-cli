mod cache;
mod cli;
mod config;
mod context;
mod export;
mod mcp;
mod providers;
mod session;
mod templating;
mod tools;

use anyhow::Result;
use cli::{Cli, Commands, HistoryAction, TemplateAction};
use colored::*;
use config::Config;
use futures_util::StreamExt;
use providers::{ChatMessage, ChatRequest, registry::ProviderRegistry};
use std::collections::HashSet;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let cli = Cli::parse();
    let cfg = Config::load(cli.config.as_deref())?;

    let registry = ProviderRegistry::from_config(&cfg)?;

    match cli.command {
        Commands::Chat(cmd) => {
            let provider = registry.get(&cmd.provider)?;
            // Build message list: files as system context, session history, then user prompt
            let mut messages: Vec<ChatMessage> = Vec::new();
            if let Some(sys) = &cmd.system {
                messages.push(ChatMessage::system(sys.clone()));
            }
            for file in &cmd.files {
                match std::fs::read_to_string(file) {
                    Ok(text) => messages.push(ChatMessage::system(format!(
                        "Attached file '{}':\n{}",
                        file, text
                    ))),
                    Err(_) => messages.push(ChatMessage::system(format!(
                        "[Failed to read attachment '{}']",
                        file
                    ))),
                }
            }
            if let Some(session_id) = &cmd.session {
                let hist = session::SessionStore::load(session_id).unwrap_or_default();
                messages.extend(hist);
            }
            // Resolve prompt from template and/or --prompt
            let prompt = if let Some(tpl) = &cmd.template {
                let mut vars = serde_json::Map::new();
                for kv in &cmd.vars {
                    if let Some((k, v)) = kv.split_once('=') {
                        vars.insert(k.to_string(), serde_json::Value::String(v.to_string()));
                    }
                }
                let ctx = serde_json::Value::Object(vars);
                templating::render_template(tpl, &ctx)
                    .unwrap_or_else(|_| cmd.prompt.clone().unwrap_or_default())
            } else {
                cmd.prompt.clone().unwrap_or_default()
            };
            if prompt.trim().is_empty() {
                anyhow::bail!("prompt is required (use --prompt or --template)");
            }
            messages.push(ChatMessage::user(prompt.clone()));

            // Context tracking and trimming
            let max_ctx = cmd.max_context.unwrap_or(16_000);
            let before = context::estimate_messages_tokens(&messages);
            let messages = context::trim_to_budget(messages, max_ctx, cmd.reserve_output);
            let after = context::estimate_messages_tokens(&messages);
            if after < before {
                eprintln!(
                    "[context] trimmed from ~{} to ~{} tokens (budget ~{})",
                    before, after, max_ctx
                );
            }

            let mut tool_registry = tools::ToolRegistry::with_default();
            // Enforce passthrough CLI opt-in
            if registry.is_cli_key(&cmd.provider)
                && !(cmd.enable_passthrough
                    || cmd.allow_passthrough.iter().any(|k| k == &cmd.provider))
            {
                anyhow::bail!(
                    "provider '{}' is a passthrough CLI. Pass --enable-passthrough to proceed.",
                    cmd.provider
                );
            }
            // Load MCP servers if configured and enabled via flags
            if !cmd.no_mcp
                && let Some(mcp_cfg) = cfg.mcp.as_ref().and_then(|m| m.servers.as_ref())
            {
                let only: Option<HashSet<&str>> = if cmd.enable_mcp.is_empty() {
                    None
                } else {
                    Some(cmd.enable_mcp.iter().map(|s| s.as_str()).collect())
                };
                for (name, sc) in mcp_cfg.iter() {
                    if let Some(ref set) = only
                        && !set.contains(name.as_str())
                    {
                        continue;
                    }
                    if let Ok(client) = mcp::client::McpClient::spawn(
                        &sc.command,
                        sc.args.as_ref(),
                        &sc.env,
                        &sc.cwd,
                    )
                    .await
                        && let Ok(tools) = client.list_tools().await
                    {
                        for t in tools {
                            let spec = tools::ToolSpec {
                                name: t.name.clone(),
                                description: t.description.clone(),
                                parameters: t.parameters.clone(),
                                read_only: t.read_only,
                            };
                            tool_registry.register(Box::new(tools::mcp_tool::McpTool::new(
                                client.clone(),
                                spec,
                            )));
                        }
                    }
                }
            }
            let read_only_only = matches!(cmd.mode, cli::Mode::Planning);
            let allowed_specs = tool_registry.list_filtered(
                if cmd.allow_tools.is_empty() {
                    None
                } else {
                    Some(&cmd.allow_tools)
                },
                read_only_only,
            );

            let request = providers::ChatRequest {
                model: cmd
                    .model
                    .unwrap_or_else(|| provider.default_model().to_string()),
                system: None,
                messages,
                stream: cmd.stream,
                temperature: cmd.temperature,
                max_tokens: cmd.max_tokens,
                tools: if cmd.enable_tools {
                    Some(
                        allowed_specs
                            .iter()
                            .map(|t| providers::ToolSpec {
                                name: t.name.clone(),
                                description: t.description.clone(),
                                parameters: t.parameters.clone(),
                            })
                            .collect(),
                    )
                } else {
                    None
                },
                session_id: cmd.session.clone(),
            };

            // Simple cache for non-tool, non-stream requests
            let cache_enabled =
                cfg.caching.as_ref().and_then(|c| c.enabled).unwrap_or(true) && !cmd.no_cache;
            if cache_enabled && !cmd.enable_tools && !request.stream {
                let mut hasher = blake3::Hasher::new();
                hasher.update(cmd.provider.as_bytes());
                hasher.update(request.model.as_bytes());
                if let Some(sys) = &request.system {
                    hasher.update(sys.as_bytes());
                }
                for m in &request.messages {
                    hasher.update(m.role.as_bytes());
                    hasher.update(m.content.as_bytes());
                }
                if let Some(t) = request.temperature {
                    hasher.update(&t.to_le_bytes());
                }
                if let Some(mt) = request.max_tokens {
                    hasher.update(&mt.to_le_bytes());
                }
                let key = hasher.finalize().to_hex().to_string();
                if let Ok(Some(cached)) = cache::CacheStore::get::<providers::ChatResponse>(&key) {
                    eprintln!("[cache] hit");
                    if let Some(content) = cached.content {
                        println!("{}", content);
                    }
                    return Ok(());
                }
                eprintln!("[cache] miss");
            }

            if cmd.enable_tools
                && matches!(
                    cmd.provider.as_str(),
                    "openai" | "grok" | "deepseek" | "anthropic"
                )
            {
                // Non-stream tool loop
                let mut history = request.messages.clone();
                let mut guard = 0;
                loop {
                    let mut req = ChatRequest {
                        messages: history.clone(),
                        ..request.clone()
                    };
                    req.stream = false;
                    let resp = provider.chat(req).await?;
                    if let Some(tool_calls) = resp.tool_calls {
                        for call in tool_calls {
                            if let Some(tool) = tool_registry.get(&call.name) {
                                // Enforce planning vs building
                                if read_only_only && !tool.spec().read_only {
                                    // Return a policy error to the model as a tool message
                                    let result = serde_json::json!({"error": format!("tool '{}' is disabled in planning mode", call.name)});
                                    history.push(ChatMessage {
                                        role: "tool".into(),
                                        content: result.to_string(),
                                        name: Some(call.name),
                                        tool_call_id: call.id,
                                    });
                                    continue;
                                }
                                let result = tool.call(&call.arguments).unwrap_or_else(
                                    |e| serde_json::json!({"error": e.to_string()}),
                                );
                                // Append tool result message
                                history.push(ChatMessage {
                                    role: "tool".into(),
                                    content: result.to_string(),
                                    name: Some(call.name),
                                    tool_call_id: call.id,
                                });
                            }
                        }
                    }
                    if let Some(content) = resp.content {
                        println!("{}", content);
                        if let Some(session_id) = &cmd.session {
                            let mut persisted =
                                session::SessionStore::load(session_id).unwrap_or_default();
                            persisted.push(ChatMessage::user(prompt.clone()));
                            persisted.push(ChatMessage {
                                role: "assistant".into(),
                                content: content.clone(),
                                name: None,
                                tool_call_id: None,
                            });
                            let _ = session::SessionStore::save(session_id, &persisted);
                        }
                        if let Some(path) = cmd.export.as_deref() {
                            let _ = export::save(path, &history, &content);
                        }
                        break;
                    }
                    guard += 1;
                    if guard > 8 {
                        break;
                    }
                }
            } else if cmd.stream {
                let mut stream = provider.chat_stream(request.clone()).await?;
                let mut acc = String::new();
                let mut tool_trigger = false;
                while let Some(chunk) = stream.next().await.transpose()? {
                    if let Some(content) = chunk.delta {
                        print!("{}", content);
                        acc.push_str(&content);
                    }
                    if chunk.tool_calls.is_some() && cmd.enable_tools && cmd.provider == "openai" {
                        tool_trigger = true;
                        break;
                    }
                }
                println!();
                if tool_trigger {
                    // Switch to non-stream tool loop using accumulated history
                    let mut history = request.messages.clone();
                    // append partial assistant text if any
                    if !acc.is_empty() {
                        history.push(ChatMessage {
                            role: "assistant".into(),
                            content: acc.clone(),
                            name: None,
                            tool_call_id: None,
                        });
                    }
                    let mut guard = 0;
                    loop {
                        let mut req = ChatRequest {
                            messages: history.clone(),
                            ..request.clone()
                        };
                        req.stream = false;
                        let resp = provider.chat(req).await?;
                        if let Some(tool_calls) = resp.tool_calls {
                            for call in tool_calls {
                                if let Some(tool) = tool_registry.get(&call.name) {
                                    if read_only_only && !tool.spec().read_only {
                                        let result = serde_json::json!({"error": format!("tool '{}' is disabled in planning mode", call.name)});
                                        history.push(ChatMessage {
                                            role: "tool".into(),
                                            content: result.to_string(),
                                            name: Some(call.name),
                                            tool_call_id: call.id,
                                        });
                                        continue;
                                    }
                                    let result = tool.call(&call.arguments).unwrap_or_else(
                                        |e| serde_json::json!({"error": e.to_string()}),
                                    );
                                    history.push(ChatMessage {
                                        role: "tool".into(),
                                        content: result.to_string(),
                                        name: Some(call.name),
                                        tool_call_id: call.id,
                                    });
                                }
                            }
                        }
                        if let Some(content) = resp.content {
                            println!("{}", content);
                            if let Some(session_id) = &cmd.session {
                                let mut persisted =
                                    session::SessionStore::load(session_id).unwrap_or_default();
                                persisted.push(ChatMessage::user(prompt.clone()));
                                persisted.push(ChatMessage {
                                    role: "assistant".into(),
                                    content: content.clone(),
                                    name: None,
                                    tool_call_id: None,
                                });
                                let _ = session::SessionStore::save(session_id, &persisted);
                            }
                            if let Some(path) = cmd.export.as_deref() {
                                let _ = export::save(path, &history, &content);
                            }
                            break;
                        }
                        guard += 1;
                        if guard > 8 {
                            break;
                        }
                    }
                } else if let Some(session_id) = &cmd.session {
                    // Save history: prior (excluding last user) is already included. Append assistant reply.
                    let mut history = session::SessionStore::load(session_id).unwrap_or_default();
                    // Ensure we also add the user prompt if it wasn't part of history yet
                    // We appended all of messages including user, so for persistence, append the last two
                    history.push(ChatMessage::user(prompt.clone()));
                    history.push(ChatMessage {
                        role: "assistant".into(),
                        content: acc.clone(),
                        name: None,
                        tool_call_id: None,
                    });
                    let _ = session::SessionStore::save(session_id, &history);
                }
                if let Some(path) = cmd.export.as_deref() {
                    let _ = export::save(path, &request.messages, &acc);
                }
            } else {
                // Non-stream with fallback
                let mut resp = provider.chat(request.clone()).await;
                if resp.is_err()
                    && let Some(fb) = &cfg.fallback.and_then(|f| f.providers.clone())
                {
                    eprintln!(
                        "[fallback] primary '{}' failed, trying chain: {}",
                        cmd.provider,
                        fb.join(", ")
                    );
                    for alt in fb {
                        if alt == &cmd.provider {
                            continue;
                        }
                        if let Ok(p) = registry.get(alt) {
                            resp = p.chat(request.clone()).await;
                            if resp.is_ok() {
                                eprintln!("[fallback] succeeded with '{}'", alt);
                                break;
                            }
                        }
                    }
                }
                let resp = resp?;
                let content = resp.content.clone().unwrap_or_default();
                if !content.is_empty() {
                    println!("{}", content);
                }
                // Estimate cost if usage and pricing present
                if let Some(ref usage) = resp.usage {
                    if let Some(pr) = &cfg.pricing {
                        let model_key = format!("{}:{}", cmd.provider, request.model);
                        let in_rate = pr
                            .input_usd_per_1k
                            .get(&model_key)
                            .copied()
                            .or_else(|| pr.input_usd_per_1k.get(&cmd.provider).copied())
                            .unwrap_or(0.0);
                        let out_rate = pr
                            .output_usd_per_1k
                            .get(&model_key)
                            .copied()
                            .or_else(|| pr.output_usd_per_1k.get(&cmd.provider).copied())
                            .unwrap_or(0.0);
                        let cost = (usage.input_tokens as f32 / 1000.0) * in_rate
                            + (usage.output_tokens as f32 / 1000.0) * out_rate;
                        eprintln!(
                            "[usage] in={} out={} total={} est_cost=${:.4}",
                            usage.input_tokens, usage.output_tokens, usage.total_tokens, cost
                        );
                    } else {
                        eprintln!(
                            "[usage] in={} out={} total={}",
                            usage.input_tokens, usage.output_tokens, usage.total_tokens
                        );
                    }
                }
                if let Some(session_id) = &cmd.session {
                    let mut history = session::SessionStore::load(session_id).unwrap_or_default();
                    history.push(ChatMessage::user(prompt.clone()));
                    history.push(ChatMessage {
                        role: "assistant".into(),
                        content: content.clone(),
                        name: None,
                        tool_call_id: None,
                    });
                    let _ = session::SessionStore::save(session_id, &history);
                }
                // Cache store when applicable
                if cache_enabled && !cmd.enable_tools && !cmd.stream {
                    // Same key logic as above
                    let mut hasher = blake3::Hasher::new();
                    hasher.update(cmd.provider.as_bytes());
                    hasher.update(request.model.as_bytes());
                    if let Some(sys) = &request.system {
                        hasher.update(sys.as_bytes());
                    }
                    for m in &request.messages {
                        hasher.update(m.role.as_bytes());
                        hasher.update(m.content.as_bytes());
                    }
                    if let Some(t) = request.temperature {
                        hasher.update(&t.to_le_bytes());
                    }
                    if let Some(mt) = request.max_tokens {
                        hasher.update(&mt.to_le_bytes());
                    }
                    let key = hasher.finalize().to_hex().to_string();
                    let _ = cache::CacheStore::put(&key, resp.clone());
                    eprintln!("[cache] store");
                }
                if let Some(path) = cmd.export.as_deref() {
                    let _ = export::save(path, &request.messages, &content);
                }
            }
        }
        Commands::History(h) => {
            match h.action {
                HistoryAction::List => {
                    let sessions = session::SessionStore::list().unwrap_or_default();
                    for s in sessions {
                        println!("{}", s);
                    }
                }
                HistoryAction::Show => {
                    let id = h.session.as_deref().unwrap_or("");
                    if id.is_empty() {
                        eprintln!("--session is required for show");
                    } else {
                        let msgs = session::SessionStore::load(id).unwrap_or_default();
                        for m in msgs {
                            println!("{}: {}", m.role, m.content);
                        }
                    }
                }
                HistoryAction::Clear => {
                    let id = h.session.as_deref().unwrap_or("");
                    if id.is_empty() {
                        eprintln!("--session is required for clear");
                    } else {
                        let _ = session::SessionStore::delete(id);
                        println!("cleared {}", id);
                    }
                }
                HistoryAction::ClearAll => {
                    let _ = session::SessionStore::clear_all();
                    println!("cleared all sessions");
                }
                HistoryAction::Export => {
                    let id = h.session.as_deref().unwrap_or("");
                    let out = h.out.as_deref().unwrap_or("");
                    if id.is_empty() || out.is_empty() {
                        eprintln!("--session and --out are required for export");
                    } else {
                        let msgs = session::SessionStore::load(id).unwrap_or_default();
                        // Last assistant content if present
                        let last = msgs
                            .iter()
                            .rev()
                            .find(|m| m.role == "assistant")
                            .map(|m| m.content.clone())
                            .unwrap_or_default();
                        if let Err(e) = export::save(out, &msgs, &last) {
                            eprintln!("export error: {}", e);
                        } else {
                            println!("exported {} to {}", id, out);
                        }
                    }
                }
            }
        }
        Commands::Templates(t) => {
            let base =
                dirs::config_dir().ok_or_else(|| anyhow::anyhow!("cannot resolve config dir"))?;
            let dir = base.join("rusty-cli").join("templates");
            match t.action {
                TemplateAction::List => {
                    if dir.exists() {
                        for entry in std::fs::read_dir(dir)? {
                            let e = entry?;
                            let p = e.path();
                            if p.extension().and_then(|s| s.to_str()) == Some("tmpl")
                                && let Some(stem) = p.file_stem().and_then(|s| s.to_str())
                            {
                                println!("{}", stem);
                            }
                        }
                    }
                }
                TemplateAction::Show => {
                    if let Some(name) = t.name.as_deref() {
                        let path = dir.join(format!("{}.tmpl", name));
                        match std::fs::read_to_string(&path) {
                            Ok(text) => println!("{}", text),
                            Err(e) => eprintln!("template error: {}", e),
                        }
                    } else {
                        eprintln!("--name is required for templates show");
                    }
                }
            }
        }
        Commands::ListModels(cmd) => {
            let provider = registry.get(&cmd.provider)?;
            let models = provider.list_models().await?;
            for m in models {
                println!("{}", m);
            }
        }
        Commands::Providers => {
            println!("{}", "Available providers:".bold());
            for key in registry.list() {
                match registry.get(&key) {
                    Ok(p) => println!("- {} ({})", key, p.name()),
                    Err(_) => println!("- {}", key),
                }
            }
        }
        Commands::ConfigPath => {
            println!("{}", Config::default_path()?.display());
        }
        Commands::InitConfig => {
            let path = Config::write_example_if_absent()?;
            println!("Wrote example config to {}", path.display());
        }
    }

    Ok(())
}
