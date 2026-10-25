mod agent;
mod sync;
mod agents;
mod git;
mod history;
mod journal;
mod mcp;
mod profiles;
mod connectors;
mod docs;
mod export;
mod pdf;
mod docsearch;
mod backup;
mod browser;
mod vault;
mod winio;
mod reminders;
mod schedules;
mod appsettings;
mod secrets;
mod system;
mod hardware;
mod localmodels;
mod localai;
mod templates;
mod ollama;
mod skills;
mod skills_hub;
mod web;
// L'impression PDF utilise des appels Windows (comctl32 v6) que l'exécutable de test, sans manifeste, ne peut pas charger.
#[cfg(not(test))]
mod webprint;
#[cfg(test)]
mod webprint {
    pub fn html_to_pdf(_app: &tauri::AppHandle, _html: &str, _out: &std::path::Path, _header: &str, _footer: &str) -> Result<(), String> {
        Err("Moteur de rendu indisponible dans les tests.".into())
    }
}

use crate::sync::MutexExt;
use std::{collections::HashMap, sync::Arc};

use history::{Conversation, History, Memory, Project, ProjectDoc, StoredMessage};
use ollama::{ChatMessage, ModelInfo, OllamaClient, OllamaError};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex;
use tokio::task::AbortHandle;

/// Résultat du dernier scan des skills, réutilisé quelques secondes (l'agent et le panneau Session le demandent souvent).
struct SkillsCache {
    workspace: Option<std::path::PathBuf>,
    at: std::time::Instant,
    list: Vec<skills::Skill>,
}

const SKILLS_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Default)]
struct AppState {
    /// Profil ouvert : tous ses fichiers sont dans `<données>/profiles/<id>/`.
    profile: std::sync::Mutex<Option<String>>,
    /// Codes PIN ratés par profil depuis le lancement : chaque échec rallonge l'attente avant la réponse.
    pin_failures: std::sync::Mutex<HashMap<String, u64>>,
    skills_cache: std::sync::Mutex<Option<SkillsCache>>,
    /// Générations en cours, indexées par id de requête, pour pouvoir les annuler.
    running: Arc<Mutex<HashMap<String, AbortHandle>>>,
    /// État de l'agent (annulations, autorisations, commandes en arrière-plan) de chaque fenêtre : plusieurs fenêtres travaillent en parallèle sans se gêner.
    agents: std::sync::Mutex<HashMap<String, Arc<agent::AgentState>>>,
    mcp: Arc<Mutex<Arc<mcp::McpHub>>>,
    /// Ce que fait chaque fenêtre (rapporté par la fenêtre elle-même) : alimente le tableau des agents en parallèle.
    board: std::sync::Mutex<HashMap<String, BoardEntry>>,
    /// Tâche à démarrer dans une fenêtre qui vient de s'ouvrir.
    tasks: std::sync::Mutex<HashMap<String, TaskSpec>>,
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Default)]
struct BoardEntry {
    #[serde(default)]
    label: String,
    title: String,
    workspace: String,
    model: String,
    /// running, waiting (attend une autorisation), done, interrupted, idle, error
    state: String,
    request_id: Option<String>,
    done: u32,
    total: u32,
    last: String,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct TaskSpec {
    prompt: String,
    workspace: Option<String>,
    mode: String,
}

impl AppState {
    /// État de l'agent de la fenêtre `label` (créé au premier besoin).
    fn agent_of(&self, label: &str) -> Arc<agent::AgentState> {
        self.agents.lock_safe().entry(label.to_string()).or_default().clone()
    }

    fn all_agents(&self) -> Vec<Arc<agent::AgentState>> {
        self.agents.lock_safe().values().cloned().collect()
    }
}

/// Client de l'API Ollama Cloud : une clé API est obligatoire.
fn client_for(api_key: &str) -> Result<OllamaClient, OllamaError> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err(OllamaError::Status(401, "Aucune clé API : ajoute ta clé Ollama dans les Réglages".into()));
    }
    OllamaClient::cloud(key).map_err(|e| OllamaError::Status(400, e))
}

/// Client pour un modèle : « local:<id> » passe par le moteur installé sur le PC (lancé au besoin), le reste par Ollama Cloud.
async fn client_for_model(app: &AppHandle, api_key: &str, model: &str) -> Result<OllamaClient, OllamaError> {
    let Some(id) = model.strip_prefix("local:") else { return client_for(api_key) };
    let data = app_root(app).map_err(|e| OllamaError::Status(500, e))?;
    let model = localmodels::catalog().into_iter().find(|m| m.id == id).ok_or_else(|| OllamaError::Status(404, format!("Modèle local inconnu : {id}")))?;
    let ctx = localai::effective_ctx(&data, &model, &hardware::cached().await);
    let (port, key) = localai::ensure_running(&data, id, ctx).await.map_err(|e| OllamaError::Status(500, e))?;
    localai::touch().await;
    OllamaClient::local(port, &key).map_err(|e| OllamaError::Status(500, e))
}

/// Modèles locaux installés sur ce PC.
#[tauri::command]
async fn local_installed(app: AppHandle) -> Result<Vec<localai::Installed>, String> {
    Ok(localai::installed(&app_root(&app)?, &hardware::cached().await))
}

/// Taille du contexte d'un modèle local (0 = automatique) ; appliquée au prochain message.
#[tauri::command]
fn local_set_ctx(app: AppHandle, id: String, ctx: u32) -> Result<(), String> {
    localai::set_ctx_setting(&app_root(&app)?, &id, ctx)
}

#[derive(serde::Serialize)]
struct LocalStatus {
    running: Option<String>,
    installing: Option<String>,
    engine_bytes: u64,
}

#[tauri::command]
async fn local_status(app: AppHandle) -> Result<LocalStatus, String> {
    Ok(LocalStatus { running: localai::running_model().await, installing: localai::installing(), engine_bytes: localai::engine_bytes(&app_root(&app)?) })
}

/// Lance l'installation d'un modèle du catalogue : la progression arrive par `local:progress`, la fin par `local:done` ou `local:error`.
#[tauri::command]
async fn local_install(app: AppHandle, id: String) -> Result<(), String> {
    let model = localmodels::catalog().into_iter().find(|m| m.id == id).ok_or("Modèle inconnu.")?;
    if let Some(other) = localai::installing() {
        return Err(format!("Une installation est déjà en cours ({other})."));
    }
    let data = app_root(&app)?;
    tokio::spawn(async move {
        let hw = tokio::task::spawn_blocking(hardware::detect).await.ok();
        let outcome = match hw {
            Some(hw) => {
                let a = app.clone();
                localai::install(&data, &model, &hw, std::sync::Arc::new(move |p| { let _ = a.emit("local:progress", p); })).await
            }
            None => Err("Analyse du PC impossible.".to_string()),
        };
        match outcome {
            Ok(()) => { let _ = app.emit("local:done", serde_json::json!({ "id": model.id })); }
            Err(e) => { let _ = app.emit("local:error", serde_json::json!({ "id": model.id, "error": e })); }
        }
    });
    Ok(())
}

#[tauri::command]
fn local_cancel(id: String) {
    localai::cancel(&id);
}

#[tauri::command]
async fn local_remove(app: AppHandle, id: String) -> Result<(), String> {
    localai::remove(&app_root(&app)?, &id).await
}

/// Catalogue des modèles d'Ollama Cloud. La liste est publique : elle ne prouve pas que la clé est valide.
#[tauri::command]
async fn cloud_models(key: String) -> Result<Vec<ModelInfo>, OllamaError> {
    client_for(&key)?.list_models().await
}

