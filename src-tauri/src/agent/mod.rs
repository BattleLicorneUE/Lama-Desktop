pub mod background;
pub mod fsops;
pub mod project;
pub mod sandbox;
pub mod tools;

mod approval;
mod calls;
mod definitions;
mod meta;
mod prompt;
#[cfg(test)]
mod tests;
mod trim;

pub use approval::needs_approval;
pub use prompt::{context_report, ContextReport};
use definitions::{file_tool_definitions, meta_definitions, tool_root};
use calls::handle_call;
use meta::run_meta;
use prompt::system_prompt;
use trim::*;

use crate::sync::MutexExt;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
};

use futures_util::future::{join_all, BoxFuture};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// Ce dont la boucle agent a besoin de l'application : émettre des événements et mémoriser.
/// Découplé de Tauri pour pouvoir tester la boucle sans fenêtre.
pub trait Host: Send + Sync + 'static {
    fn emit_json(&self, event: &str, payload: Value);
    fn remember(&self, text: &str) -> Result<(), String>;
    /// Mémorise pour un projet (par défaut : mémoire générale).
    fn remember_in(&self, text: &str, _project: Option<&str>) -> Result<(), String> {
        self.remember(text)
    }
    /// Saisit des éléments du coffre dans la fenêtre active (demande déjà résolue et acceptée par l'utilisateur).
    fn fill(&self, _resolved: &Value) -> impl std::future::Future<Output = Result<String, String>> + Send {
        async { Err("Le coffre n'est pas disponible.".to_string()) }
    }
    /// Cherche dans les conversations passées.
    fn search_chats(&self, _query: &str) -> Result<String, String> {
        Err("Recherche dans les conversations indisponible.".into())
    }
    /// Notification Windows (sans effet dans les tests).
    fn notify(&self, _title: &str, _body: &str) {}
}

impl<R: Runtime> Host for AppHandle<R> {
    fn emit_json(&self, event: &str, payload: Value) {
        let _ = self.emit(event, payload);
    }

    fn remember(&self, text: &str) -> Result<(), String> {
        let db = self.state::<std::sync::Mutex<crate::history::History>>();
        let saved = db.lock().map_err(|e| e.to_string())?.add_memory(text).map_err(|e| e.to_string());
        saved.map(|_| ())
    }

    fn remember_in(&self, text: &str, project: Option<&str>) -> Result<(), String> {
        let db = self.state::<std::sync::Mutex<crate::history::History>>();
        let saved = db.lock().map_err(|e| e.to_string())?.add_memory_in(text, project).map_err(|e| e.to_string());
        saved.map(|_| ())
    }

    fn fill(&self, resolved: &Value) -> impl std::future::Future<Output = Result<String, String>> + Send {
        let app = self.clone();
        let resolved = resolved.clone();
        async move { crate::vault::fill_flow(&app, &resolved).await }
    }

    fn search_chats(&self, query: &str) -> Result<String, String> {
        let db = self.state::<std::sync::Mutex<crate::history::History>>();
        let found = db.lock().map_err(|e| e.to_string())?.search_chats(query, 6).map_err(|e| e.to_string())?;
        Ok(crate::docsearch::chats_report(query, &found))
    }

    fn notify(&self, title: &str, body: &str) {
        use tauri_plugin_notification::NotificationExt;
        let _ = self.notification().builder().title(title).body(body).show();
    }
}
use tokio::sync::{oneshot, Mutex};

use crate::agents::{self, AgentProfile};
use crate::mcp::McpHub;
use crate::skills::{self, Skill};
use crate::system;
use crate::ollama::{ChatMessage, OllamaClient, OllamaError};
use tools::{FsOp, Prepared};

/// Garde-fou contre une boucle sans fin : très au-dessus de ce qu'une vraie tâche demande (le bouton Stop reste le recours habituel).
const MAX_STEPS: usize = 1000;

