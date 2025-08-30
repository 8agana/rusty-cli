use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, oneshot};

#[derive(Clone)]
pub struct McpClient {
    inner: Arc<McpInner>,
}

struct McpInner {
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    next_id: Mutex<u64>,
    pending: Mutex<HashMap<u64, oneshot::Sender<RpcResp>>>,
}

#[derive(Serialize)]
struct RpcReq<'a> {
    jsonrpc: &'static str,
    id: u64,
    method: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

#[derive(Deserialize)]
struct RpcResp {
    #[serde(rename = "jsonrpc")]
    _jsonrpc: String,
    id: u64,
    #[serde(default)]
    result: Value,
    #[serde(default)]
    error: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    #[serde(default)]
    pub read_only: bool,
}

impl McpClient {
    pub async fn spawn(
        command: &str,
        args: Option<&Vec<String>>,
        env: &Option<HashMap<String, String>>,
        cwd: &Option<String>,
    ) -> Result<Self> {
        let mut cmd = Command::new(command);
        if let Some(a) = args {
            cmd.args(a);
        }
        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }
        if let Some(envmap) = env {
            for (k, v) in envmap {
                cmd.env(k, v);
            }
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        let mut child = cmd
            .spawn()
            .with_context(|| format!("spawning MCP server: {}", command))?;
        let stdin = child.stdin.take().context("capturing MCP stdin")?;
        let stdout = child.stdout.take().context("capturing MCP stdout")?;
        let inner = Arc::new(McpInner {
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            next_id: Mutex::new(1),
            pending: Mutex::new(HashMap::new()),
        });
        // Spawn a persistent reader task to dispatch JSON-RPC responses by id
        {
            let inner_clone = inner.clone();
            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout);
                let mut line = String::new();
                loop {
                    line.clear();
                    let read = reader.read_line(&mut line).await;
                    let n = match read {
                        Ok(n) => n,
                        Err(_) => break,
                    };
                    if n == 0 {
                        break;
                    }
                    let trimmed = line.trim_end();
                    if trimmed.is_empty() {
                        continue;
                    }
                    // Content-Length framing support (JSON-RPC over stdio)
                    if trimmed.to_ascii_lowercase().starts_with("content-length:") {
                        // parse length
                        let len_str = trimmed.split_once(':').map(|x| x.1.trim()).unwrap_or("");
                        let len: usize = len_str.parse().unwrap_or(0);
                        // consume remaining header lines until blank
                        loop {
                            line.clear();
                            let n2: usize = reader.read_line(&mut line).await.unwrap_or_default();
                            if n2 == 0 {
                                break;
                            }
                            if line.trim().is_empty() {
                                break;
                            }
                        }
                        if len > 0 {
                            let mut body = vec![0u8; len];
                            if reader.read_exact(&mut body).await.is_err() {
                                break;
                            }
                            if let Ok(resp) = serde_json::from_slice::<RpcResp>(&body) {
                                let id = resp.id;
                                if let Some(tx) = inner_clone.pending.lock().await.remove(&id) {
                                    let _ = tx.send(resp);
                                }
                            }
                        }
                        continue;
                    }
                    // Fallback: newline-delimited JSON
                    if let Ok(resp) = serde_json::from_str::<RpcResp>(trimmed) {
                        let id = resp.id;
                        if let Some(tx) = inner_clone.pending.lock().await.remove(&id) {
                            let _ = tx.send(resp);
                        }
                    }
                }
            });
        }
        Ok(McpClient { inner })
    }

    pub async fn list_tools(&self) -> Result<Vec<McpTool>> {
        let res = self.call("tools/list", None).await?;
        let tools: Vec<McpTool> =
            serde_json::from_value(res).context("parsing MCP tools/list result")?;
        Ok(tools)
    }

    pub async fn call_tool(&self, name: &str, args: &Value) -> Result<Value> {
        let params = serde_json::json!({ "name": name, "arguments": args });
        let res = self.call("tools/call", Some(params)).await?;
        Ok(res)
    }

    async fn call(&self, method: &str, params: Option<Value>) -> Result<Value> {
        let mut stdin = self.inner.stdin.lock().await;
        let mut id_guard = self.inner.next_id.lock().await;
        let id = *id_guard;
        *id_guard += 1;
        let (tx, rx) = oneshot::channel();
        self.inner.pending.lock().await.insert(id, tx);
        let msg = RpcReq {
            jsonrpc: "2.0",
            id,
            method,
            params,
        };
        let line = serde_json::to_string(&msg)? + "\n";
        stdin.write_all(line.as_bytes()).await?;
        stdin.flush().await?;

        let resp = rx.await.context("mcp: awaiting response")?;
        if let Some(err) = resp.error {
            anyhow::bail!("mcp error: {}", err);
        }
        Ok(resp.result)
    }
}

#[allow(dead_code)]
impl McpClient {
    /// Attempts to gracefully shut down the MCP server process.
    /// Currently closes stdin and sends a kill signal if still running.
    pub async fn shutdown(&self) -> Result<()> {
        // Close stdin to signal EOF
        {
            let mut stdin = self.inner.stdin.lock().await;
            let _ = stdin.shutdown().await; // ignore errors
        }
        // Try to terminate the child
        let mut child = self.inner.child.lock().await;
        let _ = child.start_kill();
        Ok(())
    }
}