/// Vérifie la clé avec un tout petit message (quelques jetons) : seul un appel de chat est authentifié.
#[tauri::command]
async fn cloud_test(key: String, model: String) -> Result<String, OllamaError> {
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
async fn chat_start(
    app: AppHandle,
    state: State<'_, AppState>,
    request_id: String,
    model: String,
    messages: Vec<ChatMessage>,
    options: Option<serde_json::Value>,
    cloud_key: String,
) -> Result<(), String> {
    let running = state.running.clone();
    let rid = request_id.clone();
    let handle = tokio::spawn(async move {
        let emit_app = app.clone();
        let rid2 = rid.clone();
        let client = match client_for_model(&app, &cloud_key, &model).await {
            Ok(c) => c,
            Err(e) => {
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
            let _ = app.emit("chat:error", serde_json::json!({ "id": rid, "error": e.to_string() }));
        }
        running.lock().await.remove(&rid);
    });
    state.running.lock().await.insert(request_id, handle.abort_handle());
    Ok(())
}

/// Fichier joint transmis à l'agent pour son outil `read_attachment`.
#[derive(serde::Deserialize)]
struct AttachmentIn {
    name: String,
    content: String,
}

/// Lance l'agent (mode Code) : boucle outils + approbations, même événements `chat:*` que le chat.
#[tauri::command]
async fn agent_start(
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
async fn agent_inject(window: tauri::Window, state: State<'_, AppState>, request_id: String, text: String) -> Result<bool, String> {
    if text.trim().is_empty() || !state.running.lock().await.contains_key(&request_id) {
        return Ok(false);
    }
    state.agent_of(window.label()).inbox.lock_safe().entry(request_id).or_default().push(text.trim().to_string());
    Ok(true)
}

fn app_root(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

/// Dossier de données du profil ouvert.
fn profile_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let id = app.state::<AppState>().profile.lock().map_err(|e| e.to_string())?.clone().ok_or("Aucun profil ouvert.")?;
    Ok(profiles::data_dir(&app_root(app)?, &id))
}

fn active_profile(app: &AppHandle) -> Result<String, String> {
    app.state::<AppState>().profile.lock().map_err(|e| e.to_string())?.clone().ok_or_else(|| "Aucun profil ouvert.".to_string())
}

/// Clé API du profil ouvert, depuis le coffre Windows.
#[tauri::command]
fn secret_get(app: AppHandle) -> Result<Option<String>, String> {
    secrets::get(&active_profile(&app)?, "api-key")
}

/// Enregistre (ou, si vide, supprime) la clé API du profil ouvert dans le coffre Windows.
#[tauri::command]
fn secret_set(app: AppHandle, value: String) -> Result<(), String> {
    secrets::set(&active_profile(&app)?, "api-key", value.trim())
}

#[tauri::command]
fn profiles_list(app: AppHandle) -> Result<Vec<profiles::PublicProfile>, String> {
    Ok(profiles::list(&app_root(&app)?).map_err(|e| e.to_string())?.iter().map(Into::into).collect())
}

#[tauri::command]
fn profile_create(app: AppHandle, name: String, color: String) -> Result<profiles::PublicProfile, String> {
    Ok((&profiles::create(&app_root(&app)?, &name, &color)?).into())
}

/// Vérifie un code PIN ; chaque échec double presque l'attente suivante (1 s, 2 s… jusqu'à 5 s).
async fn check_pin(app: &AppHandle, id: &str, pin: Option<&str>) -> Result<(), String> {
    let result = profiles::verify(&app_root(app)?, id, pin);
    let state = app.state::<AppState>();
    let wait = {
        let mut failures = state.pin_failures.lock().map_err(|e| e.to_string())?;
        if result.is_ok() {
            failures.remove(id);
            0
        } else {
            let n = failures.entry(id.to_string()).or_insert(0);
            *n += 1;
            (*n).min(5)
        }
    };
    if wait > 0 {
        tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
    }
    result
}

#[tauri::command]
async fn profile_update(app: AppHandle, id: String, name: String, color: String, pin: Option<String>) -> Result<profiles::PublicProfile, String> {
    check_pin(&app, &id, pin.as_deref()).await?;
    Ok((&profiles::update(&app_root(&app)?, &id, &name, &color, pin.as_deref())?).into())
}

/// Définit, change (`new_pin`) ou retire (sans `new_pin`) le code d'un profil ; l'ancien code est exigé s'il y en a un.
#[tauri::command]
async fn profile_set_pin(app: AppHandle, id: String, new_pin: Option<String>, current_pin: Option<String>) -> Result<(), String> {
    check_pin(&app, &id, current_pin.as_deref()).await?;
    profiles::set_pin(&app_root(&app)?, &id, new_pin.as_deref().filter(|p| !p.trim().is_empty()), current_pin.as_deref())
}

/// Supprime un profil : ses données vont à la corbeille. Le profil ouvert et le dernier profil sont protégés.
#[tauri::command]
async fn profile_delete(app: AppHandle, state: State<'_, AppState>, id: String, pin: Option<String>) -> Result<(), String> {
    if state.profile.lock().map_err(|e| e.to_string())?.as_deref() == Some(id.as_str()) {
        return Err("Ferme ce profil avant de le supprimer.".into());
    }
    check_pin(&app, &id, pin.as_deref()).await?;
    profiles::remove(&app_root(&app)?, &id, pin.as_deref(), |dir| agent::fsops::to_trash(dir))?;
    // La clé du profil supprimé ne doit pas rester dans le coffre.
    let _ = secrets::set(&id, "api-key", "");
    Ok(())
}

#[tauri::command]
fn journal_list(limit: Option<usize>) -> Vec<journal::Entry> {
    journal::list(limit.unwrap_or(100).min(500))
}

#[tauri::command]
fn journal_clear() {
    journal::clear();
}

/// Dernière capture d'écran prise par l'agent (pour montrer où il va cliquer).
#[tauri::command]
fn last_shot() -> Option<system::Shot> {
    system::last_shot()
}

/// Arrêt d'urgence : coupe tous les agents, les commandes en arrière-plan et les demandes en attente, dans toutes les fenêtres.
fn emergency_stop(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        for (_, handle) in state.running.lock().await.drain() {
            handle.abort();
        }
        for agent in state.all_agents() {
            agent.background.stop_all();
            // Les demandes d'autorisation en attente sont refusées (l'expéditeur supprimé vaut « non »).
            agent.approvals.lock().await.clear();
        }
        let _ = app.emit("emergency:stop", ());
        use tauri_plugin_notification::NotificationExt;
        let _ = app.notification().builder().title("Lama Desktop").body("Arrêt d'urgence : tout est arrêté.").show();
    });
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[derive(serde::Serialize)]
struct AppInfo {
    keep_background: bool,
    autostart: bool,
    shortcut: &'static str,
    shortcut_ok: bool,
}

static SHORTCUT_OK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn app_info(app: &AppHandle) -> AppInfo {
    AppInfo {
        keep_background: appsettings::load(&app_root(app).unwrap_or_default()).keep_background,
        autostart: appsettings::autostart_enabled(),
        shortcut: "Ctrl+Alt+S",
        shortcut_ok: SHORTCUT_OK.load(std::sync::atomic::Ordering::Relaxed),
    }
}

#[tauri::command]
fn app_settings_get(app: AppHandle) -> AppInfo {
    app_info(&app)
}

#[tauri::command]
fn app_settings_set(app: AppHandle, keep_background: bool, autostart: bool) -> Result<AppInfo, String> {
    let root = app_root(&app)?;
    appsettings::save(&root, &appsettings::Settings { keep_background })?;
    appsettings::set_autostart(autostart)?;
    Ok(app_info(&app))
}

/// Profil actuellement ouvert (une nouvelle fenêtre l'adopte sans redemander le code PIN).
#[tauri::command]
fn profile_active(app: AppHandle) -> Option<String> {
    active_profile(&app).ok()
}

/// Ouvre une nouvelle fenêtre sur le même profil : chaque fenêtre a sa conversation, son dossier de travail et son agent.
#[tauri::command]
async fn window_new(app: AppHandle, state: State<'_, AppState>, task: Option<TaskSpec>) -> Result<String, String> {
    active_profile(&app)?;
    if let Some(t) = &task {
        if t.prompt.trim().is_empty() {
            return Err("Décris la tâche à lancer.".into());
        }
        if let Some(w) = t.workspace.as_deref().filter(|w| !w.trim().is_empty()) {
            if !std::path::Path::new(w).is_dir() {
                return Err(format!("Dossier de travail introuvable : {w}"));
            }
        }
    }
    let label = format!("win-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    if let Some(t) = task {
        state.tasks.lock_safe().insert(label.clone(), t);
    }
    tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::App("index.html".into()))
        .title("Lama Desktop")
        .inner_size(1180.0, 780.0)
        .min_inner_size(900.0, 560.0)
        .decorations(false)
        .shadow(true)
        .build()
        .map_err(|e| format!("Fenêtre impossible : {e}"))?;
    Ok(label)
}

/// Tâche à démarrer dans cette fenêtre (lancée depuis le tableau des agents), une seule fois.
#[tauri::command]
fn window_task(window: tauri::Window, state: State<'_, AppState>) -> Option<TaskSpec> {
    state.tasks.lock_safe().remove(window.label())
}

/// Chaque fenêtre dit ce qu'elle fait ; le tableau des agents les affiche toutes.
#[tauri::command]
fn board_report(window: tauri::Window, state: State<'_, AppState>, entry: BoardEntry) {
    let mut e = entry;
    e.label = window.label().to_string();
    state.board.lock_safe().insert(e.label.clone(), e);
}

#[tauri::command]
fn board_list(app: AppHandle, state: State<'_, AppState>) -> Vec<BoardEntry> {
    // Les fenêtres d'impression PDF (cachées, éphémères) n'ont rien à faire dans le tableau.
    let open: HashMap<String, tauri::WebviewWindow> = app.webview_windows().into_iter().filter(|(l, _)| !l.starts_with("pdf-")).collect();
    let mut board = state.board.lock_safe();
    board.retain(|label, _| open.contains_key(label));
    let mut out: Vec<BoardEntry> = open
        .keys()
        .map(|label| {
            board.get(label).cloned().unwrap_or_else(|| BoardEntry { label: label.clone(), state: "idle".into(), ..Default::default() })
        })
        .collect();
    // La fenêtre principale d'abord, puis par ordre alphabétique des étiquettes.
    out.sort_by(|a, b| (a.label != "main", &a.label).cmp(&(b.label != "main", &b.label)));
    out
}

#[tauri::command]
fn board_focus(app: AppHandle, label: String) -> Result<(), String> {
    let w = app.get_webview_window(&label).ok_or("Fenêtre introuvable.")?;
    let _ = w.unminimize();
    let _ = w.show();
    w.set_focus().map_err(|e| e.to_string())
}

/// Ouvre un profil : bascule l'historique, les connecteurs MCP, les skills et les agents sur ses données.
#[tauri::command]
async fn profile_select(app: AppHandle, state: State<'_, AppState>, db: Db<'_>, id: String, pin: Option<String>) -> Result<(), String> {
    check_pin(&app, &id, pin.as_deref()).await?;
    if app.webview_windows().keys().filter(|l| !l.starts_with("pdf-")).count() > 1 {
        return Err("Ferme les autres fenêtres de Lama Desktop avant de changer de profil.".into());
    }
    if !state.running.lock().await.is_empty() {
        return Err("Une génération est en cours : arrête-la avant de changer de profil.".into());
    }
    let root = app_root(&app)?;
    profiles::find(&root, &id)?;
    let dir = profiles::data_dir(&root, &id);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let history = History::open(&dir.join("history.db")).map_err(db_err)?;
    *db.lock().map_err(|e| e.to_string())? = history;
    *state.profile.lock().map_err(|e| e.to_string())? = Some(id.clone());
    invalidate_skills(&app);
    for agent in state.all_agents() {
        agent.changes.lock().await.clear();
        agent.approvals.lock().await.clear();
        agent.background.stop_all();
        agent.background.clear_finished();
    }
    *state.mcp.lock().await = Arc::new(mcp::McpHub::default());
    apply_pc(&load_pc(&dir));
    reminders::set_file(Some(dir.join("reminders.json")));
    schedules::set_file(Some(dir.join("schedules.json")));
    // Le coffre du profil précédent se referme : on ne garde jamais une clé d'un autre profil.
    vault::global().set_dir(Some(&dir));
    // Le navigateur de Lama appartient au profil : on ferme celui du profil précédent.
    browser::shutdown().await;
    browser::set_dir(Some(&dir));
    journal::set_file(Some(dir.join("actions.jsonl")));
    // Les connecteurs démarrent en arrière-plan : un `npx` ou `uvx` peut mettre une minute, l'ouverture du profil ne les attend pas.
    let (handle, config) = (app.clone(), dir.join("mcp.json"));
    tauri::async_runtime::spawn(async move {
        let hub = Arc::new(mcp::McpHub::start(&config, &id).await);
        let state = handle.state::<AppState>();
        // Si l'utilisateur a changé de profil entre-temps, ces connecteurs ne sont plus les bons.
        let still_open = state.profile.lock().ok().and_then(|p| p.clone()).as_deref() == Some(id.as_str());
        if still_open {
            *state.mcp.lock().await = hub;
            let _ = handle.emit("mcp:ready", ());
        }
    });
    Ok(())
}

/// Réglages « Ordinateur » du profil : outils système activés et dossiers autorisés en plus du dossier de travail.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct PcSettings {
    enabled: bool,
    dirs: Vec<String>,
}

