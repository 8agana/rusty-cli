use crate::providers::ChatMessage;
use anyhow::Result;
use std::fs;

pub fn save(path: &str, messages: &[ChatMessage], assistant: &str) -> Result<()> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    match ext {
        "json" => save_json(path, messages, assistant),
        "html" => save_html(path, messages, assistant),
        _ => save_md(path, messages, assistant),
    }
}

fn save_json(path: &str, messages: &[ChatMessage], assistant: &str) -> Result<()> {
    let mut all = messages.to_vec();
    all.push(ChatMessage {
        role: "assistant".into(),
        content: assistant.to_string(),
        name: None,
        tool_call_id: None,
    });
    let text = serde_json::to_string_pretty(&all)?;
    fs::write(path, text)?;
    Ok(())
}

fn save_md(path: &str, messages: &[ChatMessage], assistant: &str) -> Result<()> {
    let mut out = String::new();
    for m in messages {
        out.push_str(&format!("### {}\n\n{}\n\n", m.role, m.content));
    }
    out.push_str(&format!("### assistant\n\n{}\n", assistant));
    fs::write(path, out)?;
    Ok(())
}

fn save_html(path: &str, messages: &[ChatMessage], assistant: &str) -> Result<()> {
    let mut out = String::from(
        "<html><head><meta charset=\"utf-8\"><title>rusty-cli export</title></head><body>\n",
    );
    for m in messages {
        out.push_str(&format!(
            "<h3>{}</h3>\n<pre>{}</pre>\n",
            html_escape::encode_text(&m.role),
            html_escape::encode_text(&m.content)
        ));
    }
    out.push_str(&format!(
        "<h3>assistant</h3>\n<pre>{}</pre>\n",
        html_escape::encode_text(assistant)
    ));
    out.push_str("</body></html>\n");
    fs::write(path, out)?;
    Ok(())
}
