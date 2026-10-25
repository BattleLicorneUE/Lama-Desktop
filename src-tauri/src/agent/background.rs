//! Commandes longues lancées en arrière-plan (serveur de dev, surveillance de fichiers…) :
//! l'agent les démarre sans attendre, lit leur sortie plus tard et peut les arrêter.

use crate::sync::MutexExt;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tokio::sync::Notify;

use super::tools::{pump, truncate_tail};

const MAX_RUNNING: usize = 8;
/// Au-delà de cette taille, la sortie conservée est ramenée à sa fin.
const KEEP_OUTPUT: usize = 100_000;

struct Proc {
    command: String,
    pid: Option<u32>,
    started_ms: u64,
    output: Mutex<String>,
    /// `None` tant que la commande tourne, puis son code de sortie (`-1` si inconnu).
    code: Mutex<Option<i32>>,
    stopped_by_user: Mutex<bool>,
    stop: Notify,
}

#[derive(Serialize, Clone)]
pub struct ProcInfo {
    pub id: String,
    pub command: String,
    pub running: bool,
    pub code: Option<i32>,
    pub started_ms: u64,
    /// Fin de la sortie (quelques lignes) pour l'affichage.
    pub tail: String,
}

#[derive(Default)]
pub struct Registry {
    procs: Mutex<HashMap<String, Arc<Proc>>>,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// Arrête une commande et tous ses sous-processus (`npm run dev` en lance plusieurs).
fn kill_tree(pid: u32) {
    use std::os::windows::process::CommandExt;
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(0x0800_0000)
        .status();
}

fn tail_of(text: &str, max: usize) -> String {
    let t = text.trim_end();
    if t.len() <= max {
        return t.to_string();
    }
    let mut start = t.len() - max;
    while !t.is_char_boundary(start) {
        start += 1;
    }
    t[start..].to_string()
}

impl Registry {
    fn get(&self, id: &str) -> Result<Arc<Proc>, String> {
        self.procs.lock_safe().get(id).cloned().ok_or_else(|| format!("Aucune commande d'id « {id} »."))
    }