impl Default for PcSettings {
    fn default() -> Self {
        Self { enabled: true, dirs: vec![] }
    }
}

fn load_pc(dir: &std::path::Path) -> PcSettings {
    std::fs::read_to_string(dir.join("pc.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn apply_pc(s: &PcSettings) {
    system::set_enabled(s.enabled);
    agent::sandbox::set_extra_roots(&s.dirs);
}

/// Montre un fichier dans l'Explorateur Windows.
#[tauri::command]
fn reveal_file(path: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err("Fichier introuvable.".into());
    }
    std::process::Command::new("explorer").arg(format!("/select,{}", p.display())).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Enregistre des octets (image PNG exportée, etc.) donnés en base64.
#[tauri::command]
fn save_bytes(path: String, base64: String) -> Result<(), String> {
    use base64::Engine;
    if path.trim().is_empty() {
        return Err("Chemin vide.".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD.decode(base64.trim()).map_err(|e| format!("Données invalides : {e}"))?;
    std::fs::write(&path, bytes).map_err(|e| format!("Écriture impossible : {e}"))
}

/// Modifications faites par l'agent pendant la session, avec leurs différences.
#[tauri::command]
async fn agent_changes(window: tauri::Window, state: State<'_, AppState>) -> Result<Vec<agent::ChangeInfo>, String> {
    Ok(state.agent_of(window.label()).changes_report().await)
}

/// Tous les changements du dépôt, prêts à être relus par le modèle.
#[tauri::command]
fn git_review_diff(workspace: String) -> Result<String, String> {
    git::review_diff(std::path::Path::new(&workspace))
}

#[tauri::command]
fn git_branch_create(workspace: String, name: String) -> Result<String, String> {
    git::branch_create(std::path::Path::new(&workspace), &name)
}

#[tauri::command]
fn project_templates() -> Vec<templates::TemplateInfo> {
    templates::list()
}

#[tauri::command]
fn project_from_template(id: String, dir: String) -> Result<usize, String> {
    templates::create(&id, std::path::Path::new(&dir))
}

/// Règles du projet : le fichier AGENTS.md à la racine, lu par l'agent à chaque tâche.
#[tauri::command]
fn project_rules_get(workspace: String) -> Result<String, String> {
    let p = agent::sandbox::resolve(std::path::Path::new(&workspace), "AGENTS.md")?;
    Ok(std::fs::read_to_string(p).unwrap_or_default())
}

#[tauri::command]
fn project_rules_set(workspace: String, text: String) -> Result<(), String> {
    let p = agent::sandbox::resolve(std::path::Path::new(&workspace), "AGENTS.md")?;
    std::fs::write(p, text).map_err(|e| format!("Écriture impossible : {e}"))
}

/// Écrit un document (PDF, Word, HTML, Markdown selon l'extension) à partir de texte Markdown.
#[tauri::command]
async fn export_document(path: String, title: String, markdown: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || export::write(std::path::Path::new(&path), &title, &markdown)).await.map_err(|e| e.to_string())?
}

/// PDF d'une page HTML (artefact) : les scripts sont désactivés, la page est imprimée telle qu'elle s'affiche.
#[tauri::command]
async fn export_html_pdf(path: String, title: String, html: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let page = docs::static_page(&html);
        export::html_to_pdf(std::path::Path::new(&path), &title, &page)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Dictée vocale de Windows (Win + H) : la saisie arrive dans le champ qui a le focus.
#[tauri::command]
async fn voice_dictation() -> Result<(), String> {
    tokio::task::spawn_blocking(|| system::computer("key", None, None, None, Some("win+h"), None).map(|_| ()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn reminders_list() -> Result<Vec<reminders::Reminder>, String> {
    reminders::list()
}

#[tauri::command]
fn reminders_cancel(id: String) -> Result<bool, String> {
    reminders::cancel(&id)
}

/// Notification Windows, sauf si une fenêtre de l'application est au premier plan (l'utilisateur voit déjà le résultat).
#[tauri::command]
fn notify_os(app: AppHandle, title: String, body: String) -> bool {
    use tauri_plugin_notification::NotificationExt;
    let focused = app.webview_windows().values().any(|w| w.is_focused().unwrap_or(false) && !w.is_minimized().unwrap_or(false));
    if focused {
        return false;
    }
    app.notification().builder().title(title).body(body.chars().take(140).collect::<String>()).show().is_ok()
}

// ---------------------------------------------------------------- coffre (mots de passe et infos rapides)

#[tauri::command]
fn vault_status() -> vault::Status {
    vault::global().status()
}

#[tauri::command]
async fn vault_create(master: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || vault::global().create(&master)).await.map_err(|e| e.to_string())?
}

/// Ouvre le coffre. Chaque échec rallonge l'attente suivante (jusqu'à 5 s) : deviner le mot de passe à la main prend des heures.
#[tauri::command]
async fn vault_unlock(master: String) -> Result<(), String> {
    let r = tokio::task::spawn_blocking(move || vault::global().unlock(&master)).await.map_err(|e| e.to_string())?;
    if r.is_err() {
        tokio::time::sleep(std::time::Duration::from_secs(vault::global().failures().min(5) as u64)).await;
    }
    r
}

#[tauri::command]
fn vault_lock() {
    vault::global().lock();
}

#[tauri::command]
fn vault_list() -> Result<serde_json::Value, String> {
    let (logins, infos) = vault::global().list()?;
    Ok(serde_json::json!({ "logins": logins, "infos": infos }))
}

#[tauri::command]
fn vault_login_get(id: String) -> Result<vault::Login, String> {
    vault::global().get_login(&id)
}

#[tauri::command]
fn vault_login_save(login: vault::Login) -> Result<vault::Login, String> {
    vault::global().save_login(login)
}

#[tauri::command]
fn vault_login_delete(id: String) -> Result<(), String> {
    vault::global().delete_login(&id)
}

#[tauri::command]
fn vault_info_save(info: vault::Info) -> Result<vault::Info, String> {
    vault::global().save_info(info)
}

#[tauri::command]
fn vault_info_delete(id: String) -> Result<(), String> {
    vault::global().delete_info(&id)
}

#[tauri::command]
fn vault_generate(opts: vault::GenOpts) -> Result<serde_json::Value, String> {
    let (password, bits) = vault::generate(&opts)?;
    Ok(serde_json::json!({ "password": password, "bits": bits }))
}

#[tauri::command]
fn vault_set_agent_access(on: bool) -> Result<(), String> {
    vault::global().set_agent_access(on)
}

#[tauri::command]
async fn vault_change_master(old: String, new: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || vault::global().change_master(&old, &new)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn vault_destroy(master: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || vault::global().destroy(&master)).await.map_err(|e| e.to_string())?
}

/// Copie un secret ; le presse-papiers est vidé au bout de 30 s s'il le contient encore. Le secret ne revient jamais à l'interface.
#[tauri::command]
async fn vault_copy(id: String, field: String) -> Result<u64, String> {
    const SECONDS: u64 = 30;
    let secret = vault::global().secret(&id, &field)?;
    winio::copy_secret(&secret)?;
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(SECONDS)).await;
        winio::clear_clipboard_if(&secret);
    });
    Ok(SECONDS)
}

/// Saisie demandée par l'utilisateur lui-même (bouton « Remplir ») : `{ items: [{ kind, id }], wait, submit, tab }`.
#[tauri::command]
async fn vault_fill_ui(app: AppHandle, resolved: serde_json::Value) -> Result<String, String> {
    vault::fill_flow(&app, &resolved).await
}

// ---------------------------------------------------------------- navigateur de Lama

#[tauri::command]
async fn browser_status() -> serde_json::Value {
    let found = browser::find_browser();
    serde_json::json!({
        "available": found.is_ok(),
        "name": found.as_ref().map(|(_, n)| *n).unwrap_or(""),
        "running": browser::is_running().await.is_some(),
    })
}

/// Ouvre le navigateur de Lama (pour s'y connecter à la main : double authentification, CAPTCHA) sur une adresse facultative.
#[tauri::command]
async fn browser_launch_ui(url: Option<String>) -> Result<String, String> {
    browser::open(url.as_deref().filter(|u| !u.trim().is_empty()).unwrap_or("about:blank").replace("about:blank", "https://duckduckgo.com/").as_str()).await
}

#[tauri::command]
async fn browser_close_ui() -> Result<String, String> {
    browser::close().await
}

/// Efface les cookies, sessions et le cache du navigateur de Lama (déconnecte de tous les sites).
#[tauri::command]
async fn browser_wipe(app: AppHandle) -> Result<(), String> {
    browser::shutdown().await;
    let dir = profile_dir(&app)?.join("browser");
    tokio::task::spawn_blocking(move || if dir.exists() { std::fs::remove_dir_all(dir).map_err(|e| format!("Suppression impossible : {e}")) } else { Ok(()) })
        .await
        .map_err(|e| e.to_string())?
}

/// « Se connecter » depuis le coffre : ouvre le site de l'entrée dans le navigateur de Lama, puis y saisit identifiant et mot de passe.
#[tauri::command]
async fn vault_browser_login(id: String, submit: bool) -> Result<String, String> {
    let login = vault::global().get_login(&id)?;
    if login.url.trim().is_empty() {
        return Err("Cette entrée n'a pas d'adresse de site : ajoute-la pour te connecter automatiquement.".into());
    }
    let url = if login.url.contains("://") { login.url.clone() } else { format!("https://{}", login.url) };
    browser::open(&url).await?;
    vault::browser_login(&serde_json::json!({ "id": id, "mode": "login", "submit": submit, "label": login.title })).await
}

#[tauri::command]
fn schedules_list() -> Result<Vec<schedules::Task>, String> {
    schedules::list()
}

#[tauri::command]
fn schedule_save(task: schedules::Task) -> Result<schedules::Task, String> {
    schedules::save(task)
}

#[tauri::command]
fn schedule_delete(id: String) -> Result<(), String> {
    schedules::delete(&id)
}

#[tauri::command]
fn schedule_done(id: String, next_at: Option<u64>, conv: Option<String>) -> Result<(), String> {
    schedules::done(&id, next_at, conv)
}

/// Sauvegarde du profil ouvert (conversations, projets, mémoire, réglages, skills, connecteurs) dans une archive .zip.
/// Les secrets restent dans le coffre Windows : ils ne sont pas inclus.
#[tauri::command]
async fn data_backup(app: AppHandle, db: Db<'_>, path: String) -> Result<usize, String> {
    let dir = profile_dir(&app)?;
    let id = active_profile(&app)?;
    let name = profiles::find(&app_root(&app)?, &id).map(|p| p.name).unwrap_or_default();
    let copy = std::env::temp_dir().join(format!("lama-copie-{}.db", uuid::Uuid::new_v4()));
    db.lock().map_err(|e| e.to_string())?.backup_to(&copy).map_err(db_err)?;
    let (out, tmp) = (std::path::PathBuf::from(path), copy.clone());
    let result = tokio::task::spawn_blocking(move || backup::backup(&dir, &tmp, &out, &name)).await.map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(copy);
    result
}

/// Serveurs MCP que la sauvegarde ajouterait (à montrer à l'utilisateur avant de les restaurer).
#[tauri::command]
async fn data_restore_preview(path: String) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || backup::mcp_in_archive(std::path::Path::new(&path))).await.map_err(|e| e.to_string())?
}

/// Restaure une sauvegarde dans le profil ouvert : ses données actuelles sont remplacées (l'ancienne base est gardée à côté).
#[tauri::command]
async fn data_restore(app: AppHandle, state: State<'_, AppState>, db: Db<'_>, path: String, with_mcp: Option<bool>) -> Result<usize, String> {
    if !state.running.lock().await.is_empty() {
        return Err("Une génération est en cours : arrête-la avant de restaurer.".into());
    }
    let dir = profile_dir(&app)?;
    // La base ouverte est refermée avant d'être remplacée.
    *db.lock().map_err(|e| e.to_string())? = History::in_memory().map_err(db_err)?;
    let target = dir.clone();
    let count = tokio::task::spawn_blocking(move || backup::restore(std::path::Path::new(&path), &target, with_mcp.unwrap_or(false))).await.map_err(|e| e.to_string());
    let reopened = History::open(&dir.join("history.db")).map_err(db_err)?;
    *db.lock().map_err(|e| e.to_string())? = reopened;
    count?
}

/// Toutes les conversations en fichiers Markdown, rangés par projet, dans une archive .zip.
#[tauri::command]
async fn data_export_chats(db: Db<'_>, path: String) -> Result<usize, String> {
    let entries: Vec<(String, String, String)> = {
        let h = db.lock().map_err(|e| e.to_string())?;
        let projects = h.list_projects().map_err(db_err)?;
        let mut out = Vec::new();
        for kind in ["chat", "code"] {
            for c in h.list("", kind, None).map_err(db_err)? {
                let folder = match c.project_id.as_deref().and_then(|id| projects.iter().find(|p| p.id == id)) {
                    Some(p) => p.name.clone(),
                    None if kind == "code" => "Code".to_string(),
                    None => "Sans projet".to_string(),
                };
                let msgs = h.messages(&c.id).map_err(db_err)?;
                out.push((folder, c.title.clone(), backup::conversation_markdown(&c.title, &c.model, &msgs)));
            }
        }
        out
    };
    tokio::task::spawn_blocking(move || backup::write_chats(std::path::Path::new(&path), &entries)).await.map_err(|e| e.to_string())?
}

/// Efface toutes les données du logiciel : profils, conversations, projets, mémoire, réglages, clés du coffre Windows,
/// lancement avec Windows. Les fichiers créés par l'agent (Documents\Lama Desktop) ne sont pas touchés. L'application redémarre.
#[tauri::command]
async fn data_erase(app: AppHandle, state: State<'_, AppState>, db: Db<'_>, confirm: String) -> Result<(), String> {
    if confirm.trim() != "EFFACER" {
        return Err("Confirmation incorrecte : tape EFFACER.".into());
    }
    if !state.running.lock().await.is_empty() {
        return Err("Une génération est en cours : arrête-la avant d'effacer.".into());
    }
    for agent in state.all_agents() {
        agent.changes.lock().await.clear();
        agent.approvals.lock().await.clear();
        agent.background.stop_all();
        agent.background.clear_finished();
    }
    *db.lock().map_err(|e| e.to_string())? = History::in_memory().map_err(db_err)?;
    *state.mcp.lock().await = Arc::new(mcp::McpHub::default());
    reminders::set_file(None);
    schedules::set_file(None);
    journal::set_file(None);
    let root = app_root(&app)?;
    // Les secrets de chaque profil : clé API et jetons cités par ses connecteurs.
    for p in profiles::list(&root).unwrap_or_default() {
        let _ = secrets::set(&p.id, "api-key", "");
        let cfg: serde_json::Value = std::fs::read_to_string(profiles::data_dir(&root, &p.id).join("mcp.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        for n in connectors::secret_names(&cfg) {
            let _ = secrets::set(&p.id, &n, "");
        }
    }
    let _ = appsettings::set_autostart(false);
    *state.profile.lock().map_err(|e| e.to_string())? = None;
    localai::shutdown().await;
    browser::shutdown().await;
    tokio::task::spawn_blocking(move || backup::erase_contents(&root)).await.map_err(|e| e.to_string())??;
    app.restart()
}

/// Mémoire, carte graphique, processeur et disque de ce PC, pour recommander des modèles locaux.
#[tauri::command]
async fn hardware_detect() -> Result<hardware::Hardware, String> {
    tokio::task::spawn_blocking(hardware::detect).await.map_err(|e| e.to_string())
}

/// L'ordinateur a-t-il internet ? Même vérification que Windows ; elle sert à couper la recherche web quand on est hors ligne :
/// un Wi-Fi sans accès à internet est « connecté » pour Windows, mais pas pour une recherche.
#[tauri::command]
async fn net_online() -> bool {
    web::online().await
}

/// Les modèles locaux du catalogue, jugés pour la machine décrite par `hardware` (le résultat de `hardware_detect`).
#[tauri::command]
fn local_models_assess(hardware: hardware::Hardware) -> Vec<localmodels::Assessed> {
    localmodels::assess_all(&hardware)
}

#[tauri::command]
fn pc_get(app: AppHandle) -> Result<PcSettings, String> {
    Ok(load_pc(&profile_dir(&app)?))
}

#[tauri::command]
fn pc_set(app: AppHandle, enabled: bool, dirs: Vec<String>) -> Result<PcSettings, String> {
    let mut clean: Vec<String> = Vec::new();
    for d in dirs {
        let d = d.trim().to_string();
        if !d.is_empty() && std::path::Path::new(&d).is_dir() && !clean.iter().any(|c| c.eq_ignore_ascii_case(&d)) {
            clean.push(d);
        }
    }
    let s = PcSettings { enabled, dirs: clean };
    let text = serde_json::to_string_pretty(&s).map_err(|e| e.to_string())?;
    std::fs::write(profile_dir(&app)?.join("pc.json"), text).map_err(|e| e.to_string())?;
    apply_pc(&s);
    Ok(s)
}

fn skills_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = profile_dir(app)?.join("skills");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn disabled_file(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(skills_dir(app)?.join("..").join("skills-disabled.json"))
}

fn agents_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(profile_dir(app)?.join("agents"))
}

/// Agents disponibles : intégrés, ceux de l'application et ceux du projet (`.agents/`).
#[tauri::command]
fn agents_list(app: AppHandle, workspace: Option<String>) -> Result<Vec<agents::AgentProfile>, String> {
    let ws = workspace.filter(|w| !w.trim().is_empty()).map(std::path::PathBuf::from);
    Ok(agents::scan(&agents_dir(&app)?, ws.as_deref()))
}

#[tauri::command]
fn agent_create(app: AppHandle, name: String, description: String, prompt: String, read_only: bool) -> Result<String, String> {
    agents::create(&agents_dir(&app)?, &name, &description, &prompt, read_only).map(|p| p.to_string_lossy().into_owned())
}

/// Copie les agents de Claude Code (`~/.claude/agents`) dans l'application. Renvoie le nombre importé.
#[tauri::command]
fn agents_import_claude(app: AppHandle) -> Result<usize, String> {
    let home = std::env::var_os("USERPROFILE").ok_or("Dossier utilisateur introuvable")?;
    agents::import_from(&std::path::PathBuf::from(home).join(".claude").join("agents"), &agents_dir(&app)?)
}

#[tauri::command]
fn agents_open_dir(app: AppHandle) -> Result<(), String> {
    let dir = agents_dir(&app)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::process::Command::new("explorer").arg(&dir).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Skills installés (avec leur état activé) pour l'application et, éventuellement, le projet.
fn scan_skills_fresh(app: &AppHandle, workspace: Option<&std::path::Path>) -> Result<Vec<skills::Skill>, String> {
    let mut all = skills::scan(&skills_dir(app)?, workspace);
    skills::apply_disabled(&mut all, &skills::load_disabled(&disabled_file(app)?));
    if let Ok(mut cache) = app.state::<AppState>().skills_cache.lock() {
        *cache = Some(SkillsCache { workspace: workspace.map(Into::into), at: std::time::Instant::now(), list: all.clone() });
    }
    Ok(all)
}

/// Skills (avec leur état activé), depuis le cache s'il est récent.
fn scan_skills(app: &AppHandle, workspace: Option<&std::path::Path>) -> Result<Vec<skills::Skill>, String> {
    if let Ok(cache) = app.state::<AppState>().skills_cache.lock() {
        if let Some(c) = cache.as_ref().filter(|c| c.workspace.as_deref() == workspace && c.at.elapsed() < SKILLS_CACHE_TTL) {
            return Ok(c.list.clone());
        }
    }
    scan_skills_fresh(app, workspace)
}

fn invalidate_skills(app: &AppHandle) {
    if let Ok(mut cache) = app.state::<AppState>().skills_cache.lock() {
        *cache = None;
    }
}

/// Poids du prompt système, des skills et des outils (en caractères) pour le panneau « Contexte ».
#[tauri::command]
async fn context_report(app: AppHandle, state: State<'_, AppState>, workspace: Option<String>, mode: String, web: bool, ctx: usize) -> Result<agent::ContextReport, String> {
    let workspace = workspace.filter(|w| !w.trim().is_empty()).map(std::path::PathBuf::from);
    let skills: Vec<skills::Skill> = scan_skills(&app, workspace.as_deref())?.into_iter().filter(|s| s.enabled).collect();
    let hub = state.mcp.lock().await.clone();
    Ok(agent::context_report(workspace, &mode, web, skills, ctx, &hub))
}

#[tauri::command]
fn skills_list(app: AppHandle, workspace: Option<String>) -> Result<Vec<skills::Skill>, String> {
    let ws = workspace.filter(|w| !w.trim().is_empty()).map(std::path::PathBuf::from);
    // La liste du panneau (bouton « Actualiser » compris) relit toujours le disque.
    scan_skills_fresh(&app, ws.as_deref())
}

#[tauri::command]
fn skill_set_enabled(app: AppHandle, name: String, enabled: bool) -> Result<(), String> {
    invalidate_skills(&app);
    let file = disabled_file(&app)?;
    let mut set = skills::load_disabled(&file);
    if enabled { set.remove(&name) } else { set.insert(name) };
    skills::save_disabled(&file, &set)
}

fn auto_imported_file(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(skills_dir(app)?.join("..").join("skills-auto-imported.json"))
}

/// Au démarrage : copie les skills de Claude Desktop et Claude Code absents de l'application.
/// Chaque skill n'est importé qu'une fois : un skill supprimé ensuite ne revient pas. Renvoie le nombre importé.
#[tauri::command]
async fn skills_auto_import(app: AppHandle) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut roots = Vec::new();
        if let Some(p) = std::env::var_os("USERPROFILE") {
            roots.push(std::path::PathBuf::from(p).join(".claude"));
        }
        if let Some(p) = std::env::var_os("APPDATA") {
            roots.push(std::path::PathBuf::from(p).join("Claude"));
        }
        let installed = scan_skills_fresh(&app, None)?;
        let record_file = auto_imported_file(&app)?;
        let mut record = skills::load_disabled(&record_file);
        let dest = skills_dir(&app)?;
        let mut imported = 0;
        for found in skills::discover(&roots, &installed) {
            let key = found.name.to_lowercase();
            // Déjà installé : on le note aussi, pour qu'un skill supprimé plus tard ne revienne pas.
            if found.installed || record.contains(&key) {
                record.insert(key);
                continue;
            }
            if skills::import(&found.dir, &dest) == Ok(true) {
                imported += 1;
            }
            record.insert(key);
        }
        skills::save_disabled(&record_file, &record)?;
        if imported > 0 {
            invalidate_skills(&app);
        }
        Ok(imported)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Classement skills.sh (page publique) : `all`, `trending` ou `hot`.
#[tauri::command]
async fn skills_hub_browse(view: String) -> Result<Vec<skills_hub::Entry>, String> {
    skills_hub::browse(&view).await
}

#[derive(serde::Serialize)]
struct RepoSkills {
    owner: String,
    repo: String,
    /// Skill demandé dans l'entrée (URL ou commande), s'il y en avait un.
    wanted: Option<String>,
    skills: Vec<skills::Found>,
}

/// Télécharge un dépôt GitHub (depuis une commande, une URL skills.sh/GitHub ou `owner/repo`)
/// et liste les skills qu'il contient, sans rien installer.
#[tauri::command]
async fn skills_hub_fetch(app: AppHandle, input: String) -> Result<RepoSkills, String> {
    let spec = skills_hub::parse_spec(&input)?;
    let dest = std::env::temp_dir().join("ollama-desktop-skills").join(format!("{}__{}", spec.owner, spec.repo));
    skills_hub::fetch_repo(&spec, &dest).await?;
    let installed = scan_skills(&app, None)?;
    let mut found = skills::discover(&[dest], &installed);
    for f in &mut found {
        f.group = format!("{}/{}", spec.owner, spec.repo);
    }
    if found.is_empty() {
        return Err(format!("Aucun skill (SKILL.md) dans {}/{}", spec.owner, spec.repo));
    }
    Ok(RepoSkills { owner: spec.owner, repo: spec.repo, wanted: spec.skill, skills: found })
}

#[derive(serde::Serialize)]
struct ImportReport {
    imported: Vec<String>,
    skipped: Vec<String>,
    errors: Vec<String>,
}

/// Copie des skills (dossiers) dans le dossier de l'application. Aucun écrasement.
#[tauri::command]
fn skills_import(app: AppHandle, dirs: Vec<String>) -> Result<ImportReport, String> {
    let dest = skills_dir(&app)?;
    let mut report = ImportReport { imported: vec![], skipped: vec![], errors: vec![] };
    for d in dirs {
        let path = std::path::PathBuf::from(&d);
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(d.clone());
        match skills::import(&path, &dest) {
            Ok(true) => report.imported.push(name),
            Ok(false) => report.skipped.push(name),
            Err(e) => report.errors.push(format!("{name} : {e}")),
        }
    }
    Ok(report)
}

/// Instructions d'un skill, pour la commande `/nom` côté interface.
#[tauri::command]
fn skill_read(app: AppHandle, workspace: Option<String>, name: String, args: Option<String>) -> Result<String, String> {
    let ws = workspace.filter(|w| !w.trim().is_empty()).map(std::path::PathBuf::from);
    let all = scan_skills(&app, ws.as_deref())?;
    let skill = all.iter().find(|s| s.name.eq_ignore_ascii_case(&name)).ok_or("Skill introuvable")?;
    skills::read_body(skill, args.as_deref())
}

#[tauri::command]
fn skills_open_dir(app: AppHandle) -> Result<String, String> {
    let dir = skills_dir(&app)?;
    std::process::Command::new("explorer").arg(&dir).spawn().map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().into_owned())
}

/// Crée un squelette de skill et renvoie son dossier.
#[tauri::command]
fn skill_create(app: AppHandle, name: String, description: String) -> Result<String, String> {
    let slug: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if slug.is_empty() {
        return Err("Nom de skill invalide".into());
    }
    let dir = skills_dir(&app)?.join(&slug);
    if dir.exists() {
        return Err(format!("Le skill « {slug} » existe déjà"));
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let desc = description.replace('\n', " ");
    std::fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {slug}\ndescription: {desc}\n---\n\n# {slug}\n\nDécris ici, étape par étape, comment réaliser cette tâche.\n"),
    )
    .map_err(|e| e.to_string())?;
    std::process::Command::new("notepad").arg(dir.join("SKILL.md")).spawn().map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().into_owned())
}

