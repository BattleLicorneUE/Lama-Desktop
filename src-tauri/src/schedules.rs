//! Tâches planifiées : une consigne lancée toute seule à heure fixe (« chaque matin à 8 h, résume mes nouveautés »).
//! Les tâches vivent dans le profil ; l'interface calcule la prochaine échéance (heure locale), ce module la conserve
//! et signale les tâches échues tant que l'application est ouverte. Une tâche manquée est lancée à la réouverture.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub prompt: String,
    /// « chat » ou « code » (une tâche de code travaille dans `workspace`).
    pub kind: String,
    pub workspace: Option<String>,
    pub project_id: Option<String>,
    /// once, hourly, daily, weekdays ou weekly.
    pub repeat: String,
    /// Heure de lancement « HH:MM » (minute seulement pour hourly).
    pub time: String,
    /// Jour de la semaine pour weekly : 0 = lundi … 6 = dimanche.
    pub weekday: u8,
    /// Date « AAAA-MM-JJ » pour once.
    pub date: Option<String>,
    pub enabled: bool,
    /// Prochaine échéance, en secondes Unix ; `None` : rien à lancer (terminée, ou en cours de lancement).
    pub next_at: Option<u64>,
    pub last_at: Option<u64>,
    /// Dernière conversation créée par la tâche.
    pub last_conv: Option<String>,
}

static FILE: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn set_file(path: Option<PathBuf>) {
    *FILE.lock().unwrap() = path;
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn with<T>(f: impl FnOnce(&mut Vec<Task>) -> T) -> Result<T, String> {
    let guard = FILE.lock().unwrap();
    let path = guard.as_ref().ok_or("Aucun profil ouvert.")?;
    let mut list: Vec<Task> = std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let out = f(&mut list);
    let text = serde_json::to_string_pretty(&list).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())?;
    Ok(out)
}

pub fn list() -> Result<Vec<Task>, String> {
    with(|l| l.clone())
}

/// Ajoute la tâche (identifiant vide) ou remplace celle qui porte le même identifiant.
pub fn save(mut task: Task) -> Result<Task, String> {
    task.name = task.name.trim().to_string();
    task.prompt = task.prompt.trim().to_string();
    if task.name.is_empty() {
        return Err("Donne un nom à la tâche.".into());
    }
    if task.prompt.is_empty() {
        return Err("Écris la consigne à lancer.".into());
    }
    if !matches!(task.repeat.as_str(), "once" | "hourly" | "daily" | "weekdays" | "weekly") {
        return Err("Répétition inconnue.".into());
    }
    if task.kind != "code" {
        task.kind = "chat".into();
    }
    if task.kind == "code" && task.workspace.as_deref().is_none_or(|w| !std::path::Path::new(w).is_dir()) {
        return Err("Choisis un dossier de travail existant pour une tâche de code.".into());
    }
    if task.id.is_empty() {
        task.id = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
    }
    let saved = task.clone();
    with(|l| match l.iter_mut().find(|t| t.id == task.id) {
        Some(t) => *t = task,
        None => l.push(task),
    })?;
    Ok(saved)
}

pub fn delete(id: &str) -> Result<(), String> {
    with(|l| l.retain(|t| t.id != id))
}

/// Retire de la file les tâches échues (leur échéance est effacée tant que l'interface ne les a pas relancées) et les renvoie.
pub fn take_due() -> Vec<Task> {
    let t = now();
    with(|l| {
        let mut due = Vec::new();
        for task in l.iter_mut().filter(|x| x.enabled && x.next_at.is_some_and(|n| n <= t)) {
            task.next_at = None;
            task.last_at = Some(t);
            due.push(task.clone());
        }
        due
    })
    .unwrap_or_default()
}

/// L'interface a lancé la tâche : nouvelle échéance (ou aucune) et conversation créée.
pub fn done(id: &str, next_at: Option<u64>, conv: Option<String>) -> Result<(), String> {
    with(|l| {
        if let Some(t) = l.iter_mut().find(|t| t.id == id) {
            t.next_at = next_at;
            if conv.is_some() {
                t.last_conv = conv;
            }
            if next_at.is_none() && t.repeat == "once" {
                t.enabled = false;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(name: &str, next: Option<u64>) -> Task {
        Task { name: name.into(), prompt: "Résume mes nouveautés".into(), kind: "chat".into(), repeat: "daily".into(), time: "08:00".into(), enabled: true, next_at: next, ..Default::default() }
    }

    #[test]
    fn save_validates_lists_fires_once_and_reschedules() {
        let f = std::env::temp_dir().join(format!("sched-{}.json", uuid::Uuid::new_v4()));
        set_file(Some(f.clone()));
        assert!(save(task("", None)).unwrap_err().contains("nom"));
        let mut bad = task("x", None);
        bad.repeat = "jamais".into();
        assert!(save(bad).is_err());
        let mut code = task("code", None);
        code.kind = "code".into();
        assert!(save(code).unwrap_err().contains("dossier"));

        let due = save(task("Matin", Some(1))).unwrap();
        let later = save(task("Soir", Some(now() + 3600))).unwrap();
        assert_eq!(due.id.len(), 8);
        assert_eq!(list().unwrap().len(), 2);

        let fired = take_due();
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].id, due.id);
        assert!(take_due().is_empty(), "une tâche échue ne doit sonner qu'une fois");

        done(&due.id, Some(now() + 86_400), Some("conv1".into())).unwrap();
        let all = list().unwrap();
        let morning = all.iter().find(|t| t.id == due.id).unwrap();
        assert!(morning.next_at.unwrap() > now() && morning.last_conv.as_deref() == Some("conv1") && morning.enabled);

        let mut edited = later.clone();
        edited.name = "Soirée".into();
        save(edited).unwrap();
        assert_eq!(list().unwrap().len(), 2);

        let mut once = task("Unique", Some(1));
        once.repeat = "once".into();
        let once = save(once).unwrap();
        take_due();
        done(&once.id, None, None).unwrap();
        assert!(!list().unwrap().iter().find(|t| t.id == once.id).unwrap().enabled);

        delete(&due.id).unwrap();
        assert_eq!(list().unwrap().len(), 2);
        set_file(None);
        assert!(list().is_err());
        let _ = std::fs::remove_file(f);
    }
}
