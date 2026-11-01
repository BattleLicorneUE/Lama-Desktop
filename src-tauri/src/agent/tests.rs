use super::*;
use super::approval::PC_ACTIONS;
use super::meta::read_attachment;

fn write() -> Prepared {
    Prepared::Write { path: "x".into(), before: None, after: String::new(), diff: String::new() }
}

/// Test de bout en bout avec un vrai modèle : `cargo test live_ -- --ignored --nocapture`
/// (modèle via LIVE_MODEL, `qwen3:8b` par défaut).
#[tokio::test]
#[ignore]
async fn live_agent_fixes_a_bug_and_verifies() {
    let dir = std::env::temp_dir().join(format!("agent-live-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("package.json"), r#"{"scripts":{"test":"node test.js"}}"#).unwrap();
    std::fs::write(dir.join("calc.js"), "function add(a, b) {\n  return a - b;\n}\nmodule.exports = { add };\n").unwrap();
    std::fs::write(
        dir.join("test.js"),
        "const { add } = require('./calc');\nif (add(2, 3) !== 5) { console.error('FAIL: add(2,3) =', add(2, 3)); process.exit(1); }\nconsole.log('OK');\n",
    )
    .unwrap();

    struct TestHost(std::sync::Mutex<Vec<String>>);
    impl Host for Arc<TestHost> {
        fn emit_json(&self, event: &str, payload: Value) {
            if event == "agent:tool" && payload["state"] != "running" {
                self.0.lock_safe().push(format!(
                    "{} {} -> {}",
                    payload["name"], payload["args"], payload["state"]
                ));
            }
        }
        fn remember(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
    }
    let host = Arc::new(TestHost(Default::default()));
    let params = RunParams {
        request_id: "live".into(),
        model: std::env::var("LIVE_MODEL").unwrap_or_else(|_| "gpt-oss:120b".into()),
        messages: vec![ChatMessage {
            role: "user".into(),
            content: "Les tests échouent. Corrige le bug dans le projet.".into(),
            thinking: None, tool_calls: None, tool_name: None, images: None,
        }],
        workspace: Some(dir.clone()),
        mode: "bypass".into(),
        options: Some(json!({ "num_ctx": 8192 })),
        skills: vec![],
        web: false,
            profile: None,
        agents: vec![],
        depth: 0,
        attachments: vec![],
        docs: vec![],
        project_id: None,
    };
    let key = std::env::var("OLLAMA_API_KEY").expect("définis OLLAMA_API_KEY pour ce test");
    let client = OllamaClient::cloud(&key).unwrap();
    let state = Arc::new(AgentState::default());
    run(host.clone(), client, state.clone(), Arc::new(McpHub::default()), params).await.unwrap();
    println!("--- appels d'outils ---\n{}", host.0.lock_safe().join("\n"));

    let calc = std::fs::read_to_string(dir.join("calc.js")).unwrap();
    println!("--- calc.js final ---\n{calc}");
    println!("modifications enregistrées : {}", state.changes.lock().await.len());
    let out = tools::exec_command(&dir, "node test.js").await.unwrap();
    assert!(out.contains("OK"), "les tests échouent toujours : {out}");
}

/// Hôte de test : enregistre les événements émis.
struct RecHost(std::sync::Mutex<Vec<(String, Value)>>);
impl Host for Arc<RecHost> {
    fn emit_json(&self, event: &str, payload: Value) {
        self.0.lock_safe().push((event.to_string(), payload));
    }
    fn remember(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
}

/// Faux serveur Ollama : le parent délègue à deux agents dans le même tour, les sous-agents répondent, le parent conclut.
async fn fake_ollama() -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else { return };
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 8192];
                let body: Value = loop {
                    let n = sock.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    if let Some(i) = text.find("\r\n\r\n") {
                        let len = text[..i].to_lowercase().split("content-length:").nth(1).and_then(|r| r.lines().next()).and_then(|l| l.trim().parse::<usize>().ok()).unwrap_or(0);
                        if buf.len() >= i + 4 + len {
                            break serde_json::from_slice(&buf[i + 4..i + 4 + len]).unwrap();
                        }
                    }
                };
                let msgs = body["messages"].as_array().unwrap();
                let system = msgs[0]["content"].as_str().unwrap_or("");
                let reply = if system.starts_with("# Rôle") {
                    let who = if system.contains("explorateur") { "explorateur" } else { "relecteur" };
                    json!({ "message": { "role": "assistant", "content": format!("Résumé de {who}") }, "done": true })
                } else if msgs.iter().any(|m| m["role"] == "tool") {
                    json!({ "message": { "role": "assistant", "content": "Terminé." }, "done": true })
                } else {
                    let call = |agent: &str| json!({ "function": { "name": "delegate", "arguments": { "agent": agent, "task": "Fais ta tâche" } } });
                    json!({ "message": { "role": "assistant", "content": "", "tool_calls": [call("explorateur"), call("relecteur")] }, "done": true })
                };
                let payload = format!("{reply}\n");
                let head = format!("HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", payload.len());
                let _ = sock.write_all(head.as_bytes()).await;
                let _ = sock.write_all(payload.as_bytes()).await;
            });
        }
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn delegation_runs_isolated_sub_agents_in_parallel_and_returns_their_reports() {
    let host = Arc::new(RecHost(Default::default()));
    let params = RunParams {
        request_id: "r1".into(),
        model: "m".into(),
        messages: vec![ChatMessage { role: "user".into(), content: "Analyse le projet".into(), thinking: None, tool_calls: None, tool_name: None, images: None }],
        workspace: Some(std::env::temp_dir()),
        mode: "auto".into(),
        options: None,
        skills: vec![],
        web: false,
            profile: None,
        agents: agents::builtins(),
        depth: 0,
        attachments: vec![],
        docs: vec![],
        project_id: None,
    };
    let client = OllamaClient::for_test(fake_ollama().await);
    run(host.clone(), client, Arc::new(AgentState::default()), Arc::new(McpHub::default()), params).await.unwrap();

    let events = host.0.lock_safe();
    let tools: Vec<&Value> = events.iter().filter(|(e, _)| e == "agent:tool").map(|(_, p)| p).collect();
    let done: Vec<&&Value> = tools.iter().filter(|p| p["name"] == "delegate" && p["state"] == "done").collect();
    assert_eq!(done.len(), 2, "deux délégations terminées : {tools:?}");
    let outputs: Vec<&str> = done.iter().filter_map(|p| p["output"].as_str()).collect();
    assert!(outputs.contains(&"Résumé de explorateur") && outputs.contains(&"Résumé de relecteur"), "{outputs:?}");
    // Seul l'agent principal parle : le texte des sous-agents n'est pas diffusé.
    let streamed: String = events.iter().filter(|(e, _)| e == "chat:chunk").filter_map(|(_, p)| p["chunk"]["content"].as_str()).collect();
    assert_eq!(streamed, "Terminé.");
    assert_eq!(events.iter().filter(|(e, p)| e == "chat:chunk" && p["chunk"]["done"] == true).count(), 1);
}

