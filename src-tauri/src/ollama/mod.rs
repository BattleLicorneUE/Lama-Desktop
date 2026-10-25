use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// API directe d'Ollama Cloud (authentifiée par une clé API).
pub const CLOUD_HOST: &str = "https://ollama.com";

#[derive(Debug, thiserror::Error)]
pub enum OllamaError {
    #[error("Ollama injoignable : {0}")]
    Http(#[from] reqwest::Error),
    #[error("Ollama a répondu {0} : {1}")]
    Status(u16, String),
    #[error("Réponse invalide : {0}")]
    Parse(#[from] serde_json::Error),
}

impl Serialize for OllamaError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub modified_at: String,
    /// Renseigné pour les modèles cloud (exécutés sur les serveurs d'Ollama, pas sur le PC).
    #[serde(default)]
    pub remote_host: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Images en base64 pour les modèles vision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub images: Option<Vec<String>>,
}

/// Un morceau de la réponse en streaming, déjà décodé.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ChatChunk {
    pub content: String,
    pub thinking: String,
    pub tool_calls: Vec<Value>,
    pub done: bool,
    /// Tokens du prompt (contexte utilisé), fournis dans le dernier morceau.
    pub prompt_tokens: u64,
    /// Tokens générés (`eval_count`), fournis dans le dernier morceau.
    pub eval_tokens: u64,
}

#[derive(Clone)]
pub struct OllamaClient {
    http: reqwest::Client,
    host: String,
    /// Attentes successives avant de réessayer quand l'API répond « trop de requêtes » (429) ou « indisponible » (503).
    backoff: Vec<std::time::Duration>,
}

fn default_backoff() -> Vec<std::time::Duration> {
    [2, 5, 12].into_iter().map(std::time::Duration::from_secs).collect()
}

/// Attente demandée par le serveur (`Retry-After`, en secondes), plafonnée à une minute.
fn retry_after(resp: &reqwest::Response) -> Option<std::time::Duration> {
    let secs: u64 = resp.headers().get(reqwest::header::RETRY_AFTER)?.to_str().ok()?.trim().parse().ok()?;
    Some(std::time::Duration::from_secs(secs.min(60)))
}

impl OllamaClient {
    /// Client de l'API cloud : la clé est envoyée uniquement à `CLOUD_HOST`, en en-tête `Authorization`.
    pub fn cloud(api_key: &str) -> Result<Self, String> {
        use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
        let mut value = HeaderValue::from_str(&format!("Bearer {}", api_key.trim()))
            .map_err(|_| "Clé API invalide (caractères interdits)".to_string())?;
        value.set_sensitive(true);
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, value);
        let http = reqwest::Client::builder().default_headers(headers).build().map_err(|e| e.to_string())?;
        Ok(Self { http, host: CLOUD_HOST.into(), backoff: default_backoff() })
    }

    /// Client sans authentification vers un serveur de test local.
    #[cfg(test)]
    pub fn for_test(host: impl Into<String>) -> Self {
        Self { http: reqwest::Client::new(), host: host.into(), backoff: vec![std::time::Duration::from_millis(5); 2] }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.host.trim_end_matches('/'), path)
    }

    pub async fn list_models(&self) -> Result<Vec<ModelInfo>, OllamaError> {
        let resp = self.http.get(self.url("/api/tags")).send().await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(OllamaError::Status(status.as_u16(), resp.text().await.unwrap_or_default()));
        }
        let v: Value = resp.json().await?;
        Ok(serde_json::from_value(v.get("models").cloned().unwrap_or(Value::Array(vec![])))?)
    }

    /// Lance `/api/chat` en streaming et appelle `on_chunk` pour chaque ligne NDJSON.
    pub async fn chat_stream<F>(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: Option<Vec<Value>>,
        options: Option<Value>,
        mut on_chunk: F,
    ) -> Result<(), OllamaError>
    where
        F: FnMut(ChatChunk),
    {
        let mut body = serde_json::json!({ "model": model, "messages": messages, "stream": true });
        // `think` n'est pas une option du modèle mais un paramètre de la requête : l'interface le glisse dans les options.
        let (think, options) = split_think(options);
        if let Some(t) = think {
            body["think"] = t;
        }
        if let Some(o) = options {
            body["options"] = o;
        }
        if let Some(t) = tools.filter(|t| !t.is_empty()) {
            body["tools"] = Value::Array(t);
        }
        // La demande n'est renvoyée que si rien n'a encore été reçu : un 429 arrive avant le début du flux.
        let mut attempt = 0;
        let resp = loop {
            let resp = self.http.post(self.url("/api/chat")).json(&body).send().await?;
            let status = resp.status();
            if status.is_success() {
                break resp;
            }
            if matches!(status.as_u16(), 429 | 503) && attempt < self.backoff.len() {
                let wait = retry_after(&resp).unwrap_or(self.backoff[attempt]);
                attempt += 1;
                tokio::time::sleep(wait).await;
                continue;
            }
            let text = resp.text().await.unwrap_or_default();
            return Err(OllamaError::Status(status.as_u16(), if status.as_u16() == 429 { format!("Limite d'usage atteinte. {text}") } else { text }));
        };

        let mut stream = resp.bytes_stream();
        let mut buf: Vec<u8> = Vec::new();
        while let Some(bytes) = stream.next().await {
            buf.extend_from_slice(&bytes?);
            while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                if let Some(chunk) = parse_line(&line)? {
                    on_chunk(chunk);
                }
            }
        }
        if let Some(chunk) = parse_line(&buf)? {
            on_chunk(chunk);
        }
        Ok(())
    }
}

