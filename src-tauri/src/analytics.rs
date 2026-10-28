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
];

const LOG_MAX: usize = 50;

static LOG: Mutex<VecDeque<Sent>> = Mutex::new(VecDeque::new());
static SESSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();

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

fn session() -> &'static str {
    SESSION.get_or_init(|| format!("{}{:08}", now_secs(), uuid::Uuid::new_v4().as_u128() % 100_000_000))
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
    let body = payload(name, &props, session(), now, version);
    tauri::async_runtime::spawn(async move {
        // Une statistique perdue n'a aucune importance : aucune erreur n'est remontée.
        let Ok(client) = reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build() else { return };
        let _ = client.post(format!("{host}/api/v0/event")).header("App-Key", APP_KEY).json(&body).send().await;
    });
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
    fn unknown_event_names_are_refused() {
        assert!(EVENTS.iter().all(|(n, _)| n.chars().all(|c| c.is_ascii_lowercase() || c == '_')));
        assert!(!EVENTS.iter().any(|(n, _)| *n == "conversation_text"));
    }
}
