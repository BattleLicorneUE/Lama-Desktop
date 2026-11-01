mod agent;
mod sync;
mod commands;
mod webpane;
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
mod analytics;
mod appsettings;
mod uvrt;
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
use commands::{agents::*, skills::*};
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
    let started = std::time::Instant::now();
    let (port, key) = match localai::ensure_running(&data, id, ctx).await {
        Ok(r) => r,
        Err(e) => {
            track(app, "local_engine_failed", serde_json::json!({ "model": id }));
            return Err(OllamaError::Status(500, e));
        }
    };
    // Un modèle déjà chargé répond tout de suite : seul un vrai démarrage est intéressant.
    if started.elapsed().as_secs() >= 2 {
        track(app, "local_engine_started", serde_json::json!({ "model": id, "ctx": ctx, "secondes": started.elapsed().as_secs() }));
    }
    localai::touch().await;
    OllamaClient::local(port, &key).map_err(|e| OllamaError::Status(500, e))
}

/// Statistiques anonymes : sans « oui » de l'utilisateur, cette fonction ne fait rien.
fn track(app: &AppHandle, name: &'static str, props: serde_json::Value) {
    if let Ok(dir) = app_root(app) {
        analytics::track(&dir, &app.package_info().version.to_string(), name, props);
    }
}

/// Une réponse a échoué : on garde seulement le type d'erreur, jamais son texte.
fn track_error(app: &AppHandle, msg: &str, model: &str) {
    track(app, "chat_error", serde_json::json!({ "kind": analytics::error_kind(msg), "local": model.starts_with("local:") }));
}

/// Événements venant de l'interface : pages de Réglages ouvertes et mises à jour installées (valeurs courtes, sans espaces).
#[tauri::command]
fn analytics_ui(app: AppHandle, event: String, value: String) {
    let ok = !value.is_empty() && value.len() <= 24 && value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
    if !ok {
        return;
    }
    match event.as_str() {
        "settings_page" => track(&app, "settings_page", serde_json::json!({ "page": value })),
        "update_installed" => track(&app, "update_installed", serde_json::json!({ "from": value })),
        _ => {}
    }
}

#[tauri::command]
fn analytics_get(app: AppHandle) -> Result<analytics::Info, String> {
    Ok(analytics::info(&app_root(&app)?))
}

#[tauri::command]
fn analytics_set(app: AppHandle, enabled: bool) -> Result<analytics::Info, String> {
    let dir = app_root(&app)?;
    analytics::set_consent(&dir, enabled)?;
    if enabled {
        send_started(&app);
    }
    Ok(analytics::info(&dir))
}

/// Lancement : la puissance du PC en gros (tranches), jamais le nom exact des composants.
fn send_started(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let hw = hardware::cached().await;
        let gpu = hw.best_gpu();
        let vendor = gpu.map_or("aucune", |g| match g.vendor {
            hardware::Vendor::Nvidia => "nvidia",
            hardware::Vendor::Amd => "amd",
            hardware::Vendor::Intel => "intel",
            hardware::Vendor::Other => "autre",
        });
        track(&app, "app_started", serde_json::json!({
            "ram_go": (hw.ram_mb as f64 / 4096.0).round() as u64 * 4,
            "gpu": vendor,
            "vram_go": gpu.map_or(0, |g| (g.vram_mb as f64 / 2048.0).round() as u64 * 2),
            "avx2": hw.avx2,
        }));
    });
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

/// Refuse d'écrire un programme ou un fichier de démarrage automatique : l'enregistrement sert aux documents, images et exports.
fn check_save_target(path: &str) -> Result<(), String> {
    let p = std::path::Path::new(path.trim());
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("").trim_end_matches(['.', ' ']).to_ascii_lowercase();
    let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    if ["exe", "dll", "msi", "scr", "lnk", "url", "hta", "cpl", "pif", "com", "sys"].contains(&ext) {
        return Err(format!("Enregistrement refusé : un fichier .{ext} serait un programme."));
    }
    let lower = path.to_lowercase().replace('/', "\\");
    if lower.contains("\\start menu\\programs\\startup") || lower.starts_with("c:\\windows\\") {
        return Err("Enregistrement refusé : ce dossier lance des programmes ou appartient à Windows.".into());
    }
    Ok(())
}