/// Ce que l'agent a fait pendant la tâche en cours : sert à relancer la vérification s'il l'oublie.
#[derive(Default)]
struct Work {
    modified: bool,
    /// `Some(true)` : dernière vérification réussie ; `None` : pas de vérification depuis la dernière modification.
    verified: Option<bool>,
    nudges: u8,
}

/// Une modification de l'agent, avec de quoi la défaire.
pub enum Change {
    /// Contenu avant modification ; `None` si le fichier n'existait pas.
    Text { path: PathBuf, before: Option<String> },
    NewDir(PathBuf),
    Moved { from: PathBuf, to: PathBuf },
    /// Envoyé à la corbeille : à restaurer depuis la corbeille Windows.
    Trashed(PathBuf),
}

#[derive(Default)]
pub struct AgentState {
    pub approvals: Mutex<HashMap<String, oneshot::Sender<bool>>>,
    pub changes: Mutex<Vec<Change>>,
    pub background: background::Registry,
    /// Images (captures d'écran) à joindre à la conversation après les résultats d'outils du tour.
    pub pending_images: std::sync::Mutex<Vec<String>>,
    /// Précisions de l'utilisateur envoyées pendant que l'agent travaille : lues à la prochaine étape, sans interrompre la tâche.
    pub inbox: std::sync::Mutex<HashMap<String, Vec<String>>>,
    /// Dossiers dont l'utilisateur a accepté de lancer le code (scripts, tests) en mode Auto, pour cette session.
    trusted: std::sync::Mutex<std::collections::HashSet<PathBuf>>,
    work: std::sync::Mutex<Work>,
}

/// Un fichier ou dossier touché par l'agent, avec les différences par rapport à l'état d'origine.
#[derive(serde::Serialize)]
pub struct ChangeInfo {
    pub path: String,
    /// created, modified, deleted, dir, moved, trashed
    pub kind: &'static str,
    pub diff: Option<String>,
}

impl AgentState {
    /// Bilan des modifications de la session : un seul élément par fichier, différences depuis l'original.
    pub async fn changes_report(&self) -> Vec<ChangeInfo> {
        let changes = self.changes.lock().await;
        let mut out: Vec<ChangeInfo> = Vec::new();
        let mut seen: Vec<&PathBuf> = Vec::new();
        for c in changes.iter() {
            match c {
                Change::Text { path, before } => {
                    if seen.contains(&path) {
                        continue;
                    }
                    seen.push(path);
                    let after = std::fs::read_to_string(path).ok();
                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    let (kind, diff) = match (before, &after) {
                        (None, Some(a)) => ("created", Some(tools::unified_diff(&name, "", a))),
                        (Some(b), Some(a)) if b == a => continue,
                        (Some(b), Some(a)) => ("modified", Some(tools::unified_diff(&name, b, a))),
                        (_, None) => ("deleted", None),
                    };
                    out.push(ChangeInfo { path: path.to_string_lossy().into_owned(), kind, diff });
                }
                Change::NewDir(p) => out.push(ChangeInfo { path: p.to_string_lossy().into_owned(), kind: "dir", diff: None }),
                Change::Moved { from, to } => out.push(ChangeInfo { path: format!("{} → {}", from.display(), to.display()), kind: "moved", diff: None }),
                Change::Trashed(p) => out.push(ChangeInfo { path: p.to_string_lossy().into_owned(), kind: "trashed", diff: None }),
            }
        }
        out
    }

    /// Annule la dernière modification ; renvoie le chemin restauré.
    pub async fn undo_last(&self) -> Result<String, String> {
        let change = self.changes.lock().await.pop().ok_or("Aucune modification à annuler")?;
        let fail = |e: std::io::Error| format!("Annulation impossible : {e}");
        match &change {
            Change::Text { path, before: Some(text) } => std::fs::write(path, text).map_err(fail)?,
            Change::Text { path, before: None } => std::fs::remove_file(path).map_err(fail)?,
            // `remove_dir` refuse un dossier non vide : rien n'est perdu.
            Change::NewDir(path) => std::fs::remove_dir(path).map_err(fail)?,
            Change::Moved { from, to } => std::fs::rename(to, from).map_err(fail)?,
            Change::Trashed(path) => {
                return Err(format!("« {} » est dans la corbeille Windows : restaure-le depuis là.", path.display()));
            }
        }
        Ok(match change {
            Change::Text { path, .. } | Change::NewDir(path) | Change::Trashed(path) => path,
            Change::Moved { from, .. } => from,
        }
        .to_string_lossy()
        .into_owned())
    }
}