#[tauri::command]
fn memory_list(db: Db) -> Result<Vec<Memory>, String> {
    db.lock_safe().list_memories().map_err(db_err)
}

#[tauri::command]
fn memory_add(db: Db, text: String, project_id: Option<String>) -> Result<Memory, String> {
    db.lock_safe().add_memory_in(text.trim(), project_id.as_deref().filter(|p| !p.is_empty())).map_err(db_err)
}

/// Mémoire propre à un projet.
#[tauri::command]
fn memory_project_list(db: Db, project_id: String) -> Result<Vec<Memory>, String> {
    db.lock_safe().list_memories_of(Some(&project_id)).map_err(db_err)
}

#[tauri::command]
fn memory_delete(db: Db, id: i64) -> Result<(), String> {
    db.lock_safe().delete_memory(id).map_err(db_err)
}

#[tauri::command]
fn project_docs(db: Db, project_id: String) -> Result<Vec<ProjectDoc>, String> {
    db.lock_safe().list_docs(&project_id).map_err(db_err)
}

#[tauri::command]
fn project_doc_add(db: Db, project_id: String, name: String, content: String) -> Result<ProjectDoc, String> {
    db.lock_safe().add_doc(&project_id, &name, &content).map_err(db_err)
}

#[tauri::command]
fn project_doc_delete(db: Db, id: String) -> Result<(), String> {
    db.lock_safe().delete_doc(&id).map_err(db_err)
}

