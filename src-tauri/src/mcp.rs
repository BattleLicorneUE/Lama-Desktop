//! Client MCP minimal : serveurs lancés en local (stdio, JSON-RPC ligne par ligne) ou joints par HTTP (« Streamable HTTP »,
//! comme le serveur intégré à Unreal Engine 5.8).
//! Format de configuration identique à Claude Desktop et Claude Code :
//! `{ "mcpServers": { nom: { command, args, env } | { "type": "http", "url": …, "headers": … } } }`.

use std::{
    collections::HashMap,
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::{oneshot, Mutex},
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Deserialize)]
struct ServerConfig {
    #[serde(default)]
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: HashMap<String, String>,
    /// Serveur déjà lancé, joint par HTTP (par exemple `http://127.0.0.1:8000/mcp`).
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    headers: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct Config {
    #[serde(default, rename = "mcpServers")]
    servers: HashMap<String, ServerConfig>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerStatus {
    pub name: String,
    pub tools: Vec<String>,
    pub error: Option<String>,
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>;

/// Serveur joint par HTTP : un POST par message, la réponse arrive en JSON ou en flux d'événements (SSE).
struct Http {
    client: reqwest::Client,
    url: String,
    headers: Vec<(String, String)>,
    session: Mutex<Option<String>>,
    /// Version du protocole négociée à l'initialisation, renvoyée dans l'en-tête de chaque requête suivante.
    version: Mutex<Option<String>>,
}

impl Http {
    async fn post(&self, body: &Value) -> Result<reqwest::Response, String> {
        let mut rb = self
            .client
            .post(&self.url)
            .header("Accept", "application/json, text/event-stream")
            .json(body);
        for (k, v) in &self.headers {
            rb = rb.header(k.as_str(), v.as_str());
        }
        if let Some(id) = self.session.lock().await.as_ref() {
            rb = rb.header("Mcp-Session-Id", id.as_str());
        }
        if let Some(v) = self.version.lock().await.as_ref() {
            rb = rb.header("MCP-Protocol-Version", v.as_str());
        }
        let resp = rb.send().await.map_err(|e| format!("Serveur injoignable ({}) : {e}. Est-il démarré ?", self.url))?;
        if let Some(id) = resp.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()) {
            *self.session.lock().await = Some(id.to_string());
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("HTTP {status} : {}", text.chars().take(200).collect::<String>()));
        }
        Ok(resp)
    }

    /// Envoie une requête et attend la réponse portant le même identifiant.
    async fn request(&self, id: u64, msg: Value) -> Result<Value, String> {
        let resp = self.post(&msg).await?;
        let is_stream = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).is_some_and(|t| t.contains("text/event-stream"));
        let text = resp.text().await.map_err(|e| e.to_string())?;
        let answer = if is_stream { answer_in_stream(&text, id) } else { answer_in_json(&text, id) };
        answer.ok_or_else(|| "Réponse du serveur MCP illisible ou absente".to_string())
    }
}

fn is_answer(v: &Value, id: u64) -> bool {
    v.get("id").and_then(Value::as_u64) == Some(id) && (v.get("result").is_some() || v.get("error").is_some())
}

fn answer_in_json(text: &str, id: u64) -> Option<Value> {
    match serde_json::from_str::<Value>(text).ok()? {
        Value::Array(list) => list.into_iter().find(|v| is_answer(v, id)),
        v => is_answer(&v, id).then_some(v),
    }
}

/// Lit les événements `data:` d'un flux SSE et renvoie le message qui répond à la requête.
fn answer_in_stream(text: &str, id: u64) -> Option<Value> {
    let mut data = String::new();
    let mut found = None;
    for line in text.lines().chain(std::iter::once("")) {
        if line.is_empty() {
            if !data.is_empty() {
                if let Some(v) = answer_in_json(&data, id) {
                    found = Some(v);
                }
                data.clear();
            }
        } else if let Some(d) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(d.strip_prefix(' ').unwrap_or(d));
        }
    }
    found
}

enum Link {
    Stdio { stdin: Mutex<ChildStdin>, pending: Pending, _child: Child },
    Http(Http),
}