/// Faux serveur Ollama scripté : une réponse par requête (la dernière se répète) ; les requêtes reçues sont conservées.
async fn scripted_ollama(replies: Vec<Value>) -> (String, Arc<std::sync::Mutex<Vec<Value>>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let seen = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
    let log = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else { return };
            let mut buf = Vec::new();
            let mut chunk = [0u8; 8192];
            let body: Value = loop {
                let n = sock.read(&mut chunk).await.unwrap_or(0);
                if n == 0 {
                    break Value::Null;
                }
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf).to_string();
                if let Some(i) = text.find("\r\n\r\n") {
                    let len = text[..i].to_lowercase().split("content-length:").nth(1).and_then(|r| r.lines().next()).and_then(|l| l.trim().parse::<usize>().ok()).unwrap_or(0);
                    if buf.len() >= i + 4 + len {
                        break serde_json::from_slice(&buf[i + 4..i + 4 + len]).unwrap();
                    }
                }
            };
            if body.is_null() {
                continue;
            }
            let index = {
                let mut l = log.lock_safe();
                l.push(body);
                l.len() - 1
            };
            let reply = replies[index.min(replies.len() - 1)].clone();
            let payload = format!("{reply}\n");
            let head = format!("HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", payload.len());
            let _ = sock.write_all(head.as_bytes()).await;
            let _ = sock.write_all(payload.as_bytes()).await;
        }
    });
    (format!("http://{addr}"), seen)
}

