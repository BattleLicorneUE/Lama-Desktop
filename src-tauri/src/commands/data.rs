//! Sauvegarde, restauration, export et effacement des données.

use crate::*;

/// Sauvegarde du profil ouvert (conversations, projets, mémoire, réglages, skills, connecteurs) dans une archive .zip.
/// Les secrets restent dans le coffre Windows : ils ne sont pas inclus.
#[tauri::command]
pub(crate) async fn data_backup(app: AppHandle, db: Db<'_>, path: String) -> Result<usize, String> {
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
pub(crate) async fn data_restore_preview(path: String) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || backup::mcp_in_archive(std::path::Path::new(&path))).await.map_err(|e| e.to_string())?
}

/// Restaure une sauvegarde dans le profil ouvert : ses données actuelles sont remplacées (l'ancienne base est gardée à côté).
#[tauri::command]
pub(crate) async fn data_restore(app: AppHandle, state: State<'_, AppState>, db: Db<'_>, path: String, with_mcp: Option<bool>) -> Result<usize, String> {
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
pub(crate) async fn data_export_chats(db: Db<'_>, path: String) -> Result<usize, String> {
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
pub(crate) async fn data_erase(app: AppHandle, state: State<'_, AppState>, db: Db<'_>, confirm: String) -> Result<(), String> {
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