struct Server {
    name: String,
    next_id: AtomicU64,
    tools: Vec<Value>,
    link: Link,
}

impl Server {
    async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let resp = match &self.link {
            Link::Stdio { pending, .. } => {
                let (tx, rx) = oneshot::channel();
                pending.lock().await.insert(id, tx);
                self.send(msg).await?;
                tokio::time::timeout(REQUEST_TIMEOUT, rx)
                    .await
                    .map_err(|_| format!("{method} : délai dépassé"))?
                    .map_err(|_| "Le serveur MCP s'est arrêté".to_string())?
            }
            Link::Http(h) => tokio::time::timeout(REQUEST_TIMEOUT * 2, h.request(id, msg))
                .await
                .map_err(|_| format!("{method} : délai dépassé"))??,
        };
        if let Some(err) = resp.get("error") {
            return Err(err.get("message").and_then(Value::as_str).unwrap_or("erreur MCP").to_string());
        }
        Ok(resp.get("result").cloned().unwrap_or(Value::Null))
    }

    /// Message sans réponse attendue (notification).
    async fn send(&self, msg: Value) -> Result<(), String> {
        match &self.link {
            Link::Stdio { stdin, .. } => {
                let mut line = msg.to_string();
                line.push('\n');
                let mut stdin = stdin.lock().await;
                stdin.write_all(line.as_bytes()).await.map_err(|e| e.to_string())?;
                stdin.flush().await.map_err(|e| e.to_string())
            }
            Link::Http(h) => h.post(&msg).await.map(|_| ()),
        }
    }
}

/// Remplace les `${secret:nom}` par la valeur du coffre Windows du profil.
pub(crate) fn expand_secrets(text: &str, lookup: &dyn Fn(&str) -> Option<String>) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find("${secret:") {
        out.push_str(&rest[..i]);
        let after = &rest[i + "${secret:".len()..];
        let end = after.find('}').ok_or("Renvoi « ${secret:…} » mal fermé")?;
        let name = &after[..end];
        out.push_str(&lookup(name).ok_or_else(|| format!("Secret manquant dans le coffre : {name} (réajoute le connecteur)"))?);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

async fn start_http(name: &str, url: &str, cfg: &ServerConfig, lookup: &(dyn Fn(&str) -> Option<String> + Sync)) -> Result<Server, String> {
    let url = expand_secrets(url, lookup)?;
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(format!("Adresse invalide : {url} (http:// ou https://)"));
    }
    let headers: Vec<(String, String)> = cfg.headers.iter().map(|(k, v)| expand_secrets(v, lookup).map(|v| (k.clone(), v))).collect::<Result<_, _>>()?;
    let client = reqwest::Client::builder().timeout(REQUEST_TIMEOUT * 2).no_proxy().build().map_err(|e| e.to_string())?;
    let mut server = Server {
        name: name.to_string(),
        next_id: AtomicU64::new(1),
        tools: vec![],
        link: Link::Http(Http { client, url, headers, session: Mutex::new(None), version: Mutex::new(None) }),
    };
    let init = server
        .request(
            "initialize",
            json!({ "protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": { "name": "lama-desktop", "version": "0.1.0" } }),
        )
        .await?;
    if let Link::Http(h) = &server.link {
        *h.version.lock().await = init.get("protocolVersion").and_then(Value::as_str).map(String::from);
    }
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await?;
    let list = server.request("tools/list", json!({})).await?;
    server.tools = list.get("tools").and_then(Value::as_array).cloned().unwrap_or_default();
    Ok(server)
}

/// Le programme existe-t-il quelque part dans le PATH ? (`npx`, `uvx`… sont des .cmd ou .exe.)
fn on_path(program: &str) -> bool {
    let p = std::path::Path::new(program);
    if p.components().count() > 1 {
        return p.exists();
    }
    let exts: Vec<String> = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into()).split(';').map(str::to_lowercase).collect();
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).any(|dir| {
        dir.join(program).is_file() || exts.iter().any(|e| dir.join(format!("{program}{e}")).is_file())
    })
}

