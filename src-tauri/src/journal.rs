//! Journal des actions de l'agent sur le PC (commandes, fichiers, souris, presse-papiers…), pour savoir après coup ce qui s'est passé.
//! Une ligne JSON par action, dans le dossier du profil. Le contenu saisi ou copié n'est jamais enregistré, seulement sa taille.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

static FILE: Mutex<Option<PathBuf>> = Mutex::new(None);
const KEEP: usize = 500;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Entry {
    /// Secondes Unix.
    pub t: u64,
    pub tool: String,
    pub summary: String,
    /// done, error ou denied.
    pub outcome: String,
}

pub fn set_file(path: Option<PathBuf>) {
    *FILE.lock().unwrap() = path;
}

/// Les outils dont on garde la trace : tout ce qui agit (pas les simples lectures de fichiers).
pub fn is_logged(tool: &str) -> bool {
    matches!(
        tool,
        "write_file" | "edit_file" | "make_dir" | "move_path" | "delete_path" | "run_command" | "verify" | "stop_command"
            | "open_target" | "clipboard" | "pc_control" | "screenshot" | "computer" | "create_document" | "notify"
    ) || tool.starts_with("mcp__")
}

fn clip(s: &str, n: usize) -> String {
    let one: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > n { format!("{}…", one.chars().take(n).collect::<String>()) } else { one }
}

/// Résumé lisible de l'appel, sans le texte saisi ni copié.
pub fn summarize(tool: &str, args: &Value) -> String {
    let s = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("");
    let text = match tool {
        "run_command" => s("command").to_string(),
        "verify" => format!("vérification {}", if s("target").is_empty() { "check" } else { s("target") }),
        "write_file" | "edit_file" | "make_dir" | "delete_path" => s("path").to_string(),
        "move_path" => format!("{} → {}", s("from"), s("to")),
        "open_target" => s("target").to_string(),
        "clipboard" => match (s("action"), args.get("text").and_then(Value::as_str)) {
            ("write", Some(t)) => format!("copie de {} caractères", t.chars().count()),
            (a, _) => a.to_string(),
        },
        "pc_control" => format!("{} {}", s("action"), args.get("value").map(|v| v.to_string()).unwrap_or_default()),
        "computer" => {
            let pos = match (args.get("x"), args.get("y")) {
                (Some(x), Some(y)) => format!(" en ({x}, {y})"),
                _ => String::new(),
            };
            let extra = match (args.get("text").and_then(Value::as_str), s("keys")) {
                (Some(t), _) => format!(" ({} caractères)", t.chars().count()),
                (None, k) if !k.is_empty() => format!(" {k}"),
                _ => String::new(),
            };
            format!("{}{pos}{extra}", s("action"))
        }
        "create_document" => format!("{} (.{})", s("title"), s("format")),
        "notify" => s("message").to_string(),
        "screenshot" => "capture de l'écran".to_string(),
        _ => args.to_string(),
    };
    clip(&text, 200)
}

pub fn add(tool: &str, args: &Value, outcome: &str) {
    let guard = FILE.lock().unwrap();
    let Some(path) = guard.as_ref() else { return };
    let entry = Entry {
        t: SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()),
        tool: tool.to_string(),
        summary: summarize(tool, args),
        outcome: outcome.to_string(),
    };
    if let Ok(line) = serde_json::to_string(&entry) {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{line}");
        }
    }
}

/// Les `limit` dernières actions, la plus récente d'abord.
pub fn list(limit: usize) -> Vec<Entry> {
    let guard = FILE.lock().unwrap();
    let Some(path) = guard.as_ref() else { return vec![] };
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut all: Vec<Entry> = text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    if all.len() > KEEP * 2 {
        // Le journal ne grossit pas indéfiniment : on ne garde que les dernières lignes.
        let keep = all.split_off(all.len() - KEEP);
        let body: String = keep.iter().filter_map(|e| serde_json::to_string(e).ok()).map(|l| l + "\n").collect();
        let _ = std::fs::write(path, body);
        all = keep;
    }
    all.into_iter().rev().take(limit).collect()
}

pub fn clear() {
    if let Some(path) = FILE.lock().unwrap().as_ref() {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn logs_actions_without_the_typed_text() {
        let f = std::env::temp_dir().join(format!("journal-{}.jsonl", uuid::Uuid::new_v4()));
        set_file(Some(f.clone()));
        add("computer", &json!({ "action": "type", "text": "mot-de-passe-secret" }), "done");
        add("run_command", &json!({ "command": "npm  test" }), "denied");
        add("clipboard", &json!({ "action": "write", "text": "confidentiel" }), "done");
        let l = list(10);
        assert_eq!(l.len(), 3);
        assert_eq!(l[0].tool, "clipboard");
        assert!(l.iter().all(|e| !e.summary.contains("secret") && !e.summary.contains("confidentiel")));
        assert_eq!(l[1].summary, "npm test");
        assert_eq!(l[2].summary, "type (19 caractères)");
        assert!(is_logged("delete_path") && is_logged("mcp__x__y") && !is_logged("read_file"));
        clear();
        assert!(list(10).is_empty());
        set_file(None);
        let _ = std::fs::remove_file(f);
    }
}
