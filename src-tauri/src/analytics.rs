//! Statistiques d'usage anonymes (Aptabase), envoyées seulement si l'utilisateur a dit oui.
//!
//! Aucun identifiant n'est gardé d'un lancement à l'autre : une « session » aléatoire par lancement.
//! Rien du contenu des conversations n'est jamais envoyé, seulement les événements de la liste EVENTS.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// Clé de l'application sur aptabase.com (publique par nature). Vide : rien n'est jamais envoyé.
const APP_KEY: &str = match option_env!("LAMA_APTABASE_KEY") {
    Some(k) => k,
    None => "",
};

/// Les seuls événements que l'application peut envoyer, avec leur explication affichée dans les Réglages.
pub const EVENTS: &[(&str, &str)] = &[
    ("app_started", "Lancement de l'application, avec la puissance du PC en gros (mémoire, type de carte graphique)."),
    ("chat_started", "Un message envoyé : cloud ou local, et le nom du modèle. Jamais le texte."),
    ("agent_started", "Une tâche confiée à l'agent : le mode choisi et le nom du modèle. Jamais le contenu."),
    ("local_model_installed", "Un modèle local installé : son nom dans le catalogue."),
    ("local_model_failed", "Une installation de modèle local qui a échoué : son nom dans le catalogue."),
    ("connector_added", "Un connecteur ajouté depuis le catalogue : son nom dans le catalogue."),
    ("local_engine_started", "Le moteur local a démarré : le modèle, la taille de contexte et le temps de démarrage."),
    ("local_engine_failed", "Le moteur local n'a pas pu démarrer : le modèle."),
    ("chat_error", "Une réponse qui a échoué : seulement le type d'erreur (clé refusée, limite atteinte, réseau…), jamais le message."),
    ("feature_used", "Une fonction essayée (sauvegarde, tâche planifiée, coffre, nouvelle fenêtre…) : son nom, jamais son contenu."),
    ("settings_page", "Une page des Réglages ouverte : son nom."),
    ("update_installed", "Une mise à jour installée : la version précédente."),
    ("app_closed", "La fermeture de l'application : la durée d'utilisation en minutes."),
    ("app_panic", "Un plantage interne : le fichier et la ligne du code concerné, jamais de données."),
];

/// Commandes de l'application comptées comme « fonction essayée » : le nom de la commande, jamais ses arguments.
pub const FEATURES: &[(&str, &str)] = &[
    ("data_backup", "backup"), ("data_restore", "restore"), ("data_export_chats", "export_chats"), ("export_document", "export_document"),
    ("skill_save", "skill_saved"), ("skill_delete", "skill_deleted"), ("skills_import", "skill_imported"), ("agent_create", "agent_created"), ("agents_import_claude", "agents_imported"),
    ("project_create", "project_created"), ("project_from_template", "project_from_template"), ("schedule_save", "task_scheduled"),
    ("vault_create", "vault_created"), ("vault_fill_ui", "vault_fill"), ("vault_browser_login", "vault_browser_login"),
    ("window_new", "new_window"), ("mcp_add_custom", "custom_server"), ("mcp_import", "mcp_import"), ("voice_dictation", "voice"),
    ("conv_fork", "conversation_fork"), ("artifact_save", "artifact_saved"), ("git_commit", "git_commit"), ("git_review_diff", "code_review"),
    ("profile_create", "profile_created"), ("agent_undo", "undo"), ("term_run", "terminal"), ("browser_launch_ui", "browser"),
    ("skills_hub_browse", "skills_hub"), ("memory_add", "memory_added"), ("pc_set", "pc_control"), ("local_set_ctx", "local_context"),
    ("data_erase", "erase_all"),
];

/// Nom de la fonction pour une commande, si elle fait partie de la liste.
pub fn feature_of(command: &str) -> Option<&'static str> {
    FEATURES.iter().find(|(c, _)| *c == command).map(|(_, f)| *f)
}