/// Message clair quand le programme du connecteur n'est pas installé (sans lui, `cmd` échoue en silence).
fn missing_program_help(program: &str) -> String {
    let base = program.trim().trim_matches('"').to_lowercase();
    let tool = match base.as_str() {
        "uvx" | "uv" => Some(("uv", "https://docs.astral.sh/uv/getting-started/installation/")),
        "npx" | "node" | "npm" => Some(("Node.js", "https://nodejs.org/")),
        _ => None,
    };
    match tool {
        Some((name, url)) => format!("« {program} » est introuvable sur ce PC : ce connecteur a besoin de {name} ({url}). Installe-le, puis ferme et rouvre Lama Desktop pour qu'il le trouve."),
        None => format!("« {program} » est introuvable sur ce PC. Vérifie le nom de la commande ou son installation, puis ferme et rouvre Lama Desktop."),
    }
}

async fn start_server(name: &str, cfg: &ServerConfig, lookup: &(dyn Fn(&str) -> Option<String> + Sync)) -> Result<Server, String> {
    if let Some(url) = cfg.url.as_deref().filter(|u| !u.trim().is_empty()) {
        return start_http(name, url, cfg, lookup).await;
    }
    if cfg.command.trim().is_empty() {
        return Err("Serveur sans « command » ni « url ».".into());
    }
    // uv absent du PC : Lama télécharge sa propre copie (vérifiée) au lieu de demander une installation.
    let program = cfg.command.trim().trim_matches('"');
    let mut extra_path: Option<std::path::PathBuf> = None;
    if !on_path(program) {
        if matches!(program.to_lowercase().as_str(), "uvx" | "uv") {
            extra_path = Some(crate::uvrt::ensure().await?);
        } else {
            return Err(missing_program_help(&cfg.command));
        }
    }
    let args: Vec<String> = cfg.args.iter().map(|a| expand_secrets(a, lookup)).collect::<Result<_, _>>()?;
    let env: HashMap<String, String> = cfg.env.iter().map(|(k, v)| expand_secrets(v, lookup).map(|v| (k.clone(), v))).collect::<Result<_, _>>()?;
    // `npx`, `uvx`… sont des .cmd sous Windows : on passe par cmd.exe.
    let mut cmd = Command::new("cmd");
    cmd.arg("/C").arg(&cfg.command).args(&args);
    cmd.envs(&env);
    if let Some(dir) = &extra_path {
        let mut paths = vec![dir.clone()];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
        if let Ok(joined) = std::env::join_paths(paths) {
            cmd.env("PATH", joined);
        }
    }
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .creation_flags(0x0800_0000);
    let mut child = cmd.spawn().map_err(|e| format!("Lancement impossible : {e}"))?;
    let stdin = child.stdin.take().ok_or("stdin indisponible")?;
    let stdout = child.stdout.take().ok_or("stdout indisponible")?;

    let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
    let reader_pending = pending.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
            if let Some(id) = v.get("id").and_then(Value::as_u64) {
                if v.get("result").is_some() || v.get("error").is_some() {
                    if let Some(tx) = reader_pending.lock().await.remove(&id) {
                        let _ = tx.send(v);
                    }
                }
            }
        }
    });

    let mut server = Server {
        name: name.to_string(),
        next_id: AtomicU64::new(1),
        tools: vec![],
        link: Link::Stdio { stdin: Mutex::new(stdin), pending, _child: child },
    };
    server
        .request(
            "initialize",
            json!({ "protocolVersion": "2024-11-05", "capabilities": {},
                    "clientInfo": { "name": "lama-desktop", "version": "0.1.0" } }),
        )
        .await?;
    server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).await?;
    let list = server.request("tools/list", json!({})).await?;
    server.tools = list.get("tools").and_then(Value::as_array).cloned().unwrap_or_default();
    Ok(server)
}

#[derive(Default)]
pub struct McpHub {
    servers: Vec<Server>,
    pub status: Vec<ServerStatus>,
}

/// Les noms d'outils Ollama n'acceptent pas tous les caractères.
fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect()
}

