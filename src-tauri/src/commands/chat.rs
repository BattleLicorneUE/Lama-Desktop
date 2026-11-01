//! Discussion et agent : démarrage, annulation, approbations, annulation des modifications.

use crate::*;

/// Catalogue des modèles d'Ollama Cloud. La liste est publique : elle ne prouve pas que la clé est valide.
#[tauri::command]
pub(crate) async fn cloud_models(key: String) -> Result<Vec<ModelInfo>, OllamaError> {
    client_for(&key)?.list_models().await
}

/// Vérifie la clé avec un tout petit message (quelques jetons) : seul un appel de chat est authentifié.
#[tauri::command]
pub(crate) async fn cloud_test(key: String, model: String) -> Result<String, OllamaError> {
    let ask = ChatMessage {
        role: "user".into(),
        content: "Réponds seulement : ok".into(),
        thinking: None,
        tool_calls: None,
        tool_name: None,
        images: None,
    };
    let mut out = String::new();
    client_for(&key)?
        .chat_stream(&model, &[ask], None, Some(serde_json::json!({ "num_predict": 8 })), |c| out.push_str(&c.content))
        .await
        .map_err(|e| match e {
            OllamaError::Status(401 | 403, _) => OllamaError::Status(401, "clé API refusée par Ollama Cloud".into()),
            other => other,
        })?;
    Ok(out)
}

/// Démarre un chat en streaming ; les tokens arrivent via l'event `chat:chunk`.
#[tauri::command]
pub(crate) async fn chat_start(
    app: AppHandle,
    state: State<'_, AppState>,
    request_id: String,
    model: String,
    messages: Vec<ChatMessage>,
    options: Option<serde_json::Value>,
    cloud_key: String,
) -> Result<(), String> {
    track(&app, "chat_started", serde_json::json!({ "kind": if model.starts_with("local:") { "local" } else { "cloud" }, "model": model }));
    let running = state.running.clone();
    let rid = request_id.clone();
    let handle = tokio::spawn(async move {
        let emit_app = app.clone();
        let rid2 = rid.clone();
        let client = match client_for_model(&app, &cloud_key, &model).await {
            Ok(c) => c,
            Err(e) => {
                track_error(&app, &e.to_string(), &model);
                let _ = app.emit("chat:error", serde_json::json!({ "id": rid, "error": e.to_string() }));
                running.lock().await.remove(&rid);
                return;
            }
        };
        let result = client
            .chat_stream(&model, &messages, None, options, move |chunk| {
                if chunk.done && (chunk.prompt_tokens > 0 || chunk.eval_tokens > 0) {
                    let _ = emit_app.emit("usage:tokens", serde_json::json!({ "id": rid2, "prompt": chunk.prompt_tokens, "eval": chunk.eval_tokens }));
                }
                if chunk.done && chunk.prompt_tokens > 0 {
                    let _ = emit_app.emit(
                        "chat:usage",
                        serde_json::json!({ "id": rid2, "prompt_tokens": chunk.prompt_tokens }),
                    );
                }
                let _ = emit_app.emit("chat:chunk", serde_json::json!({ "id": rid2, "chunk": chunk }));
            })
            .await;
        if let Err(e) = result {
            track_error(&app, &e.to_string(), &model);
            let _ = app.emit("chat:error", serde_json::json!({ "id": rid, "error": e.to_string() }));
        }
        running.lock().await.remove(&rid);
    });
    state.running.lock().await.insert(request_id, handle.abort_handle());
    Ok(())
}

/// Fichier joint transmis à l'agent pour son outil `read_attachment`.
#[derive(serde::Deserialize)]
pub(crate) struct AttachmentIn {
    pub(crate) name: String,
    pub(crate) content: String,
}