pub struct RunParams {
    pub request_id: String,
    pub model: String,
    pub messages: Vec<ChatMessage>,
    /// `None` : chat avec outils (web, skills, mémoire) sans accès aux fichiers.
    pub workspace: Option<PathBuf>,
    pub mode: String,
    pub options: Option<Value>,
    pub skills: Vec<Skill>,
    pub web: bool,
    /// Agent actif : consigne de rôle, outils autorisés, skills.
    pub profile: Option<AgentProfile>,
    /// Agents auxquels l'outil `delegate` peut confier une tâche (vide pour un sous-agent).
    pub agents: Vec<AgentProfile>,
    /// 0 pour l'agent principal, 1 pour un sous-agent (qui ne délègue pas à son tour).
    pub depth: u8,
    /// Fichiers joints à la conversation (nom, contenu) : l'outil `read_attachment` en lit des passages.
    pub attachments: Vec<(String, String)>,
    /// Documents du projet ouvert (nom, contenu) : l'outil `search_project_docs` y cherche des passages.
    pub docs: Vec<(String, String)>,
    /// Projet de la conversation : la mémoire peut lui être propre.
    pub project_id: Option<String>,
}

fn agent_is_main<H: Host>(ctx: &Ctx<'_, H>) -> bool {
    ctx.agent.is_none()
}

struct Ctx<'a, H: Host> {
    app: &'a H,
    client: &'a OllamaClient,
    request_id: &'a str,
    /// Nom du sous-agent qui travaille, `None` pour l'agent principal.
    agent: Option<&'a str>,
}

impl<H: Host> Ctx<'_, H> {
    #[allow(clippy::too_many_arguments)]
    fn tool_event(&self, call_id: &str, name: &str, args: &Value, state: &str, output: Option<&str>, diff: Option<&str>) {
        self.app.emit_json(
            "agent:tool",
            json!({ "id": self.request_id, "call_id": call_id, "name": name, "args": args,
                    "state": state, "output": output, "diff": diff, "agent": self.agent }),
        );
    }
}

/// Boucle agent : modèle -> appels d'outils -> résultats -> modèle, jusqu'à la réponse finale.
pub async fn run<H: Host>(
    app: H,
    client: OllamaClient,
    state: Arc<AgentState>,
    hub: Arc<McpHub>,
    p: RunParams,
) -> Result<(), OllamaError> {
    let result = run_loop(&app, &client, &state, &hub, &p, None).await;
    state.inbox.lock_safe().remove(&p.request_id);
    result?;
    app.emit_json(
        "chat:chunk",
        json!({ "id": p.request_id, "chunk": { "content": "", "thinking": "", "tool_calls": [], "done": true } }),
    );
    Ok(())
}