impl OllamaClient {
    pub async fn show_model(&self, name: &str) -> Result<Value, OllamaError> {
        let resp = self
            .http
            .post(self.url("/api/show"))
            .json(&serde_json::json!({ "model": name }))
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            return Err(OllamaError::Status(status.as_u16(), resp.text().await.unwrap_or_default()));
        }
        Ok(resp.json().await?)
    }

}

/// Sépare `think` (booléen, ou « low » / « medium » / « high » pour gpt-oss) des autres options.
fn split_think(options: Option<Value>) -> (Option<Value>, Option<Value>) {
    match options {
        Some(Value::Object(mut o)) => {
            let think = o.remove("think");
            (think, (!o.is_empty()).then_some(Value::Object(o)))
        }
        other => (None, other),
    }
}

fn parse_line(line: &[u8]) -> Result<Option<ChatChunk>, OllamaError> {
    let line = std::str::from_utf8(line).unwrap_or("").trim();
    if line.is_empty() {
        return Ok(None);
    }
    let v: Value = serde_json::from_str(line)?;
    if let Some(err) = v.get("error").and_then(Value::as_str) {
        return Err(OllamaError::Status(500, err.to_string()));
    }
    let msg = v.get("message");
    let text = |k: &str| msg.and_then(|m| m.get(k)).and_then(Value::as_str).unwrap_or("").to_string();
    Ok(Some(ChatChunk {
        content: text("content"),
        thinking: text("thinking"),
        tool_calls: msg
            .and_then(|m| m.get("tool_calls"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        done: v.get("done").and_then(Value::as_bool).unwrap_or(false),
        prompt_tokens: v.get("prompt_eval_count").and_then(Value::as_u64).unwrap_or(0),
        eval_tokens: v.get("eval_count").and_then(Value::as_u64).unwrap_or(0),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test réseau : `cargo test live_ -- --ignored --nocapture` (envoie une fausse clé à ollama.com).
    #[tokio::test]
    #[ignore]
    async fn live_cloud_listing_is_public_but_chat_needs_a_valid_key() {
        let client = OllamaClient::cloud("clef-invalide").unwrap();
        let models = client.list_models().await.unwrap();
        assert!(!models.is_empty(), "le catalogue public doit répondre");
        let ask = ChatMessage { role: "user".into(), content: "ok".into(), thinking: None, tool_calls: None, tool_name: None, images: None };
        let res = client.chat_stream("gpt-oss:20b", &[ask], None, None, |_| {}).await;
        println!("chat avec fausse clé -> {res:?}");
        assert!(matches!(res, Err(OllamaError::Status(401 | 403, _))), "{res:?}");
    }

    /// Faux serveur : répond `first` à la première requête, puis une réponse normale.
    async fn serve(first: &'static str, retry_after: Option<&'static str>) -> (String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = hits.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else { return };
                let n = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let mut buf = [0u8; 16384];
                let _ = sock.read(&mut buf).await;
                let resp = if n == 0 && first != "200" {
                    let extra = retry_after.map(|r| format!("Retry-After: {r}\r\n")).unwrap_or_default();
                    format!("HTTP/1.1 {first}\r\n{extra}Content-Length: 4\r\nConnection: close\r\n\r\nbusy")
                } else {
                    let body = "{\"message\":{\"content\":\"ok\"},\"done\":true}\n";
                    format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
                };
                let _ = sock.write_all(resp.as_bytes()).await;
            }
        });
        (format!("http://{addr}"), hits)
    }

    fn hello() -> Vec<ChatMessage> {
        vec![ChatMessage { role: "user".into(), content: "x".into(), thinking: None, tool_calls: None, tool_name: None, images: None }]
    }

    #[tokio::test]
    async fn rate_limited_requests_are_retried_then_succeed() {
        let (host, hits) = serve("429 Too Many Requests", Some("0")).await;
        let mut got = String::new();
        OllamaClient::for_test(host).chat_stream("m", &hello(), None, None, |c| got.push_str(&c.content)).await.unwrap();
        assert_eq!(got, "ok");
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 2, "une tentative refusée puis une réussie");
    }

    #[tokio::test]
    async fn persistent_rate_limit_gives_a_clear_error_after_the_retries() {
        let (host, hits) = serve("429 Too Many Requests", None).await;
        // Le serveur ne refuse que la première requête : on force 3 refus en le relançant avec un client sans tentative restante.
        let mut client = OllamaClient::for_test(host);
        client.backoff.clear();
        let err = client.chat_stream("m", &hello(), None, None, |_| {}).await.unwrap_err();
        assert!(matches!(&err, OllamaError::Status(429, m) if m.contains("Limite d'usage")), "{err:?}");
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1, "sans attente prévue, pas de nouvel essai");
    }

    #[tokio::test]
    async fn other_errors_are_not_retried() {
        let (host, hits) = serve("401 Unauthorized", None).await;
        let err = OllamaClient::for_test(host).chat_stream("m", &hello(), None, None, |_| {}).await.unwrap_err();
        assert!(matches!(err, OllamaError::Status(401, _)));
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn cloud_client_rejects_keys_with_forbidden_characters() {
        assert!(OllamaClient::cloud("abc\ndef").is_err());
        assert!(OllamaClient::cloud("abc123").is_ok());
    }

    #[test]
    fn parses_content_line() {
        let c = parse_line(br#"{"message":{"role":"assistant","content":"Salut"},"done":false}"#)
            .unwrap()
            .unwrap();
        assert_eq!(c.content, "Salut");
        assert!(!c.done);
    }

    #[test]
    fn parses_done_and_tool_calls() {
        let c = parse_line(
            br#"{"message":{"content":"","tool_calls":[{"function":{"name":"x"}}]},"done":true}"#,
        )
        .unwrap()
        .unwrap();
        assert!(c.done);
        assert_eq!(c.tool_calls.len(), 1);
    }

    #[test]
    fn think_moves_out_of_the_options() {
        let (t, o) = split_think(Some(serde_json::json!({ "num_ctx": 8192, "think": false })));
        assert_eq!(t, Some(serde_json::json!(false)));
        assert_eq!(o, Some(serde_json::json!({ "num_ctx": 8192 })));
        let (t, o) = split_think(Some(serde_json::json!({ "think": "low" })));
        assert_eq!((t, o), (Some(serde_json::json!("low")), None));
        assert_eq!(split_think(None), (None, None));
    }

    #[test]
    fn reads_generated_tokens_too() {
        let c = parse_line(br#"{"message":{"content":""},"done":true,"prompt_eval_count":10,"eval_count":42}"#).unwrap().unwrap();
        assert_eq!((c.prompt_tokens, c.eval_tokens), (10, 42));
    }

    #[test]
    fn parses_token_usage_on_final_line() {
        let c = parse_line(br#"{"message":{"content":""},"done":true,"prompt_eval_count":321,"eval_count":9}"#)
            .unwrap()
            .unwrap();
        assert_eq!(c.prompt_tokens, 321);
    }

    #[test]
    fn empty_line_is_none_and_error_line_fails() {
        assert!(parse_line(b"  \n").unwrap().is_none());
        assert!(parse_line(br#"{"error":"model not found"}"#).is_err());
    }
}