#[tokio::test]
async fn agent_is_asked_to_verify_after_editing_without_verifying() {
    let dir = std::env::temp_dir().join(format!("nudge-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("package.json"), r#"{"scripts":{"check":"echo tout-va-bien"}}"#).unwrap();
    let call = |name: &str, args: Value| json!({ "message": { "role": "assistant", "content": "", "tool_calls": [{ "function": { "name": name, "arguments": args } }] }, "done": true });
    let say = |t: &str| json!({ "message": { "role": "assistant", "content": t }, "done": true });
    let (url, seen) = scripted_ollama(vec![
        call("write_file", json!({ "path": "a.txt", "content": "salut" })),
        say("C'est fait."),
        call("verify", json!({})),
        say("Vérifié."),
    ])
    .await;
    let host = Arc::new(RecHost(Default::default()));
    let params = RunParams {
        request_id: "n1".into(),
        model: "m".into(),
        messages: vec![ChatMessage { role: "user".into(), content: "Crée a.txt".into(), thinking: None, tool_calls: None, tool_name: None, images: None }],
        workspace: Some(dir.clone()),
        mode: "bypass".into(),
        options: None,
        skills: vec![],
        web: false,
        profile: None,
        agents: vec![],
        depth: 0,
        attachments: vec![],
        docs: vec![],
        project_id: None,
    };
    let state = Arc::new(AgentState::default());
    run(host.clone(), OllamaClient::for_test(url), state.clone(), Arc::new(McpHub::default()), params).await.unwrap();

    let reqs = seen.lock_safe();
    assert_eq!(reqs.len(), 4, "écriture, conclusion trop tôt, vérification, conclusion");
    let reminder = reqs[2]["messages"].as_array().unwrap().iter().any(|m| m["role"] == "user" && m["content"].as_str().is_some_and(|c| c.starts_with("(Rappel automatique)")));
    assert!(reminder, "le rappel doit figurer dans la 3e requête");
    let events = host.0.lock_safe();
    assert!(events.iter().any(|(e, p)| e == "agent:tool" && p["name"] == "verify" && p["state"] == "done"));
    let shown: String = events.iter().filter(|(e, _)| e == "chat:chunk").filter_map(|(_, p)| p["chunk"]["content"].as_str()).collect();
    assert_eq!(shown, "C'est fait.\n\nVérifié.", "les textes de deux étapes ne doivent pas se coller");
    let steps = events.iter().filter(|(e, _)| e == "agent:step").count();
    assert_eq!(steps, 3, "un marqueur à chaque étape après la première (4 requêtes)");
    // Bilan de la session : un fichier créé, avec son diff.
    let report = state.changes_report().await;
    assert_eq!(report.len(), 1);
    assert_eq!(report[0].kind, "created");
    assert!(report[0].diff.as_deref().unwrap().contains("+salut"));
    let _ = std::fs::remove_dir_all(dir);
}

/// Hôte qui « tape » une précision dans la boîte de réception dès le premier appel d'outil.
struct TypingHost(Arc<RecHost>, Arc<AgentState>);
impl Host for TypingHost {
    fn emit_json(&self, event: &str, payload: Value) {
        if event == "agent:tool" && self.1.inbox.lock_safe().is_empty() && !self.0 .0.lock_safe().iter().any(|(e, _)| e == "agent:injected") {
            self.1.inbox.lock_safe().entry("i1".into()).or_default().push("Mets plutôt « bonjour ».".into());
        }
        self.0.emit_json(event, payload);
    }
    fn remember(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
}

#[tokio::test]
async fn user_can_add_a_request_while_the_agent_works() {
    let dir = std::env::temp_dir().join(format!("inject-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let call = |name: &str, args: Value| json!({ "message": { "role": "assistant", "content": "", "tool_calls": [{ "function": { "name": name, "arguments": args } }] }, "done": true });
    let say = |t: &str| json!({ "message": { "role": "assistant", "content": t }, "done": true });
    let (url, seen) = scripted_ollama(vec![call("write_file", json!({ "path": "a.txt", "content": "salut" })), say("Fait.")]).await;
    let rec = Arc::new(RecHost(Default::default()));
    let state = Arc::new(AgentState::default());
    let params = RunParams {
        request_id: "i1".into(),
        model: "m".into(),
        messages: vec![ChatMessage { role: "user".into(), content: "Crée a.txt".into(), thinking: None, tool_calls: None, tool_name: None, images: None }],
        workspace: Some(dir.clone()),
        mode: "bypass".into(),
        options: None,
        skills: vec![],
        web: false,
        profile: None,
        agents: vec![],
        depth: 0,
        attachments: vec![],
        docs: vec![],
        project_id: None,
    };
    run(TypingHost(rec.clone(), state.clone()), OllamaClient::for_test(url), state.clone(), Arc::new(McpHub::default()), params).await.unwrap();
    let reqs = seen.lock_safe();
    assert_eq!(reqs.len(), 2, "la tâche continue sans repartir de zéro");
    let has = |r: &Value| r["messages"].as_array().unwrap().iter().any(|m| m["role"] == "user" && m["content"].as_str().is_some_and(|c| c.contains("Mets plutôt « bonjour »")));
    assert!(!has(&reqs[0]) && has(&reqs[1]), "la précision rejoint la conversation à l'étape suivante");
    assert!(rec.0.lock_safe().iter().any(|(e, p)| e == "agent:injected" && p["text"] == "Mets plutôt « bonjour »."));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn attachment_passages_are_found_by_word_or_range() {
    let text = format!("{}Le mot-clé ÉCLAIR ici.{}", "a".repeat(1000), "b".repeat(1000));
    let list = vec![("rapport.txt".to_string(), text), ("autre.md".to_string(), "rien".to_string())];
    let found = read_attachment(&list, &json!({ "name": "rapport", "search": "éclair" })).unwrap();
    assert!(found.contains("ÉCLAIR ici") && found.contains("position 1011"), "{found}");
    assert!(read_attachment(&list, &json!({ "name": "rapport.txt", "search": "absent" })).unwrap().contains("n'apparaît pas"));
    let slice = read_attachment(&list, &json!({ "name": "rapport.txt", "start": 1000, "length": 30 })).unwrap();
    assert!(slice.contains("Le mot-clé") && slice.contains("Suite : start = 1030"), "{slice}");
    assert!(read_attachment(&list, &json!({ "name": "inconnu" })).unwrap_err().contains("autre.md"));
    assert!(read_attachment(&list, &json!({ "name": "autre.md", "start": 99 })).is_err());
}

/// Hôte de test pour la mémoire de projet et la recherche dans les conversations.
struct MemHost(std::sync::Mutex<Vec<String>>);
impl Host for Arc<MemHost> {
    fn emit_json(&self, _: &str, _: Value) {}
    fn remember(&self, text: &str) -> Result<(), String> {
        self.0.lock_safe().push(format!("global:{text}"));
        Ok(())
    }
    fn remember_in(&self, text: &str, project: Option<&str>) -> Result<(), String> {
        self.0.lock_safe().push(format!("{}:{text}", project.unwrap_or("global")));
        Ok(())
    }
    fn search_chats(&self, query: &str) -> Result<String, String> {
        Ok(format!("2 conversation(s) passée(s) pour « {query} » : « Billets de train »"))
    }
}

#[tokio::test]
async fn project_docs_chats_and_project_memory_tools_are_wired_without_approval() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let seen: Arc<std::sync::Mutex<Vec<Value>>> = Default::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let log = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else { return };
            let log = log.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 8192];
                let body: Value = loop {
                    let n = sock.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    if let Some(i) = text.find("\r\n\r\n") {
                        let len = text[..i].to_lowercase().split("content-length:").nth(1).and_then(|r| r.lines().next()).and_then(|l| l.trim().parse::<usize>().ok()).unwrap_or(0);
                        if buf.len() >= i + 4 + len {
                            break serde_json::from_slice(&buf[i + 4..i + 4 + len]).unwrap();
                        }
                    }
                };
                let done = body["messages"].as_array().unwrap().iter().any(|m| m["role"] == "tool");
                log.lock_safe().push(body);
                let call = |name: &str, args: Value| json!({ "function": { "name": name, "arguments": args } });
                let reply = if done {
                    json!({ "message": { "role": "assistant", "content": "Voilà." }, "done": true })
                } else {
                    json!({ "message": { "role": "assistant", "content": "", "tool_calls": [
                        call("search_project_docs", json!({ "query": "préavis résiliation" })),
                        call("search_chats", json!({ "query": "billet" })),
                        call("remember", json!({ "text": "Budget 2000 €", "scope": "project" })),
                    ] }, "done": true })
                };
                let payload = format!("{reply}\n");
                let head = format!("HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", payload.len());
                let _ = sock.write_all(head.as_bytes()).await;
                let _ = sock.write_all(payload.as_bytes()).await;
            });
        }
    });
    let host = Arc::new(MemHost(Default::default()));
    let filler = "Texte sans rapport. ".repeat(80);
    let params = RunParams {
        request_id: "r1".into(),
        model: "m".into(),
        messages: vec![ChatMessage { role: "user".into(), content: "Que dit le contrat ?".into(), thinking: None, tool_calls: None, tool_name: None, images: None }],
        workspace: None,
        mode: "manual".into(),
        options: None,
        skills: vec![],
        web: false,
        profile: None,
        agents: vec![],
        depth: 0,
        attachments: vec![],
        docs: vec![("contrat.md".into(), format!("{filler}\nLa résiliation exige un préavis de trois mois.\n{filler}"))],
        project_id: Some("proj-1".into()),
    };
    let state = Arc::new(AgentState::default());
    run(host.clone(), OllamaClient::for_test(url), state, Arc::new(McpHub::default()), params).await.unwrap();
    let reqs = seen.lock_safe();
    assert_eq!(reqs.len(), 2, "aucune approbation ne doit bloquer ces outils, même en mode manuel");
    let names: Vec<&str> = reqs[0]["tools"].as_array().unwrap().iter().filter_map(|t| t["function"]["name"].as_str()).collect();
    assert!(names.contains(&"search_project_docs") && names.contains(&"search_chats") && names.contains(&"remember"), "{names:?}");
    let tool_text: String = reqs[1]["messages"].as_array().unwrap().iter().filter(|m| m["role"] == "tool").map(|m| m["content"].as_str().unwrap_or("").to_string()).collect::<Vec<_>>().join("\n");
    assert!(tool_text.contains("préavis de trois mois"), "{tool_text}");
    assert!(tool_text.contains("Billets de train"), "{tool_text}");
    assert!(tool_text.contains("Mémorisé pour ce projet."), "{tool_text}");
    assert_eq!(host.0.lock_safe().as_slice(), ["proj-1:Budget 2000 €"]);
}