/// Lance l'agent (mode Code) : boucle outils + approbations, même événements `chat:*` que le chat.
#[tauri::command]
pub(crate) async fn agent_start(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    request_id: String,
    model: String,
    messages: Vec<ChatMessage>,
    options: Option<serde_json::Value>,
    workspace: Option<String>,
    mode: String,
    web: bool,
    cloud_key: String,
    profile: Option<String>,
    attachments: Option<Vec<AttachmentIn>>,
    docs: Option<Vec<AttachmentIn>>,
    project_id: Option<String>,
) -> Result<(), String> {
    track(&app, "agent_started", serde_json::json!({ "mode": mode, "model": model }));
    let attachments: Vec<(String, String)> = attachments.unwrap_or_default().into_iter().map(|a| (a.name, a.content)).collect();
    let docs: Vec<(String, String)> = docs.unwrap_or_default().into_iter().map(|a| (a.name, a.content)).collect();
    let project_id = project_id.filter(|p| !p.trim().is_empty());
    let workspace = workspace.filter(|w| !w.trim().is_empty());
    if let Some(w) = &workspace {
        if !std::path::Path::new(w).is_dir() {
            return Err(format!("Dossier de travail introuvable : {w}"));
        }
    }
    let workspace: Option<std::path::PathBuf> = workspace.map(Into::into);
    let all_agents = agents::scan(&agents_dir(&app)?, workspace.as_deref());
    let profile: Option<agents::AgentProfile> = match profile.as_deref().filter(|n| !n.trim().is_empty()) {
        Some(name) => Some(agents::find(&all_agents, name).cloned().ok_or_else(|| format!("Agent introuvable : {name}"))?),
        None => None,
    };
    let mode = agents::effective_mode(&mode, profile.as_ref());
    let mut skills: Vec<skills::Skill> = scan_skills(&app, workspace.as_deref())?.into_iter().filter(|s| s.enabled).collect();
    if let Some(names) = profile.as_ref().and_then(|p| p.skills.as_ref()) {
        skills.retain(|s| names.iter().any(|n| n.eq_ignore_ascii_case(&s.name)));
    }
    let running = state.running.clone();
    let ollama = client_for_model(&app, &cloud_key, &model).await.map_err(|e| e.to_string())?;
    let agent_state = state.agent_of(window.label());
    let hub = state.mcp.lock().await.clone();
    let rid = request_id.clone();
    let model_name = model.clone();
    let handle = tokio::spawn(async move {
        let params = agent::RunParams {
            request_id: rid.clone(),
            model,
            messages,
            workspace,
            mode,
            options,
            skills,
            web,
            profile,
            agents: all_agents,
            depth: 0,
            attachments,
            docs,
            project_id,
        };
        if let Err(e) = agent::run(app.clone(), ollama, agent_state, hub, params).await {
            track_error(&app, &e.to_string(), &model_name);
            let _ = app.emit("chat:error", serde_json::json!({ "id": rid, "error": e.to_string() }));
        }
        running.lock().await.remove(&rid);
    });
    state.running.lock().await.insert(request_id, handle.abort_handle());
    Ok(())
}

/// Ajoute une précision à la tâche en cours : l'agent la lit à sa prochaine étape, sans rien interrompre.
/// Renvoie false si aucune tâche ne tourne (l'interface renverra alors le message normalement).
#[tauri::command]
pub(crate) async fn agent_inject(window: tauri::Window, state: State<'_, AppState>, request_id: String, text: String) -> Result<bool, String> {
    if text.trim().is_empty() || !state.running.lock().await.contains_key(&request_id) {
        return Ok(false);
    }
    state.agent_of(window.label()).inbox.lock_safe().entry(request_id).or_default().push(text.trim().to_string());
    Ok(true)
}

/// Modifications faites par l'agent pendant la session, avec leurs différences.
#[tauri::command]
pub(crate) async fn agent_changes(window: tauri::Window, state: State<'_, AppState>) -> Result<Vec<agent::ChangeInfo>, String> {
    Ok(state.agent_of(window.label()).changes_report().await)
}

/// Réponse complète sans streaming (résumés de compactage).
#[tauri::command]
pub(crate) async fn chat_once(
    app: AppHandle,
    model: String,
    messages: Vec<ChatMessage>,
    options: Option<serde_json::Value>,
    cloud_key: String,
) -> Result<String, OllamaError> {
    let mut out = String::new();
    client_for_model(&app, &cloud_key, &model).await?.chat_stream(&model, &messages, None, options, |c| out.push_str(&c.content)).await?;
    Ok(out)
}

#[tauri::command]
pub(crate) async fn agent_approve(state: State<'_, AppState>, call_id: String, allow: bool) -> Result<(), String> {
    for agent in state.all_agents() {
        if let Some(tx) = agent.approvals.lock().await.remove(&call_id) {
            let _ = tx.send(allow);
            break;
        }
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn agent_undo(window: tauri::Window, state: State<'_, AppState>) -> Result<String, String> {
    state.agent_of(window.label()).undo_last().await
}

#[tauri::command]
pub(crate) async fn model_show(name: String, cloud_key: String) -> Result<serde_json::Value, OllamaError> {
    client_for(&cloud_key)?.show_model(&name).await
}

#[tauri::command]
pub(crate) async fn chat_cancel(state: State<'_, AppState>, request_id: String) -> Result<(), String> {
    if let Some(h) = state.running.lock().await.remove(&request_id) {
        h.abort();
    }
    Ok(())
}