/// Type d'une erreur de réponse, sans jamais garder son texte.
pub fn error_kind(msg: &str) -> &'static str {
    let m = msg.to_lowercase();
    if m.contains("401") || m.contains("403") || m.contains("clé") || m.contains("unauthorized") {
        "auth"
    } else if m.contains("429") || m.contains("limite") || m.contains("rate") {
        "rate_limit"
    } else if m.contains("modèle local") || m.contains("moteur") || m.contains("llama") {
        "local_engine"
    } else if m.contains("timed out") || m.contains("timeout") || m.contains("délai") {
        "timeout"
    } else if m.contains("dns") || m.contains("connect") || m.contains("réseau") || m.contains("network") {
        "network"
    } else if m.contains("500") || m.contains("502") || m.contains("503") || m.contains("504") {
        "server"
    } else {
        "other"
    }
}

const LOG_MAX: usize = 50;

static LOG: Mutex<VecDeque<Sent>> = Mutex::new(VecDeque::new());
/// Session en cours et date de son dernier événement : Aptabase en veut une nouvelle après une heure sans activité.
static SESSION: Mutex<(String, u64)> = Mutex::new((String::new(), 0));
static STARTED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
static CONTEXT: std::sync::OnceLock<(std::path::PathBuf, String)> = std::sync::OnceLock::new();

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct Consent {
    /// La question a été posée (une seule fois).
    pub asked: bool,
    pub enabled: bool,
}

/// Un événement tel qu'il a été envoyé, gardé en mémoire pour la page « ce qui est envoyé ».
#[derive(Serialize, Clone, Debug)]
pub struct Sent {
    pub t: u64,
    pub name: String,
    pub props: Value,
}

#[derive(Serialize)]
pub struct Info {
    /// Une clé Aptabase est intégrée à cette version.
    pub available: bool,
    pub asked: bool,
    pub enabled: bool,
    pub events: Vec<(String, String)>,
    pub sent: Vec<Sent>,
}

fn file(dir: &Path) -> std::path::PathBuf {
    dir.join("analytics.json")
}