/// Enregistre des octets (image PNG exportée, etc.) donnés en base64.
#[tauri::command]
fn save_bytes(path: String, base64: String) -> Result<(), String> {
    use base64::Engine;
    if path.trim().is_empty() {
        return Err("Chemin vide.".into());
    }
    check_save_target(&path)?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(base64.trim()).map_err(|e| format!("Données invalides : {e}"))?;
    std::fs::write(&path, bytes).map_err(|e| format!("Écriture impossible : {e}"))
}

/// Dictée vocale de Windows (Win + H) : la saisie arrive dans le champ qui a le focus.
#[tauri::command]
async fn voice_dictation() -> Result<(), String> {
    tokio::task::spawn_blocking(|| system::computer("key", None, None, None, Some("win+h"), None).map(|_| ()))
        .await
        .map_err(|e| e.to_string())?
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

type Db<'a> = State<'a, std::sync::Mutex<History>>;

fn db_err(e: rusqlite::Error) -> String {
    e.to_string()
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
            analytics::init(&app.path().app_data_dir()?, &app.package_info().version.to_string());
            uvrt::set_data_dir(&app.path().app_data_dir()?);
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
            send_started(app.handle());
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
        .invoke_handler({
            let handler: Box<dyn Fn(tauri::ipc::Invoke) -> bool + Send + Sync> = Box::new(tauri::generate_handler![
            webpane::webpane_open, webpane::webpane_bounds, webpane::webpane_navigate, webpane::webpane_action, webpane::webpane_visible, webpane::webpane_close,
            commands::chat::cloud_models, commands::chat::cloud_test, commands::chat::chat_start, commands::chat::chat_cancel,
            commands::chat::agent_start, commands::chat::agent_inject, commands::chat::agent_approve, commands::chat::agent_undo, commands::connectors::mcp_status, commands::connectors::mcp_reload, commands::connectors::mcp_config_file, commands::connectors::mcp_edit_config,
            commands::workspace::fs_list, commands::workspace::term_run, commands::workspace::read_attachment,
            commands::skills::skills_list, commands::skills::skill_read, commands::skills::skills_open_dir, commands::skills::skill_get, commands::skills::skill_save, commands::skills::skill_delete,
            commands::skills::skill_set_enabled, commands::skills::skills_auto_import, commands::skills::skills_import, commands::skills::skills_hub_browse, commands::skills::skills_hub_fetch,
            commands::history::memory_list, commands::history::memory_add, commands::history::memory_delete, commands::history::memory_project_list,
            notify_os, commands::vault::browser_status, commands::vault::browser_launch_ui, commands::vault::browser_close_ui, commands::vault::browser_wipe, commands::vault::vault_browser_login, commands::vault::vault_status, commands::vault::vault_create, commands::vault::vault_unlock, commands::vault::vault_lock, commands::vault::vault_list, commands::vault::vault_login_get, commands::vault::vault_login_save, commands::vault::vault_login_delete, commands::vault::vault_info_save, commands::vault::vault_info_delete, commands::vault::vault_generate, commands::vault::vault_set_agent_access, commands::vault::vault_change_master, commands::vault::vault_destroy, commands::vault::vault_copy, commands::vault::vault_fill_ui, commands::schedules::schedules_list, commands::schedules::schedule_save, commands::schedules::schedule_delete, commands::schedules::schedule_done,
            commands::data::data_backup, commands::data::data_restore, commands::data::data_restore_preview, commands::data::data_export_chats, commands::data::data_erase,
            commands::history::project_docs, commands::history::project_doc_add, commands::history::project_doc_delete,
            commands::chat::chat_once, commands::connectors::mcp_add, commands::connectors::mcp_add_custom, commands::connectors::mcp_import, commands::connectors::mcp_remove,
            commands::profiles::secret_get, commands::profiles::secret_set, commands::workspace::save_text, commands::workspace::git_status, commands::workspace::git_diff, commands::workspace::git_commit,
            journal_list, journal_clear, last_shot, app_settings_get, app_settings_set, analytics_get, analytics_set, analytics_ui, commands::connectors::connectors_catalog, commands::connectors::connectors_installed, commands::connectors::connector_add, commands::connectors::connector_remove, commands::profiles::profile_active, commands::windows::window_new, commands::windows::window_task, commands::windows::board_report, commands::windows::board_list, commands::windows::board_focus, pc_get, pc_set, commands::local::hardware_detect, commands::local::net_online, commands::local::local_models_assess, commands::local::local_installed, commands::local::local_set_ctx, commands::local::local_status, commands::local::local_install, commands::local::local_cancel, commands::local::local_remove, commands::local::local_clean_partials, save_bytes, reveal_file, commands::chat::agent_changes, commands::workspace::git_branch_create, commands::workspace::git_review_diff, commands::workspace::project_templates, commands::workspace::project_from_template, commands::workspace::project_rules_get, commands::workspace::project_rules_set, commands::workspace::export_document, commands::workspace::export_html_pdf, voice_dictation, commands::schedules::reminders_list, commands::schedules::reminders_cancel, commands::workspace::bg_list, commands::workspace::bg_stop, commands::workspace::bg_clear,
            commands::profiles::profiles_list, commands::profiles::profile_create, commands::profiles::profile_update, commands::profiles::profile_set_pin, commands::profiles::profile_delete, commands::profiles::profile_select,
            commands::agents::agents_list, commands::agents::agent_get, commands::agents::agent_save, commands::agents::agent_delete, commands::agents::agents_import_claude, commands::agents::agents_open_dir,
            commands::chat::model_show, commands::skills::context_report, commands::workspace::fs_create, commands::workspace::fs_rename, commands::workspace::fs_delete,
            commands::history::conv_pin, commands::history::conv_fork, commands::history::conv_create, commands::history::conv_list, commands::history::conv_messages, commands::history::conv_save, commands::history::conv_rename, commands::history::conv_delete,
            commands::history::project_create, commands::history::project_list, commands::history::project_update, commands::history::project_delete, commands::history::project_pin, commands::history::artifact_save, commands::history::artifact_list, commands::history::artifact_delete, commands::history::conv_set_project
            ]);
            move |invoke| {
                if let Some(feature) = analytics::feature_of(invoke.message.command()) {
                    track(invoke.message.webview_ref().app_handle(), "feature_used", serde_json::json!({ "feature": feature }));
                }
                handler(invoke)
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Le navigateur et le moteur local se ferment avec l'application.
            if let tauri::RunEvent::Exit = event {
                if let Ok(dir) = app.path().app_data_dir() {
                    analytics::track_blocking(&dir, &app.package_info().version.to_string(), "app_closed", serde_json::json!({ "minutes": analytics::minutes_open() }));
                }
                browser::kill_now();
                if let Ok(data) = app.path().app_data_dir() {
                    localai::kill_now(&data);
                }
            }
        });
}

#[cfg(test)]
mod save_tests {
    use super::check_save_target;

    #[test]
    fn saving_refuses_programs_and_startup_folders_but_keeps_documents() {
        for bad in ["C:/x/virus.exe", "C:/x/virus.exe.", "C:/x/a.LNK", "C:/Users/a/AppData/Roaming/Microsoft/Windows/Start Menu/Programs/Startup/x.txt", "C:/Windows/System32/x.txt"] {
            assert!(check_save_target(bad).is_err(), "{bad}");
        }
        for ok in ["C:/Users/a/Documents/note.md", "C:/Users/a/Desktop/export.pdf", "C:/Users/a/Desktop/script.js", "C:/Users/a/Desktop/app.py"] {
            assert!(check_save_target(ok).is_ok(), "{ok}");
        }
    }
}