/// Une boucle complète ; renvoie le dernier texte du modèle. Un sous-agent (`agent` renseigné) ne diffuse
/// ni texte ni compteur de contexte : seuls ses appels d'outils apparaissent, avec son nom.
fn run_loop<'a, H: Host>(
    app: &'a H,
    client: &'a OllamaClient,
    state: &'a Arc<AgentState>,
    hub: &'a Arc<McpHub>,
    p: &'a RunParams,
    agent: Option<&'a str>,
) -> BoxFuture<'a, Result<String, OllamaError>> {
    Box::pin(async move {
        let plan = p.mode == "plan";
        let mut tool_defs = file_tool_definitions(p);
        tool_defs.extend(meta_definitions(p));
        if !plan {
            tool_defs.extend(hub.definitions());
        }
        tool_defs.retain(|d| d["function"]["name"].as_str().is_some_and(|n| agents::allows(p.profile.as_ref(), n)));
        let mut msgs = p.messages.clone();
        msgs.insert(0, ChatMessage {
            role: "system".into(),
            content: system_prompt(
                p,
                p.messages.iter().rev().find(|m| m.role == "user").map_or("", |m| m.content.as_str()),
                p.options.as_ref().and_then(|o| o["num_ctx"].as_u64()).unwrap_or(32_768) as usize,
            ),
            thinking: None, tool_calls: None, tool_name: None, images: None,
        });
        let ctx = Ctx { app, client, request_id: &p.request_id, agent };
        let mut last_text = String::new();
        let num_ctx = p.options.as_ref().and_then(|o| o["num_ctx"].as_u64()).unwrap_or(32_768) as usize;
        let mut last_prompt = 0usize;

        if agent.is_none() && p.depth == 0 {
            *state.work.lock_safe() = Work::default();
            state.inbox.lock_safe().remove(&p.request_id);
        }
        let top = agent.is_none() && p.depth == 0;
        let mut finished = false;
        // Le texte de chaque étape est séparé du précédent par une ligne vide (sinon les phrases se collent en un seul bloc).
        let mut had_text = false;
        for step in 0..MAX_STEPS {
            // Début d'une nouvelle étape : l'interface range le commentaire de la précédente avec le détail des actions.
            if top && step > 0 {
                app.emit_json("agent:step", json!({ "id": p.request_id }));
            }
            // Précisions ajoutées par l'utilisateur pendant la tâche : elles rejoignent la conversation à cette étape.
            if top {
                let incoming = state.inbox.lock_safe().remove(&p.request_id).unwrap_or_default();
                for text in incoming {
                    app.emit_json("agent:injected", json!({ "id": p.request_id, "text": text }));
                    msgs.push(ChatMessage {
                        role: "user".into(),
                        content: format!("(Précision ajoutée par l'utilisateur pendant ta tâche — tiens-en compte et poursuis, sans repartir de zéro.)\n{text}"),
                        thinking: None, tool_calls: None, tool_name: None, images: None,
                    });
                }
            }
            // Contexte presque plein : les vieux résultats d'outils (fichiers lus, sorties de commandes) sont retirés.
            let used = estimate_tokens(&msgs, &tool_defs).max(last_prompt);
            if used as f64 > TRIM_AT * num_ctx as f64 {
                let keep = if used as f64 > 0.95 * num_ctx as f64 { KEEP_RECENT_TIGHT } else { KEEP_RECENT };
                trim_tool_outputs(&mut msgs, keep);
            }
            let mut content = String::new();
            let mut calls: Vec<Value> = Vec::new();
            let mut prompt_seen = 0u64;
            let mut step_text_started = false;
            client
                .chat_stream(&p.model, &msgs, Some(tool_defs.clone()), p.options.clone(), |c| {
                    content.push_str(&c.content);
                    calls.extend(c.tool_calls.iter().cloned());
                    if c.done {
                        prompt_seen = c.prompt_tokens;
                        // Consommation de tout le monde (sous-agents compris), pour le compteur d'usage.
                        if c.prompt_tokens > 0 || c.eval_tokens > 0 {
                            app.emit_json("usage:tokens", json!({ "id": p.request_id, "prompt": c.prompt_tokens, "eval": c.eval_tokens }));
                        }
                    }
                    if agent.is_some() {
                        return;
                    }
                    if c.done && c.prompt_tokens > 0 {
                        app.emit_json("chat:usage", json!({ "id": p.request_id, "prompt_tokens": c.prompt_tokens }));
                    }
                    if !c.content.is_empty() || !c.thinking.is_empty() {
                        let mut out = c.content.clone();
                        if !c.content.trim().is_empty() && !step_text_started {
                            step_text_started = true;
                            if had_text {
                                out = format!("\n\n{}", out.trim_start());
                            }
                        }
                        app.emit_json(
                            "chat:chunk",
                            json!({ "id": p.request_id, "chunk": { "content": out, "thinking": c.thinking,
                                     "tool_calls": [], "done": false } }),
                        );
                    }
                })
                .await?;
            last_prompt = prompt_seen as usize;

            if !content.trim().is_empty() {
                last_text = content.clone();
                had_text = true;
            }
            if calls.is_empty() {
                // Une précision est arrivée pendant la réponse : le modèle la reçoit avant de conclure.
                if top && state.inbox.lock_safe().get(&p.request_id).is_some_and(|v| !v.is_empty()) {
                    msgs.push(ChatMessage { role: "assistant".into(), content, thinking: None, tool_calls: None, tool_name: None, images: None });
                    continue;
                }
                // Des fichiers modifiés sans vérification réussie : on demande de vérifier (deux fois au plus).
                let nudge = agent.is_none()
                    && p.depth == 0
                    && !plan
                    && p.workspace.as_deref().is_some_and(|w| project::command_for(w, "check").is_some())
                    && {
                        let mut w = state.work.lock_safe();
                        let due = w.modified && w.verified != Some(true) && w.nudges < 2;
                        if due {
                            w.nudges += 1;
                        }
                        due
                    };
                if nudge {
                    msgs.push(ChatMessage { role: "assistant".into(), content, thinking: None, tool_calls: None, tool_name: None, images: None });
                    msgs.push(ChatMessage {
                        role: "user".into(),
                        content: "(Rappel automatique) Tu as modifié des fichiers mais la dernière vérification n'a pas réussi ou n'a pas été lancée. Appelle verify, lis les erreurs et corrige-les ; si la vérification est impossible pour ce projet, explique pourquoi.".into(),
                        thinking: None, tool_calls: None, tool_name: None, images: None,
                    });
                    continue;
                }
                finished = true;
                break;
            }
            msgs.push(ChatMessage {
                role: "assistant".into(), content, thinking: None,
                tool_calls: Some(calls.clone()), tool_name: None, images: None,
            });

            let parsed: Vec<(String, String, Value)> = calls
                .iter()
                .map(|call| {
                    let name = call["function"]["name"].as_str().unwrap_or("").to_string();
                    let args = match &call["function"]["arguments"] {
                        Value::String(s) => serde_json::from_str(s).unwrap_or(Value::Null),
                        v => v.clone(),
                    };
                    (uuid::Uuid::new_v4().to_string(), name, args)
                })
                .collect();
            // Plusieurs délégations dans un même tour s'exécutent en parallèle ; le reste, dans l'ordre.
            let outputs: Vec<String> = if parsed.len() > 1 && parsed.iter().all(|(_, n, _)| n == "delegate") {
                join_all(parsed.iter().map(|(id, n, a)| handle_call(&ctx, state, hub, p, id, n, a))).await
            } else {
                let mut v = Vec::new();
                for (id, n, a) in &parsed {
                    v.push(handle_call(&ctx, state, hub, p, id, n, a).await);
                }
                v
            };
            for ((_, name, _), output) in parsed.into_iter().zip(outputs) {
                msgs.push(ChatMessage {
                    role: "tool".into(), content: output, thinking: None,
                    tool_calls: None, tool_name: Some(name), images: None,
                });
            }
            // Les captures d'écran voyagent dans un message « utilisateur » : c'est là que les modèles à vision lisent les images.
            let shots: Vec<String> = std::mem::take(&mut *state.pending_images.lock_safe());
            if !shots.is_empty() {
                msgs.push(ChatMessage {
                    role: "user".into(), content: "(Capture d'écran demandée par ton outil screenshot.)".into(), thinking: None,
                    tool_calls: None, tool_name: None, images: Some(shots),
                });
            }
        }
        if !finished && agent.is_none() {
            let notice = format!("\n\n⚠️ Tâche interrompue après {MAX_STEPS} étapes (garde-fou). Écris « continue » pour reprendre.");
            app.emit_json("chat:chunk", json!({ "id": p.request_id, "chunk": { "content": notice, "thinking": "", "tool_calls": [], "done": false } }));
            last_text.push_str(&notice);
        }
        Ok(last_text)
    })
}