    /// Lance `command` dans `root` et renvoie son identifiant.
    pub fn start(&self, root: &Path, command: &str) -> Result<String, String> {
        let running = self.procs.lock_safe().values().filter(|p| p.code.lock_safe().is_none()).count();
        if running >= MAX_RUNNING {
            return Err(format!("Déjà {MAX_RUNNING} commandes en arrière-plan : arrête-en une avec stop_command."));
        }
        let mut child = tokio::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", command])
            .current_dir(root)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .creation_flags(0x0800_0000)
            .spawn()
            .map_err(|e| format!("Impossible de lancer la commande : {e}"))?;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        pump(child.stdout.take().ok_or("stdout indisponible")?, tx.clone());
        pump(child.stderr.take().ok_or("stderr indisponible")?, tx);

        let proc = Arc::new(Proc {
            command: command.to_string(),
            pid: child.id(),
            started_ms: now_ms(),
            output: Mutex::new(String::new()),
            code: Mutex::new(None),
            stopped_by_user: Mutex::new(false),
            stop: Notify::new(),
        });
        let id = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
        self.procs.lock_safe().insert(id.clone(), proc.clone());

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    line = rx.recv() => match line {
                        Some(l) => {
                            let mut out = proc.output.lock_safe();
                            out.push_str(&l);
                            out.push('\n');
                            if out.len() > 2 * KEEP_OUTPUT {
                                *out = truncate_tail(std::mem::take(&mut *out), KEEP_OUTPUT);
                            }
                        }
                        None => break,
                    },
                    _ = proc.stop.notified() => {
                        if let Some(pid) = proc.pid {
                            kill_tree(pid);
                        }
                        break;
                    }
                }
            }
            let code = child.wait().await.ok().and_then(|s| s.code()).unwrap_or(-1);
            *proc.code.lock_safe() = Some(code);
        });
        Ok(id)
    }

    /// État et fin de la sortie d'une commande.
    pub fn output(&self, id: &str, max: usize) -> Result<String, String> {
        let p = self.get(id)?;
        let state = match *p.code.lock_safe() {
            None => "en cours".to_string(),
            Some(c) if *p.stopped_by_user.lock_safe() => format!("arrêtée (code {c})"),
            Some(c) => format!("terminée (code {c})"),
        };
        let out = tail_of(&p.output.lock_safe(), max);
        Ok(format!("Commande « {} » : {state}.\n{}", p.command, if out.is_empty() { "(aucune sortie pour l'instant)".into() } else { out }))
    }

    pub fn stop(&self, id: &str) -> Result<(), String> {
        let p = self.get(id)?;
        if p.code.lock_safe().is_some() {
            return Ok(());
        }
        *p.stopped_by_user.lock_safe() = true;
        p.stop.notify_one();
        Ok(())
    }

    /// Arrête tout de suite (fermeture de l'application, changement de profil) : sans attendre les tâches asynchrones.
    pub fn stop_all(&self) {
        for p in self.procs.lock_safe().values() {
            if p.code.lock_safe().is_none() {
                if let Some(pid) = p.pid {
                    kill_tree(pid);
                }
                *p.stopped_by_user.lock_safe() = true;
            }
        }
    }

    pub fn list(&self) -> Vec<ProcInfo> {
        let mut v: Vec<ProcInfo> = self
            .procs
            .lock()
            .unwrap()
            .iter()
            .map(|(id, p)| {
                // Une seule prise du verrou : le reverrouiller dans la même expression bloquerait pour toujours.
                let code = *p.code.lock_safe();
                ProcInfo {
                    id: id.clone(),
                    command: p.command.clone(),
                    running: code.is_none(),
                    code,
                    started_ms: p.started_ms,
                    tail: tail_of(&p.output.lock_safe(), 600),
                }
            })
            .collect();
        v.sort_by_key(|p| std::cmp::Reverse(p.started_ms));
        v
    }

    /// Oublie les commandes terminées.
    pub fn clear_finished(&self) {
        self.procs.lock_safe().retain(|_, p| p.code.lock_safe().is_none());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn wait_until(mut f: impl FnMut() -> bool) -> bool {
        for _ in 0..80 {
            if f() {
                return true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        false
    }

    #[tokio::test]
    async fn short_command_is_captured_and_finishes_with_its_code() {
        let reg = Registry::default();
        let id = reg.start(&std::env::temp_dir(), "Write-Output bonjour; exit 3").unwrap();
        assert!(wait_until(|| reg.list().iter().any(|p| !p.running)).await, "doit se terminer");
        let out = reg.output(&id, 1000).unwrap();
        assert!(out.contains("bonjour") && out.contains("terminée (code 3)"), "{out}");
    }

    #[tokio::test]
    async fn long_command_runs_then_stops_with_its_children() {
        let reg = Registry::default();
        let id = reg.start(&std::env::temp_dir(), "Write-Output demarre; Start-Sleep -Seconds 60").unwrap();
        assert!(wait_until(|| reg.output(&id, 1000).unwrap().contains("demarre")).await);
        assert!(reg.output(&id, 1000).unwrap().contains("en cours"));
        assert!(reg.list()[0].running);
        reg.stop(&id).unwrap();
        assert!(wait_until(|| !reg.list()[0].running).await, "doit s'arrêter");
        assert!(reg.output(&id, 1000).unwrap().contains("arrêtée"));
        reg.clear_finished();
        assert!(reg.list().is_empty());
        assert!(reg.output("inconnu", 10).is_err() && reg.stop("inconnu").is_err());
    }

    #[test]
    fn tail_keeps_the_end_on_char_boundaries() {
        assert_eq!(tail_of("abcdef\n", 3), "def");
        assert_eq!(tail_of("é".repeat(10).as_str(), 5).chars().count(), 2);
        assert_eq!(tail_of("court", 100), "court");
    }
}