impl McpHub {
    /// `profile` : profil dont le coffre Windows fournit les `${secret:…}` de la configuration.
    pub async fn start(config_path: &Path, profile: &str) -> McpHub {
        let mut hub = McpHub::default();
        let Ok(text) = std::fs::read_to_string(config_path) else { return hub };
        let cfg: Config = match serde_json::from_str(&text) {
            Ok(c) => c,
            Err(e) => {
                hub.status.push(ServerStatus { name: "mcp.json".into(), tools: vec![], error: Some(e.to_string()) });
                return hub;
            }
        };
        for (name, sc) in &cfg.servers {
            let lookup = |n: &str| crate::secrets::get(profile, n).ok().flatten();
            match start_server(name, sc, &lookup).await {
                Ok(s) => {
                    let tools = s.tools.iter().filter_map(|t| t.get("name").and_then(Value::as_str).map(String::from)).collect();
                    hub.status.push(ServerStatus { name: name.clone(), tools, error: None });
                    hub.servers.push(s);
                }
                Err(e) => hub.status.push(ServerStatus { name: name.clone(), tools: vec![], error: Some(e) }),
            }
        }
        hub
    }

    /// Outils MCP au format Ollama, nommés `mcp__<serveur>__<outil>`.
    pub fn definitions(&self) -> Vec<Value> {
        self.servers
            .iter()
            .flat_map(|s| {
                s.tools.iter().filter_map(move |t| {
                    let tool = t.get("name")?.as_str()?;
                    Some(json!({ "type": "function", "function": {
                        "name": format!("mcp__{}__{}", sanitize(&s.name), sanitize(tool)),
                        "description": t.get("description").and_then(Value::as_str).unwrap_or(""),
                        "parameters": t.get("inputSchema").cloned().unwrap_or(json!({"type":"object","properties":{}})),
                    }}))
                })
            })
            .collect()
    }

    /// Retrouve (serveur, outil) à partir du nom préfixé.
    pub fn resolve(&self, full: &str) -> Option<(String, String)> {
        let rest = full.strip_prefix("mcp__")?;
        self.servers.iter().find_map(|s| {
            let prefix = format!("{}__", sanitize(&s.name));
            let tool = rest.strip_prefix(&prefix)?;
            s.tools
                .iter()
                .filter_map(|t| t.get("name").and_then(Value::as_str))
                .find(|n| sanitize(n) == tool)
                .map(|n| (s.name.clone(), n.to_string()))
        })
    }