/// Réponse complète sans streaming (résumés de compactage).
#[tauri::command]
async fn chat_once(
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
fn connectors_catalog() -> Vec<connectors::Connector> {
    connectors::catalog()
}

/// Identifiants des serveurs déjà présents dans `mcp.json`.
#[tauri::command]
fn connectors_installed(app: AppHandle) -> Result<Vec<String>, String> {
    let path = mcp_config_file(app)?;
    let cfg: serde_json::Value = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    Ok(cfg["mcpServers"].as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default())
}

/// Ajoute un connecteur du catalogue : jetons dans le coffre Windows, renvois dans `mcp.json`.
#[tauri::command]
fn connector_add(app: AppHandle, id: String, values: HashMap<String, String>) -> Result<(), String> {
    let conn = connectors::catalog().into_iter().find(|c| c.id == id).ok_or("Connecteur inconnu.")?;
    let (entry, secrets_to_store) = connectors::build_entry(&conn, &values)?;
    let profile = active_profile(&app)?;
    for (name, value) in &secrets_to_store {
        secrets::set(&profile, name, value)?;
    }
    let path = mcp_config_file(app)?;
    let mut cfg: serde_json::Value = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
    let servers = cfg.as_object_mut().ok_or("mcp.json invalide")?.entry("mcpServers").or_insert_with(|| serde_json::json!({}));
    servers[&id] = entry;
    std::fs::write(&path, serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Réécrit `mcp.json` après modification de la liste des serveurs.
fn mcp_update(app: &AppHandle, change: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>) -> Result<(), String>) -> Result<(), String> {
    let path = mcp_config_file(app.clone())?;
    let mut cfg: serde_json::Value = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
    let servers = cfg.as_object_mut().ok_or("mcp.json invalide")?.entry("mcpServers").or_insert_with(|| serde_json::json!({}));
    change(servers.as_object_mut().ok_or("mcp.json invalide")?)?;
    std::fs::write(&path, serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Ajoute un serveur MCP personnalisé : une commande à lancer sur ce PC, ou l'adresse d'un serveur HTTP.
/// `pairs` : variables d'environnement (`NOM=valeur`) ou en-têtes (`Nom: valeur`), une par ligne.
#[tauri::command]
fn mcp_add_custom(app: AppHandle, name: String, http: bool, target: String, pairs: String, all_secret: bool) -> Result<String, String> {
    let id = connectors::clean_id(&name)?;
    let (entry, secrets_to_store) = connectors::custom_entry(&id, http, &target, &connectors::parse_pairs(&pairs, http)?, all_secret)?;
    let profile = active_profile(&app)?;
    mcp_update(&app, |servers| {
        if servers.contains_key(&id) {
            return Err(format!("« {id} » existe déjà : retire-le d'abord ou choisis un autre nom."));
        }
        for (n, v) in &secrets_to_store {
            secrets::set(&profile, n, v)?;
        }
        servers.insert(id.clone(), entry);
        Ok(())
    })?;
    Ok(id)
}

/// Importe une configuration collée (`{ "mcpServers": { … } }`) ; renvoie les noms ajoutés.
#[tauri::command]
fn mcp_import(app: AppHandle, text: String) -> Result<Vec<String>, String> {
    let list = connectors::import_servers(&text)?;
    let profile = active_profile(&app)?;
    let mut added = Vec::new();
    mcp_update(&app, |servers| {
        if let Some((id, _, _)) = list.iter().find(|(id, _, _)| servers.contains_key(id)) {
            return Err(format!("« {id} » existe déjà : retire-le d'abord."));
        }
        for (id, entry, secs) in list {
            for (n, v) in &secs {
                secrets::set(&profile, n, v)?;
            }
            servers.insert(id.clone(), entry);
            added.push(id);
        }
        Ok(())
    })?;
    Ok(added)
}

/// Retire un serveur MCP (du catalogue ou personnalisé) et efface les secrets qu'il citait.
#[tauri::command]
fn mcp_remove(app: AppHandle, name: String) -> Result<(), String> {
    let profile = active_profile(&app)?;
    mcp_update(&app, |servers| {
        if let Some(entry) = servers.remove(&name) {
            for n in connectors::secret_names(&entry) {
                let _ = secrets::set(&profile, &n, "");
            }
        }
        Ok(())
    })
}

/// Retire un connecteur et efface ses secrets du coffre.
#[tauri::command]
fn connector_remove(app: AppHandle, id: String) -> Result<(), String> {
    let profile = active_profile(&app)?;
    if let Some(conn) = connectors::catalog().into_iter().find(|c| c.id == id) {
        for fld in conn.fields.iter().filter(|f| f.secret) {
            let _ = secrets::set(&profile, &format!("mcp:{}:{}", conn.id, fld.key), "");
        }
    }
    let path = mcp_config_file(app)?;
    let mut cfg: serde_json::Value = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    if let Some(o) = cfg.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
        o.remove(&id);
    }
    std::fs::write(&path, serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Ajoute un serveur MCP à mcp.json (sans toucher aux autres).
#[tauri::command]
fn mcp_add(app: AppHandle, name: String, command: String, args: Vec<String>) -> Result<(), String> {
    let path = mcp_config_file(app)?;
    let mut cfg: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let servers = cfg
        .as_object_mut()
        .ok_or("mcp.json invalide")?
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}));
    servers[name] = serde_json::json!({ "command": command, "args": args });
    std::fs::write(&path, serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

#[tauri::command]
async fn agent_approve(state: State<'_, AppState>, call_id: String, allow: bool) -> Result<(), String> {
    for agent in state.all_agents() {
        if let Some(tx) = agent.approvals.lock().await.remove(&call_id) {
            let _ = tx.send(allow);
            break;
        }
    }
    Ok(())
}

#[tauri::command]
async fn agent_undo(window: tauri::Window, state: State<'_, AppState>) -> Result<String, String> {
    state.agent_of(window.label()).undo_last().await
}

#[tauri::command]
async fn mcp_status(state: State<'_, AppState>) -> Result<Vec<mcp::ServerStatus>, String> {
    Ok(state.mcp.lock().await.status.clone())
}

fn mcp_config_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(profile_dir(app)?.join("mcp.json"))
}

#[tauri::command]
fn mcp_config_file(app: AppHandle) -> Result<String, String> {
    let path = mcp_config_path(&app)?;
    if !path.exists() {
        std::fs::write(&path, "{
  \"mcpServers\": {}
}
").map_err(|e| e.to_string())?;
    }
    Ok(path.to_string_lossy().into_owned())
}

/// Ouvre mcp.json dans le Bloc-notes pour l'éditer.
#[tauri::command]
fn mcp_edit_config(app: AppHandle) -> Result<(), String> {
    let path = mcp_config_file(app)?;
    std::process::Command::new("notepad").arg(path).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
async fn mcp_reload(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<mcp::ServerStatus>, String> {
    let hub = Arc::new(mcp::McpHub::start(&mcp_config_path(&app)?, &active_profile(&app)?).await);
    let status = hub.status.clone();
    *state.mcp.lock().await = hub;
    Ok(status)
}

#[derive(serde::Serialize)]
struct FsEntry {
    name: String,
    is_dir: bool,
}

/// Contenu d'un dossier du projet (pour l'arbre de fichiers).
#[tauri::command]
fn fs_list(workspace: String, rel: String) -> Result<Vec<FsEntry>, String> {
    const HIDDEN: [&str; 4] = [".git", "node_modules", "target", ".svelte-kit"];
    let dir = agent::sandbox::resolve(std::path::Path::new(&workspace), &rel)?;
    let mut out: Vec<FsEntry> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| FsEntry {
            name: e.file_name().to_string_lossy().into_owned(),
            is_dir: e.file_type().map(|t| t.is_dir()).unwrap_or(false),
        })
        .filter(|e| !HIDDEN.contains(&e.name.as_str()))
        .collect();
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

/// Arbre de fichiers : crée un fichier vide ou un dossier.
#[tauri::command]
fn fs_create(workspace: String, rel: String, is_dir: bool) -> Result<(), String> {
    agent::fsops::create(std::path::Path::new(&workspace), &rel, is_dir).map(|_| ())
}

/// Arbre de fichiers : renomme un élément (même dossier, nouveau nom).
#[tauri::command]
fn fs_rename(workspace: String, rel: String, new_name: String) -> Result<(), String> {
    let (from, to) = agent::fsops::rename_targets(std::path::Path::new(&workspace), &rel, &new_name)?;
    agent::fsops::do_move(&from, &to)
}

/// Arbre de fichiers : envoie un fichier ou un dossier à la corbeille.
#[tauri::command]
fn fs_delete(workspace: String, rel: String) -> Result<(), String> {
    let path = agent::fsops::existing_target(std::path::Path::new(&workspace), &rel)?;
    agent::fsops::to_trash(&path)
}

/// Enregistre un texte dans un fichier choisi par l'utilisateur (export de conversation).
#[tauri::command]
fn save_text(path: String, content: String) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("Chemin vide.".into());
    }
    std::fs::write(&path, content).map_err(|e| format!("Écriture impossible : {e}"))
}

/// État git du dossier de travail (branche, fichiers modifiés).
#[tauri::command]
fn git_status(workspace: String) -> Result<git::GitStatus, String> {
    git::status(std::path::Path::new(&workspace))
}

#[tauri::command]
fn git_diff(workspace: String, path: String) -> Result<String, String> {
    git::diff(std::path::Path::new(&workspace), &path)
}

/// Valide (commit) tous les changements. Ne pousse rien vers un serveur.
#[tauri::command]
fn git_commit(workspace: String, message: String) -> Result<String, String> {
    git::commit(std::path::Path::new(&workspace), &message)
}

/// Commandes lancées en arrière-plan par l'agent (état et fin de sortie).
#[tauri::command]
fn bg_list(window: tauri::Window, state: State<'_, AppState>) -> Vec<agent::background::ProcInfo> {
    state.agent_of(window.label()).background.list()
}

#[tauri::command]
fn bg_stop(window: tauri::Window, state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.agent_of(window.label()).background.stop(&id)
}

#[tauri::command]
fn bg_clear(window: tauri::Window, state: State<'_, AppState>) {
    state.agent_of(window.label()).background.clear_finished();
}

/// Console du mode Code : exécute une commande dans le dossier du projet.
#[tauri::command]
async fn term_run(workspace: String, command: String) -> Result<String, String> {
    agent::tools::exec_command(std::path::Path::new(&workspace), &command).await
}

#[derive(serde::Serialize)]
struct Attachment {
    name: String,
    /// "image" (contenu en base64) ou "text".
    kind: String,
    data: String,
}

/// Lit un fichier déposé ou choisi par l'utilisateur pour l'attacher à un message.
#[tauri::command]
fn read_attachment(path: String) -> Result<Attachment, String> {
    use base64::Engine;
    const MAX_TEXT: u64 = 300_000;
    const MAX_IMAGE: u64 = 15_000_000;
    let p = std::path::Path::new(&path);
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let size = std::fs::metadata(p).map_err(|e| e.to_string())?.len();
    if ["png", "jpg", "jpeg", "gif", "webp", "bmp"].contains(&ext.as_str()) {
        if size > MAX_IMAGE {
            return Err(format!("Image trop volumineuse ({} Mo max)", MAX_IMAGE / 1_000_000));
        }
        let bytes = std::fs::read(p).map_err(|e| e.to_string())?;
        return Ok(Attachment { name, kind: "image".into(), data: base64::engine::general_purpose::STANDARD.encode(bytes) });
    }
    if ext == "pdf" {
        let data = pdf::extract_text(p)?;
        return Ok(Attachment { name, kind: "text".into(), data });
    }
    if size > MAX_TEXT {
        return Err(format!("Fichier trop volumineux ({} Ko max)", MAX_TEXT / 1000));
    }
    let data = std::fs::read_to_string(p).map_err(|_| format!("{name} n'est pas un fichier texte lisible"))?;
    // Moins de jetons pour rien : espaces de fin et longues séries de lignes vides retirés.
    Ok(Attachment { name, kind: "text".into(), data: pdf::tidy(&data) })
}

#[tauri::command]
async fn model_show(name: String, cloud_key: String) -> Result<serde_json::Value, OllamaError> {
    client_for(&cloud_key)?.show_model(&name).await
}

#[tauri::command]
async fn chat_cancel(state: State<'_, AppState>, request_id: String) -> Result<(), String> {
    if let Some(h) = state.running.lock().await.remove(&request_id) {
        h.abort();
    }
    Ok(())
}

type Db<'a> = State<'a, std::sync::Mutex<History>>;

fn db_err(e: rusqlite::Error) -> String {
    e.to_string()
}

#[tauri::command]
fn conv_create(
    db: Db,
    model: String,
    kind: String,
    workspace: Option<String>,
    project_id: Option<String>,
) -> Result<Conversation, String> {
    db.lock_safe().create(&model, &kind, workspace.as_deref(), project_id.as_deref()).map_err(db_err)
}

#[tauri::command]
fn conv_list(db: Db, search: String, kind: String, project_id: Option<String>) -> Result<Vec<Conversation>, String> {
    db.lock_safe().list(&search, &kind, project_id.as_deref()).map_err(db_err)
}

#[tauri::command]
fn project_create(db: Db, name: String, instructions: String, kind: Option<String>, workspace: Option<String>) -> Result<Project, String> {
    let kind = if kind.as_deref() == Some("code") { "code" } else { "chat" };
    db.lock_safe().create_project(&name, &instructions, kind, workspace.as_deref().filter(|w| !w.trim().is_empty())).map_err(db_err)
}

#[tauri::command]
fn project_list(db: Db) -> Result<Vec<Project>, String> {
    db.lock_safe().list_projects().map_err(db_err)
}

#[tauri::command]
fn project_update(db: Db, id: String, name: String, instructions: String, workspace: Option<String>) -> Result<(), String> {
    db.lock_safe().update_project(&id, &name, &instructions, workspace.as_deref().filter(|w| !w.trim().is_empty())).map_err(db_err)
}

#[tauri::command]
fn project_delete(db: Db, id: String) -> Result<(), String> {
    db.lock_safe().delete_project(&id).map_err(db_err)
}

#[tauri::command]
fn artifact_save(db: Db, conv_id: Option<String>, title: String, lang: String, kind: String, code: String) -> Result<history::SavedArtifact, String> {
    db.lock_safe().save_artifact(conv_id.as_deref(), &title, &lang, &kind, &code).map_err(db_err)
}

#[tauri::command]
fn artifact_list(db: Db, search: String) -> Result<Vec<history::SavedArtifact>, String> {
    db.lock_safe().list_artifacts(&search).map_err(db_err)
}

#[tauri::command]
fn artifact_delete(db: Db, id: String) -> Result<(), String> {
    db.lock_safe().delete_artifact(&id).map_err(db_err)
}

#[tauri::command]
fn project_pin(db: Db, id: String, pinned: bool) -> Result<(), String> {
    db.lock_safe().set_project_pinned(&id, pinned).map_err(db_err)
}

#[tauri::command]
fn conv_pin(db: Db, id: String, pinned: bool) -> Result<(), String> {
    db.lock_safe().set_pinned(&id, pinned).map_err(db_err)
}

/// Nouvelle conversation = copie de celle-ci jusqu'au message `upto` (exclu).
#[tauri::command]
fn conv_fork(db: Db, id: String, upto: usize) -> Result<Conversation, String> {
    db.lock_safe().fork(&id, upto).map_err(db_err)
}

#[tauri::command]
fn conv_set_project(db: Db, id: String, project_id: Option<String>) -> Result<(), String> {
    db.lock_safe().set_conversation_project(&id, project_id.as_deref()).map_err(db_err)
}

#[tauri::command]
fn conv_messages(db: Db, id: String) -> Result<Vec<StoredMessage>, String> {
    db.lock_safe().messages(&id).map_err(db_err)
}

#[tauri::command]
fn conv_save(db: Db, id: String, model: String, messages: Vec<StoredMessage>) -> Result<(), String> {
    db.lock_safe().save_messages(&id, &model, &messages).map_err(db_err)
}

#[tauri::command]
fn conv_rename(db: Db, id: String, title: String) -> Result<(), String> {
    db.lock_safe().rename(&id, &title).map_err(db_err)
}

#[tauri::command]
fn conv_delete(db: Db, id: String) -> Result<(), String> {
    db.lock_safe().delete(&id).map_err(db_err)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Une seule instance : relancer l'application ramène la fenêtre déjà ouverte au premier plan.
        // Ce plugin doit être déclaré avant les autres.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        emergency_stop(app);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            // Un moteur local resté en vie après un plantage serait une mémoire perdue.
            if let Ok(data) = app.path().app_data_dir() {
                localai::kill_pid_file(&data);
            }
            std::fs::create_dir_all(app.path().app_data_dir()?)?;
            export::set_app(app.handle().clone());
            {
                use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
                let ok = app.global_shortcut().register(Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyS)).is_ok();
                SHORTCUT_OK.store(ok, std::sync::atomic::Ordering::Relaxed);
            }
            {
                use tauri::menu::{Menu, MenuItem};
                use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
                let show = MenuItem::with_id(app, "show", "Ouvrir Lama Desktop", true, None::<&str>)?;
                let stop = MenuItem::with_id(app, "stop", "Arrêt d'urgence", true, None::<&str>)?;
                let quit = MenuItem::with_id(app, "quit", "Quitter", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&show, &stop, &quit])?;
                let mut tray = TrayIconBuilder::new()
                    .tooltip("Lama Desktop")
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "show" => show_main(app),
                        "stop" => emergency_stop(app),
                        "quit" => app.exit(0),
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                            show_main(tray.app_handle());
                        }
                    });
                if let Some(icon) = app.default_window_icon() {
                    tray = tray.icon(icon.clone());
                }
                tray.build(app)?;
            }
            // Lancée par Windows au démarrage : la fenêtre reste cachée dans la zone de notification.
            if std::env::args().any(|a| a == "--background") {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            // Base vide jusqu'au choix d'un profil (`profile_select` la remplace par celle du profil).
            app.manage(std::sync::Mutex::new(History::in_memory()?));
            // Rappels : vérifiés toutes les 10 s, tant que l'application tourne.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                use tauri_plugin_notification::NotificationExt;
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                    for r in reminders::take_due() {
                        let _ = handle.notification().builder().title(&r.title).body(&r.body).show();
                        let _ = handle.emit("reminders:fired", &r.id);
                    }
                    // Tâches planifiées : l'interface les lance (nouvelle conversation) et fixe la prochaine échéance.
                    for t in schedules::take_due() {
                        let _ = handle.emit("schedule:due", &t);
                    }
                    // Coffre : verrouillé après 5 minutes sans usage.
                    if vault::global().tick() {
                        let _ = handle.emit("vault:locked", ());
                    }
                }
            });
            Ok(())
        })
        .manage(AppState::default())
        .on_window_event(|window, event| {
            // « Garder actif en arrière-plan » : fermer la fenêtre principale la cache au lieu de quitter.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" && appsettings::load(&window.app_handle().path().app_data_dir().unwrap_or_default()).keep_background {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
            // Fermeture d'une fenêtre : aucune commande lancée par son agent ne doit rester active.
            if let tauri::WindowEvent::Destroyed = event {
                let app = window.app_handle();
                let state = app.state::<AppState>();
                state.board.lock_safe().remove(window.label());
                if let Some(agent) = state.agents.lock_safe().remove(window.label()) {
                    agent.background.stop_all();
                }
                // La fenêtre principale fermée, l'application s'arrête (les autres fenêtres se ferment avec elle).
                if window.label() == "main" {
                    for agent in state.all_agents() {
                        agent.background.stop_all();
                    }
                    app.exit(0);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            cloud_models, cloud_test, chat_start, chat_cancel,
            agent_start, agent_inject, agent_approve, agent_undo, mcp_status, mcp_reload, mcp_config_file, mcp_edit_config,
            fs_list, term_run, read_attachment,
            skills_list, skill_read, skills_open_dir, skill_create,
            skill_set_enabled, skills_auto_import, skills_import, skills_hub_browse, skills_hub_fetch,
            memory_list, memory_add, memory_delete, memory_project_list,
            notify_os, browser_status, browser_launch_ui, browser_close_ui, browser_wipe, vault_browser_login, vault_status, vault_create, vault_unlock, vault_lock, vault_list, vault_login_get, vault_login_save, vault_login_delete, vault_info_save, vault_info_delete, vault_generate, vault_set_agent_access, vault_change_master, vault_destroy, vault_copy, vault_fill_ui, schedules_list, schedule_save, schedule_delete, schedule_done,
            data_backup, data_restore, data_restore_preview, data_export_chats, data_erase,
            project_docs, project_doc_add, project_doc_delete,
            chat_once, mcp_add, mcp_add_custom, mcp_import, mcp_remove,
            secret_get, secret_set, save_text, git_status, git_diff, git_commit,
            journal_list, journal_clear, last_shot, app_settings_get, app_settings_set, connectors_catalog, connectors_installed, connector_add, connector_remove, profile_active, window_new, window_task, board_report, board_list, board_focus, pc_get, pc_set, hardware_detect, net_online, local_models_assess, local_installed, local_set_ctx, local_status, local_install, local_cancel, local_remove, save_bytes, reveal_file, agent_changes, git_branch_create, git_review_diff, project_templates, project_from_template, project_rules_get, project_rules_set, export_document, export_html_pdf, voice_dictation, reminders_list, reminders_cancel, bg_list, bg_stop, bg_clear,
            profiles_list, profile_create, profile_update, profile_set_pin, profile_delete, profile_select,
            agents_list, agent_create, agents_import_claude, agents_open_dir,
            model_show, context_report, fs_create, fs_rename, fs_delete,
            conv_pin, conv_fork, conv_create, conv_list, conv_messages, conv_save, conv_rename, conv_delete,
            project_create, project_list, project_update, project_delete, project_pin, artifact_save, artifact_list, artifact_delete, conv_set_project
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Le navigateur et le moteur local se ferment avec l'application.
            if let tauri::RunEvent::Exit = event {
                browser::kill_now();
                if let Ok(data) = app.path().app_data_dir() {
                    localai::kill_now(&data);
                }
            }
        });
}