pub fn consent(dir: &Path) -> Consent {
    std::fs::read_to_string(file(dir)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn set_consent(dir: &Path, enabled: bool) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let c = Consent { asked: true, enabled };
    std::fs::write(file(dir), serde_json::to_string_pretty(&c).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    if !enabled {
        LOG.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
    Ok(())
}

pub fn info(dir: &Path) -> Info {
    let c = consent(dir);
    Info {
        available: !APP_KEY.is_empty(),
        asked: c.asked,
        enabled: c.enabled && !APP_KEY.is_empty(),
        events: EVENTS.iter().map(|(n, d)| (n.to_string(), d.to_string())).collect(),
        sent: LOG.lock().unwrap_or_else(|e| e.into_inner()).iter().rev().cloned().collect(),
    }
}

/// Serveur Aptabase de la région de la clé (A-EU-…, A-US-…).
fn host(key: &str) -> Option<&'static str> {
    match key.split('-').nth(1)? {
        "EU" => Some("https://eu.aptabase.com"),
        "US" => Some("https://us.aptabase.com"),
        _ => None,
    }
}

/// Date ISO 8601 en UTC à partir d'un nombre de secondes depuis 1970.
fn iso(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Jour civil depuis le nombre de jours (algorithme de H. Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Garde seulement des textes, nombres et booléens courts : rien d'autre ne part.
fn clean(props: Value) -> Value {
    let mut out = Map::new();
    if let Value::Object(m) = props {
        for (k, v) in m {
            let v = match v {
                Value::String(s) => Value::String(s.chars().take(60).collect()),
                Value::Number(_) | Value::Bool(_) => v,
                _ => continue,
            };
            out.insert(k.chars().take(40).collect(), v);
        }
    }
    Value::Object(out)
}

fn payload(name: &str, props: &Value, session: &str, now: u64, version: &str) -> Value {
    json!({
        "timestamp": iso(now),
        "sessionId": session,
        "eventName": name,
        "systemProps": {
            "isDebug": cfg!(debug_assertions),
            "locale": "fr-FR",
            "osName": "Windows",
            "osVersion": "",
            "engineName": "WebView2",
            "engineVersion": "",
            "appVersion": version,
            "sdkVersion": "lama-desktop@1",
        },
        "props": props,
    })
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn session(now: u64) -> String {
    let mut s = SESSION.lock().unwrap_or_else(|e| e.into_inner());
    if s.0.is_empty() || now.saturating_sub(s.1) > 3600 {
        s.0 = format!("{now}{:08}", uuid::Uuid::new_v4().as_u128() % 100_000_000);
    }
    s.1 = now;
    s.0.clone()
}

/// À appeler au démarrage : mémorise le dossier et la version pour les événements envoyés sans accès à l'application (plantages).
pub fn init(dir: &Path, version: &str) {
    let _ = STARTED.set(std::time::Instant::now());
    let _ = CONTEXT.set((dir.to_path_buf(), version.to_string()));
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some((dir, version)) = CONTEXT.get() {
            // Seulement l'endroit du code : le message d'un plantage peut contenir des données.
            let at = info
                .location()
                .map(|l| format!("{}:{}", l.file().rsplit(['/', '\\']).next().unwrap_or(""), l.line()))
                .unwrap_or_default();
            track(dir, version, "app_panic", json!({ "at": at }));
        }
        previous(info);
    }));
}

/// Minutes depuis le lancement.
pub fn minutes_open() -> u64 {
    STARTED.get().map_or(0, |t| t.elapsed().as_secs() / 60)
}

/// Envoie un événement si, et seulement si, une clé est intégrée, l'utilisateur a dit oui et le nom est dans la liste.
pub fn track(dir: &Path, version: &str, name: &'static str, props: Value) {
    if APP_KEY.is_empty() || !EVENTS.iter().any(|(n, _)| *n == name) || !consent(dir).enabled {
        return;
    }
    let Some(host) = host(APP_KEY) else { return };
    let props = clean(props);
    let now = now_secs();
    {
        let mut log = LOG.lock().unwrap_or_else(|e| e.into_inner());
        if log.len() == LOG_MAX {
            log.pop_front();
        }
        log.push_back(Sent { t: now, name: name.to_string(), props: props.clone() });
    }
    let body = payload(name, &props, &session(now), now, version);
    tauri::async_runtime::spawn(async move {
        // Une statistique perdue n'a aucune importance : aucune erreur n'est remontée.
        let _ = post(host, &body).await;
    });
}

/// Envoie l'événement et attend la réponse : à la fermeture, pour ne pas le perdre.
pub fn track_blocking(dir: &Path, version: &str, name: &'static str, props: Value) {
    if APP_KEY.is_empty() || !EVENTS.iter().any(|(n, _)| *n == name) || !consent(dir).enabled {
        return;
    }
    let Some(host) = host(APP_KEY) else { return };
    let now = now_secs();
    let body = payload(name, &clean(props), &session(now), now, version);
    let _ = tauri::async_runtime::block_on(async { tokio::time::timeout(std::time::Duration::from_secs(3), post(host, &body)).await });
}

/// Code HTTP de la réponse du serveur.
async fn post(host: &str, body: &Value) -> Result<u16, reqwest::Error> {
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build()?;
    Ok(client.post(format!("{host}/api/v0/event")).header("App-Key", APP_KEY).json(body).send().await?.status().as_u16())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_sent_before_the_user_says_yes() {
        let d = std::env::temp_dir().join(format!("analytics-{}", uuid::Uuid::new_v4()));
        assert_eq!(consent(&d), Consent::default());
        assert!(!consent(&d).enabled && !consent(&d).asked);
        set_consent(&d, true).unwrap();
        assert_eq!(consent(&d), Consent { asked: true, enabled: true });
        set_consent(&d, false).unwrap();
        assert_eq!(consent(&d), Consent { asked: true, enabled: false });
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn without_a_key_the_feature_reports_itself_unavailable() {
        let d = std::env::temp_dir().join(format!("analytics-{}", uuid::Uuid::new_v4()));
        set_consent(&d, true).unwrap();
        if APP_KEY.is_empty() {
            assert!(!info(&d).available && !info(&d).enabled);
            track(&d, "0", "app_started", json!({}));
            assert!(info(&d).sent.is_empty());
        }
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn region_comes_from_the_key() {
        assert_eq!(host("A-EU-1234567890"), Some("https://eu.aptabase.com"));
        assert_eq!(host("A-US-1234567890"), Some("https://us.aptabase.com"));
        assert_eq!(host("A-SH-1234567890"), None);
        assert_eq!(host("n'importe quoi"), None);
    }

    #[test]
    fn dates_are_iso_8601_in_utc() {
        assert_eq!(iso(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(iso(1_793_000_000), "2026-10-26T07:33:20Z");
    }

    #[test]
    fn only_short_plain_values_are_kept() {
        let p = clean(json!({ "model": "x".repeat(200), "n": 3, "ok": true, "nested": { "a": 1 }, "list": [1, 2], "none": null }));
        assert_eq!(p["model"].as_str().unwrap().len(), 60);
        assert_eq!(p["n"], 3);
        assert_eq!(p["ok"], true);
        assert!(p.get("nested").is_none() && p.get("list").is_none() && p.get("none").is_none());
    }

    #[test]
    fn payload_has_what_aptabase_expects() {
        let b = payload("chat_started", &json!({ "kind": "cloud" }), "123", 0, "0.3.1");
        assert_eq!(b["eventName"], "chat_started");
        assert_eq!(b["sessionId"], "123");
        assert_eq!(b["systemProps"]["appVersion"], "0.3.1");
        assert_eq!(b["props"]["kind"], "cloud");
    }

    #[test]
    fn a_session_lasts_until_an_hour_without_activity() {
        let a = session(10_000_000_000);
        assert_eq!(a, session(10_000_000_000 + 3_000), "dans l'heure : même session");
        assert_ne!(a, session(10_000_000_000 + 3_000 + 3_601), "plus d'une heure : nouvelle session");
    }

    #[test]
    fn features_are_known_commands_only_and_errors_lose_their_text() {
        assert_eq!(feature_of("data_backup"), Some("backup"));
        assert_eq!(feature_of("chat_start"), None);
        assert_eq!(feature_of("secret_get"), None);
        assert_eq!(error_kind("HTTP 401 : clé API refusée"), "auth");
        assert_eq!(error_kind("429 Too Many Requests"), "rate_limit");
        assert_eq!(error_kind("Modèle local inconnu : x"), "local_engine");
        assert_eq!(error_kind("operation timed out"), "timeout");
        assert_eq!(error_kind("secret du client ABC"), "other");
    }

    /// Envoie un vrai événement de test (marqué « debug », donc séparé des vraies statistiques) : `cargo test live_event -- --ignored`.
    #[test]
    #[ignore]
    fn live_event_is_accepted_by_aptabase() {
        if APP_KEY.is_empty() {
            return;
        }
        let now = now_secs();
        let body = payload("app_started", &json!({ "verif": "test" }), &session(now), now, "test");
        let status = tauri::async_runtime::block_on(post(host(APP_KEY).unwrap(), &body)).unwrap();
        assert_eq!(status, 200);
    }

    #[test]
    fn unknown_event_names_are_refused() {
        assert!(EVENTS.iter().all(|(n, _)| n.chars().all(|c| c.is_ascii_lowercase() || c == '_')));
        assert!(!EVENTS.iter().any(|(n, _)| *n == "conversation_text"));
    }
}