    pub async fn call(&self, server: &str, tool: &str, args: Value) -> Result<String, String> {
        let s = self.servers.iter().find(|s| s.name == server).ok_or("Serveur MCP introuvable")?;
        let res = s.request("tools/call", json!({ "name": tool, "arguments": args })).await?;
        let text = res
            .get("content")
            .and_then(Value::as_array)
            .map(|items| {
                items.iter().filter_map(|i| i.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n")
            })
            .unwrap_or_default();
        if res.get("isError").and_then(Value::as_bool).unwrap_or(false) {
            return Err(text);
        }
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_missing_program_is_explained_instead_of_failing_silently() {
        assert!(!super::on_path("programme-qui-n-existe-pas-xyz"));
        assert!(super::on_path("cmd"), "cmd est toujours dans le PATH de Windows");
        let uv = super::missing_program_help("uvx");
        assert!(uv.contains("uv") && uv.contains("astral.sh") && uv.contains("rouvre"), "{uv}");
        assert!(super::missing_program_help("npx").contains("Node.js"));
        assert!(super::missing_program_help("autre").contains("introuvable"));
    }

    use super::*;
    use crate::sync::MutexExt;

    #[test]
    fn secret_references_are_expanded_and_missing_ones_reported() {
        let lookup = |n: &str| (n == "mcp:x:token").then(|| "abc123".to_string());
        assert_eq!(expand_secrets("Bearer ${secret:mcp:x:token}!", &lookup).unwrap(), "Bearer abc123!");
        assert_eq!(expand_secrets("sans renvoi", &lookup).unwrap(), "sans renvoi");
        assert!(expand_secrets("${secret:inconnu}", &lookup).unwrap_err().contains("inconnu"));
        assert!(expand_secrets("${secret:mal", &lookup).is_err());
    }

    /// Chaîne complète du connecteur Notion, avec le vrai coffre Windows et le vrai paquet npm (jeton factice, rien n'est modifié
    /// chez Notion) : `cargo test notion_connector -- --ignored`.
    #[tokio::test]
    #[ignore = "lance npx, télécharge le serveur Notion et contacte son API avec un jeton factice"]
    async fn notion_connector_starts_through_the_vault_and_reaches_notion() {
        let conn = crate::connectors::catalog().into_iter().find(|c| c.id == "notion").unwrap();
        let values: std::collections::HashMap<String, String> = [("token".to_string(), "ntn_factice_pour_test".to_string())].into();
        let (entry, secrets) = crate::connectors::build_entry(&conn, &values).unwrap();
        let profile = format!("test-{}", uuid::Uuid::new_v4().simple());
        for (name, value) in &secrets {
            crate::secrets::set(&profile, name, value).unwrap();
        }
        let dir = std::env::temp_dir().join(format!("notion-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mcp.json");
        std::fs::write(&path, json!({ "mcpServers": { "notion": entry } }).to_string()).unwrap();
        assert!(!std::fs::read_to_string(&path).unwrap().contains("ntn_factice"), "le jeton ne doit pas être dans mcp.json");

        let hub = McpHub::start(&path, &profile).await;
        let status = hub.status.iter().find(|s| s.name == "notion").unwrap();
        let outcome = (status.error.clone(), status.tools.len());
        let defs = hub.definitions();
        let search = defs.iter().find_map(|d| d["function"]["name"].as_str().filter(|n| n.ends_with("post_search")).map(String::from));
        let answer = match &search {
            Some(full) => {
                let (server, tool) = hub.resolve(full).unwrap();
                Some(hub.call(&server, &tool, json!({ "query": "test" })).await)
            }
            None => None,
        };
        for (name, _) in &secrets {
            crate::secrets::set(&profile, name, "").unwrap();
        }
        let _ = std::fs::remove_dir_all(dir);

        assert_eq!(outcome.0, None, "le serveur doit démarrer");
        assert!(outcome.1 >= 10, "outils exposés : {}", outcome.1);
        let text = format!("{:?}", answer.expect("outil de recherche exposé"));
        assert!(text.contains("401") || text.contains("unauthorized") || text.contains("invalid"), "Notion doit répondre « jeton invalide » : {text}");
    }

    /// Faux serveur « Streamable HTTP » : session par en-tête, réponse JSON pour l'initialisation, flux SSE pour le reste.
    async fn fake_http_server(seen: Arc<std::sync::Mutex<Vec<(String, Option<String>)>>>) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else { return };
                let seen = seen.clone();
                tokio::spawn(async move {
                    let mut data = Vec::new();
                    let mut chunk = [0u8; 4096];
                    let (head, body) = loop {
                        let n = sock.read(&mut chunk).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        data.extend_from_slice(&chunk[..n]);
                        let text = String::from_utf8_lossy(&data).to_string();
                        if let Some(i) = text.find("\r\n\r\n") {
                            let head = text[..i].to_string();
                            let len = head
                                .lines()
                                .find_map(|l| l.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                                .unwrap_or(0);
                            if text.len() >= i + 4 + len {
                                break (head, text[i + 4..i + 4 + len].to_string());
                            }
                        }
                    };
                    let msg: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
                    let method = msg["method"].as_str().unwrap_or("").to_string();
                    let session = head.lines().find_map(|l| l.to_lowercase().strip_prefix("mcp-session-id:").map(|v| v.trim().to_string()));
                    seen.lock_safe().push((method.clone(), session));
                    let id = msg["id"].clone();
                    let reply = match method.as_str() {
                        "initialize" => {
                            let b = json!({ "jsonrpc": "2.0", "id": id, "result": { "protocolVersion": "2025-03-26", "capabilities": {}, "serverInfo": { "name": "faux", "version": "1" } } }).to_string();
                            format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nMcp-Session-Id: abc123\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}", b.len())
                        }
                        "tools/list" => {
                            let b = format!("event: message\ndata: {}\n\n", json!({ "jsonrpc": "2.0", "id": id, "result": { "tools": [{ "name": "call_tool", "description": "d", "inputSchema": { "type": "object" } }] } }));
                            format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}", b.len())
                        }
                        "tools/call" => {
                            let b = json!({ "jsonrpc": "2.0", "id": id, "result": { "content": [{ "type": "text", "text": "fait" }] } }).to_string();
                            format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}", b.len())
                        }
                        _ => "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
                    };
                    let _ = sock.write_all(reply.as_bytes()).await;
                });
            }
        });
        format!("http://{addr}/mcp")
    }

    #[tokio::test]
    async fn http_server_is_reached_with_session_and_both_response_kinds() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let url = fake_http_server(seen.clone()).await;
        let dir = std::env::temp_dir().join(format!("mcphttp-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mcp.json");
        std::fs::write(&path, json!({ "mcpServers": { "unreal-mcp": { "type": "http", "url": url } } }).to_string()).unwrap();
        let hub = McpHub::start(&path, "test-http").await;
        assert_eq!(hub.status[0].error, None);
        assert_eq!(hub.status[0].tools, vec!["call_tool"]);
        let full = hub.definitions()[0]["function"]["name"].as_str().unwrap().to_string();
        assert_eq!(full, "mcp__unreal_mcp__call_tool");
        let (server, tool) = hub.resolve(&full).unwrap();
        assert_eq!(hub.call(&server, &tool, json!({})).await.unwrap(), "fait");
        let log = seen.lock_safe().clone();
        assert_eq!(log[0], ("initialize".to_string(), None), "pas de session avant l'initialisation");
        assert!(log[1..].iter().all(|(_, s)| s.as_deref() == Some("abc123")), "la session est renvoyée ensuite : {log:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn unreachable_http_server_gives_a_readable_error() {
        let dir = std::env::temp_dir().join(format!("mcphttp-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mcp.json");
        std::fs::write(&path, r#"{"mcpServers":{"u":{"type":"http","url":"http://127.0.0.1:9/mcp"}}}"#).unwrap();
        let hub = McpHub::start(&path, "test-http").await;
        let err = hub.status[0].error.clone().unwrap();
        assert!(err.contains("injoignable") && err.contains("127.0.0.1:9"), "{err}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sse_and_json_answers_are_matched_by_id() {
        let sse = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}\n\nevent: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":7,\"result\":{\"ok\":true}}\n\n";
        assert_eq!(answer_in_stream(sse, 7).unwrap()["result"]["ok"], true);
        assert!(answer_in_stream(sse, 8).is_none());
        assert!(answer_in_json(r#"[{"id":1,"result":{}},{"id":2,"result":{"x":1}}]"#, 2).is_some());
    }

    #[test]
    fn sanitize_replaces_unsupported_chars() {
        assert_eq!(sanitize("my-server.v2"), "my_server_v2");
    }

    #[test]
    fn config_parses_claude_desktop_format() {
        let c: Config = serde_json::from_str(
            r#"{"mcpServers":{"fs":{"command":"npx","args":["-y","pkg"],"env":{"A":"1"}}}}"#,
        )
        .unwrap();
        assert_eq!(c.servers["fs"].args.len(), 2);
    }

    /// Vrai Blender ouvert avec l'extension MCP, et un PC sans uv : `cargo test blender_through_lama -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore]
    async fn blender_through_lama_with_its_own_uv() {
        let dir = std::env::temp_dir().join(format!("mcpblender-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        crate::uvrt::set_data_dir(&dir);
        let path = dir.join("mcp.json");
        let entry = crate::connectors::catalog().into_iter().find(|c| c.id == "blender").unwrap();
        std::fs::write(&path, json!({ "mcpServers": { "blender": { "command": entry.command, "args": entry.args } } }).to_string()).unwrap();
        let hub = McpHub::start(&path, "test-blender").await;
        assert_eq!(hub.status[0].error, None, "{:?}", hub.status);
        println!("{} outils : {:?}", hub.status[0].tools.len(), hub.status[0].tools);
        let (server, tool) = hub.resolve("mcp__blender__get_objects_summary").expect("outil absent");
        let out = hub.call(&server, &tool, json!({})).await.unwrap();
        println!("réponse de Blender ({} caractères) : {}", out.len(), out.chars().take(120).collect::<String>());
        assert!(!out.to_lowercase().contains("connection refused"), "{out}");
        let _ = std::fs::remove_dir_all(dir);
    }
}
