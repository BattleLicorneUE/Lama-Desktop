//! Rappels programmés par le modèle (« préviens-moi dans 20 minutes »), conservés dans le profil et vérifiés toutes les
//! quelques secondes tant que l'application est ouverte. Un rappel manqué (appli fermée) sonne à la réouverture.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Reminder {
    pub id: String,
    pub title: String,
    pub body: String,
    /// Échéance, en secondes Unix.
    pub at: u64,
}

/// Fichier du profil ouvert ; une seule opération à la fois sur ce fichier.
static FILE: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn set_file(path: Option<PathBuf>) {
    *FILE.lock().unwrap() = path;
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Lit, modifie puis réécrit la liste sous verrou.
fn with<T>(f: impl FnOnce(&mut Vec<Reminder>) -> T) -> Result<T, String> {
    let guard = FILE.lock().unwrap();
    let path = guard.as_ref().ok_or("Aucun profil ouvert.")?;
    let mut list: Vec<Reminder> = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let out = f(&mut list);
    let text = serde_json::to_string_pretty(&list).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())?;
    Ok(out)
}

pub fn add(title: &str, body: &str, delay_secs: u64) -> Result<Reminder, String> {
    let r = Reminder { id: uuid::Uuid::new_v4().simple().to_string()[..8].to_string(), title: title.into(), body: body.into(), at: now() + delay_secs };
    let saved = r.clone();
    with(|l| l.push(r))?;
    Ok(saved)
}

pub fn list() -> Result<Vec<Reminder>, String> {
    with(|l| {
        l.sort_by_key(|r| r.at);
        l.clone()
    })
}

pub fn cancel(id: &str) -> Result<bool, String> {
    with(|l| {
        let before = l.len();
        l.retain(|r| r.id != id);
        l.len() != before
    })
}

/// Retire et renvoie les rappels échus.
pub fn take_due() -> Vec<Reminder> {
    let t = now();
    with(|l| {
        let (due, rest): (Vec<_>, Vec<_>) = l.drain(..).partition(|r| r.at <= t);
        *l = rest;
        due
    })
    .unwrap_or_default()
}

/// « dans 20 min », « à 14:05 » : texte lisible de l'échéance.
pub fn describe(r: &Reminder) -> String {
    let left = r.at.saturating_sub(now());
    let when = if left < 90 {
        format!("dans {left} s")
    } else if left < 5400 {
        format!("dans {} min", left.div_ceil(60))
    } else {
        format!("dans {} h {:02} min", left / 3600, (left % 3600) / 60)
    };
    format!("[{}] {} — {}", r.id, r.title, when)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_list_cancel_and_due() {
        let f = std::env::temp_dir().join(format!("rem-{}.json", uuid::Uuid::new_v4()));
        set_file(Some(f.clone()));
        let soon = add("Thé", "Sors le thé", 0).unwrap();
        let later = add("Réunion", "À 15 h", 3600).unwrap();
        assert_eq!(list().unwrap().len(), 2);
        let due = take_due();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, soon.id);
        assert_eq!(list().unwrap().len(), 1);
        assert!(!cancel("inconnu").unwrap());
        assert!(cancel(&later.id).unwrap());
        assert!(list().unwrap().is_empty());
        assert!(describe(&later).contains("Réunion"));
        set_file(None);
        assert!(list().is_err());
        let _ = std::fs::remove_file(f);
    }
}