#[tokio::test]
async fn auto_mode_asks_once_per_folder_before_running_project_code() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else { return };
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 8192];
                let body: Value = loop {
                    let n = sock.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    if let Some(i) = text.find("\r\n\r\n") {
                        let len = text[..i].to_lowercase().split("content-length:").nth(1).and_then(|r| r.lines().next()).and_then(|l| l.trim().parse::<usize>().ok()).unwrap_or(0);
                        if buf.len() >= i + 4 + len {
                            break serde_json::from_slice(&buf[i + 4..i + 4 + len]).unwrap();
                        }
                    }
                };
                let tools_done = body["messages"].as_array().unwrap().iter().filter(|m| m["role"] == "tool").count();
                let call = || json!({ "function": { "name": "run_command", "arguments": { "command": "npm test" } } });
                let reply = if tools_done >= 2 {
                    json!({ "message": { "role": "assistant", "content": "Fini." }, "done": true })
                } else {
                    json!({ "message": { "role": "assistant", "content": "", "tool_calls": [call()] }, "done": true })
                };
                let payload = format!("{reply}\n");
                let head = format!("HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", payload.len());
                let _ = sock.write_all(head.as_bytes()).await;
                let _ = sock.write_all(payload.as_bytes()).await;
            });
        }
    });
    let dir = std::env::temp_dir().join(format!("trust-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let host = Arc::new(RecHost(Default::default()));
    let params = RunParams {
        request_id: "r1".into(),
        model: "m".into(),
        messages: vec![ChatMessage { role: "user".into(), content: "Lance les tests deux fois".into(), thinking: None, tool_calls: None, tool_name: None, images: None }],
        workspace: Some(dir.clone()),
        mode: "auto".into(),
        options: None,
        skills: vec![],
        web: false,
        profile: None,
        agents: vec![],
        depth: 0,
        attachments: vec![],
        docs: vec![],
        project_id: None,
    };
    let state = Arc::new(AgentState::default());
    // L'utilisateur accepte chaque demande qui apparaît.
    let approver = state.clone();
    let answering = tokio::spawn(async move {
        for _ in 0..400 {
            {
                let mut pending = approver.approvals.lock().await;
                let ids: Vec<String> = pending.keys().cloned().collect();
                for id in ids {
                    if let Some(tx) = pending.remove(&id) {
                        let _ = tx.send(true);
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    });
    run(host.clone(), OllamaClient::for_test(url), state.clone(), Arc::new(McpHub::default()), params).await.unwrap();
    answering.abort();
    let asked = host.0.lock_safe().iter().filter(|(e, p)| e == "agent:tool" && p["state"] == "awaiting").count();
    assert_eq!(asked, 1, "l'accord est demandé la première fois seulement, pas à la seconde commande");
    let _ = std::fs::remove_dir_all(dir);
}

/// Le chemin complet d'une connexion demandée par le modèle : la demande est résolue, montrée à l'utilisateur avec le verdict
/// sur la page, exige l'accord même en « Ignorer les permissions », et le mot de passe n'est jamais envoyé au modèle.
#[tokio::test]
async fn vault_login_is_resolved_approved_even_in_bypass_and_never_leaks_the_password() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let seen: Arc<std::sync::Mutex<Vec<Value>>> = Default::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let log = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else { return };
            let log = log.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 8192];
                let body: Value = loop {
                    let n = sock.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    if let Some(i) = text.find("\r\n\r\n") {
                        let len = text[..i].to_lowercase().split("content-length:").nth(1).and_then(|r| r.lines().next()).and_then(|l| l.trim().parse::<usize>().ok()).unwrap_or(0);
                        if buf.len() >= i + 4 + len {
                            break serde_json::from_slice(&buf[i + 4..i + 4 + len]).unwrap();
                        }
                    }
                };
                let done = body["messages"].as_array().unwrap().iter().any(|m| m["role"] == "tool");
                log.lock_safe().push(body);
                let reply = if done {
                    json!({ "message": { "role": "assistant", "content": "Voilà." }, "done": true })
                } else {
                    json!({ "message": { "role": "assistant", "content": "", "tool_calls": [
                        { "function": { "name": "vault_login", "arguments": { "entry": "github", "submit": true } } }
                    ] }, "done": true })
                };
                let payload = format!("{reply}\n");
                let head = format!("HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", payload.len());
                let _ = sock.write_all(head.as_bytes()).await;
                let _ = sock.write_all(payload.as_bytes()).await;
            });
        }
    });
    // Le coffre global du processus, ouvert avec un profil jetable.
    let dir = std::env::temp_dir().join(format!("agent-vault-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let vault = crate::vault::global();
    vault.set_dir(Some(&dir));
    vault.create("phrase de passe très longue").unwrap();
    vault.save_login(crate::vault::Login { title: "GitHub".into(), url: "https://github.com/login".into(), username: "samuel".into(), password: "MOT-DE-PASSE-AGENT-42".into(), ..Default::default() }).unwrap();
    vault.set_agent_access(true).unwrap();
    // Aucun navigateur ouvert dans ce test : la connexion échouera, mais après la demande d'accord.
    crate::browser::set_dir(Some(&dir));

    let host = Arc::new(RecHost(Default::default()));
    let params = RunParams {
        request_id: "r1".into(),
        model: "m".into(),
        messages: vec![ChatMessage { role: "user".into(), content: "Connecte-moi à GitHub".into(), thinking: None, tool_calls: None, tool_name: None, images: None }],
        workspace: None,
        mode: "bypass".into(),
        options: None,
        skills: vec![],
        web: false,
        profile: None,
        agents: vec![],
        depth: 0,
        attachments: vec![],
        docs: vec![],
        project_id: None,
    };
    let state = Arc::new(AgentState::default());
    let approver = state.clone();
    let answering = tokio::spawn(async move {
        for _ in 0..400 {
            {
                let mut pending = approver.approvals.lock().await;
                let ids: Vec<String> = pending.keys().cloned().collect();
                for id in ids {
                    if let Some(tx) = pending.remove(&id) {
                        let _ = tx.send(true);
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    });
    run(host.clone(), OllamaClient::for_test(url), state, Arc::new(McpHub::default()), params).await.unwrap();
    answering.abort();

    let events = host.0.lock_safe().clone();
    let awaiting: Vec<&Value> = events.iter().filter(|(e, p)| e == "agent:tool" && p["state"] == "awaiting").map(|(_, p)| p).collect();
    assert_eq!(awaiting.len(), 1, "l'accord est demandé, même en « Ignorer les permissions »");
    let shown = &awaiting[0]["args"];
    assert_eq!((shown["label"].as_str(), shown["site"].as_str(), shown["username"].as_str()), (Some("GitHub"), Some("github.com"), Some("samuel")), "la demande affichée est l'entrée exacte du coffre, pas le mot du modèle");
    assert_eq!(shown["allowed"], false, "aucune page ouverte : le verdict est négatif");
    assert!(shown["why"].as_str().unwrap().contains("navigateur"), "{shown}");
    assert!(!shown.to_string().contains("MOT-DE-PASSE-AGENT-42"), "le mot de passe ne figure pas dans la demande");

    // Ce que le modèle a reçu, du début à la fin : jamais le mot de passe.
    let everything = serde_json::to_string(&*seen.lock_safe()).unwrap();
    assert!(!everything.contains("MOT-DE-PASSE-AGENT-42"), "le mot de passe ne doit jamais atteindre le modèle");
    assert!(everything.contains("navigateur n'est pas ouvert"), "le modèle apprend pourquoi la connexion a échoué");
    let results: Vec<&Value> = events.iter().filter(|(e, p)| e == "agent:tool" && p["name"] == "vault_login" && p["state"] == "error").map(|(_, p)| p).collect();
    assert_eq!(results.len(), 1);

    vault.lock();
    vault.set_dir(None);
    crate::browser::set_dir(None);
    let _ = std::fs::remove_dir_all(dir);
}

fn msg(role: &str, tool: Option<&str>, len: usize) -> ChatMessage {
    ChatMessage { role: role.into(), content: "x".repeat(len), thinking: None, tool_calls: None, tool_name: tool.map(String::from), images: None }
}

#[test]
fn old_tool_results_are_replaced_but_recent_and_small_ones_stay() {
    let mut msgs = vec![msg("user", None, 50)];
    for i in 0..10 {
        msgs.push(msg("assistant", None, 10));
        msgs.push(msg("tool", Some("read_file"), if i == 1 { 100 } else { 5_000 }));
    }
    let before = estimate_tokens(&msgs, &[]);
    let n = trim_tool_outputs(&mut msgs, 3);
    // 10 résultats, 3 conservés, un des 7 anciens est trop petit pour valoir le remplacement.
    assert_eq!(n, 6);
    let tools: Vec<&ChatMessage> = msgs.iter().filter(|m| m.role == "tool").collect();
    assert!(tools[0].content.starts_with("[résultat de read_file retiré"));
    assert_eq!(tools[1].content.len(), 100);
    assert!(tools[7..].iter().all(|m| m.content.len() == 5_000));
    assert!(estimate_tokens(&msgs, &[]) < before / 2);
    assert_eq!(trim_tool_outputs(&mut msgs, 3), 0, "idempotent");
    assert_eq!(msgs[0].content.len(), 50, "les messages de l'utilisateur ne sont jamais touchés");
}

#[test]
fn permission_matrix() {
    for n in PC_ACTIONS {
        let t = Prepared::Meta { name: (*n).into(), args: Value::Null };
        assert!(needs_approval("auto", &t) && needs_approval("accept-edits", &t) && needs_approval("manual", &t), "{n}");
        assert!(!needs_approval("bypass", &t));
    }
    let exec = |c: &str| Prepared::Exec { command: c.into() };
    let read = || Prepared::Done(String::new());
    assert!(!needs_approval("manual", &read()));
    assert!(needs_approval("manual", &write()));
    assert!(needs_approval("manual", &exec("git status")));
    assert!(!needs_approval("accept-edits", &write()));
    assert!(needs_approval("accept-edits", &exec("git status")));
    assert!(!needs_approval("auto", &write()));
    assert!(!needs_approval("auto", &exec("git status")));
    assert!(needs_approval("auto", &exec("npm install")));
    assert!(!needs_approval("bypass", &exec("rm -r x")));
    let mkdir = || Prepared::Fs(FsOp::MakeDir("x".into()));
    let del = || Prepared::Fs(FsOp::Delete("x".into()));
    assert!(!needs_approval("auto", &mkdir()) && !needs_approval("accept-edits", &mkdir()));
    assert!(needs_approval("manual", &mkdir()));
    assert!(needs_approval("auto", &del()) && needs_approval("accept-edits", &del()));
    assert!(!needs_approval("bypass", &del()));
    let meta = |n: &str| Prepared::Meta { name: n.into(), args: Value::Null };
    assert!(!needs_approval("manual", &meta("web_search")), "la recherche web ne demande pas de fenêtre");
    assert!(!needs_approval("manual", &meta("remember")));
    // Lire une page web peut envoyer des données dans l'adresse : en mode Manuel, chaque page demande l'accord.
    assert!(needs_approval("manual", &meta("web_fetch")));
    assert!(!needs_approval("auto", &meta("web_fetch")));
    // Le coffre : accord obligatoire dans tous les modes, y compris « Ignorer les permissions ».
    for mode in ["manual", "accept-edits", "auto", "plan", "bypass"] {
        assert!(needs_approval(mode, &meta("vault_fill")) && needs_approval(mode, &meta("vault_login")), "{mode}");
    }
    // Navigateur : ouvrir, cliquer et saisir demandent l'accord (sauf « Ignorer les permissions ») ; lire et fermer sont libres hors mode Manuel.
    for tool in ["browser_open", "browser_click", "browser_type"] {
        assert!(needs_approval("auto", &meta(tool)) && needs_approval("manual", &meta(tool)) && !needs_approval("bypass", &meta(tool)), "{tool}");
    }
    assert!(needs_approval("manual", &meta("browser_read")) && !needs_approval("auto", &meta("browser_read")) && !needs_approval("manual", &meta("browser_close")));
    assert!(!needs_approval("manual", &meta("vault_list")));
    assert!(!needs_approval("manual", &meta("use_skill")));
    assert!(!needs_approval("manual", &meta("update_tasks")));
    assert!(!needs_approval("auto", &meta("web_search")));
    assert!(!needs_approval("manual", &meta("system_info")));
    assert!(needs_approval("auto", &meta("open_target")) && needs_approval("accept-edits", &meta("open_target")));
    assert!(!needs_approval("bypass", &meta("open_target")));
}
