//! Exécution d'un appel d'outil (approbation, annulation) et délégation à un sous-agent.

use super::*;

/// Confie une tâche à un sous-agent isolé (contexte vide) et renvoie son résumé.
pub(super) async fn delegate<H: Host>(
    ctx: &Ctx<'_, H>,
    state: &Arc<AgentState>,
    hub: &Arc<McpHub>,
    p: &RunParams,
    args: &Value,
) -> Result<String, String> {
    const MAX_REPORT: usize = 8_000;
    let wanted = args.get("agent").and_then(Value::as_str).ok_or("Argument manquant : agent")?;
    let task = args.get("task").and_then(Value::as_str).filter(|t| !t.trim().is_empty()).ok_or("Argument manquant : task")?;
    if p.depth > 0 {
        return Err("Un sous-agent ne peut pas déléguer à son tour.".into());
    }
    let profile = agents::find(&p.agents, wanted).ok_or_else(|| format!("Agent inconnu : {wanted}"))?;
    let skills = match &profile.skills {
        Some(names) => p.skills.iter().filter(|s| names.iter().any(|n| n.eq_ignore_ascii_case(&s.name))).cloned().collect(),
        None => p.skills.clone(),
    };
    let sub = RunParams {
        request_id: p.request_id.clone(),
        model: profile.model.clone().unwrap_or_else(|| p.model.clone()),
        messages: vec![ChatMessage { role: "user".into(), content: task.to_string(), thinking: None, tool_calls: None, tool_name: None, images: None }],
        workspace: p.workspace.clone(),
        mode: agents::effective_mode(&p.mode, Some(profile)),
        options: p.options.clone(),
        skills,
        web: p.web,
        profile: Some(profile.clone()),
        agents: vec![],
        depth: 1,
        attachments: vec![],
        docs: vec![],
        project_id: None,
    };
    let mut report = run_loop(ctx.app, ctx.client, state, hub, &sub, Some(&profile.name)).await.map_err(|e| e.to_string())?;
    if report.trim().is_empty() {
        report = "(l'agent n'a rien répondu)".into();
    }
    if report.len() > MAX_REPORT {
        let cut = (0..=MAX_REPORT).rev().find(|i| report.is_char_boundary(*i)).unwrap_or(0);
        report.truncate(cut);
        report.push_str("\n[… résumé tronqué …]");
    }
    Ok(report)
}

/// Exécute un appel d'outil (avec approbation si nécessaire) et renvoie le texte pour le modèle.
pub(super) async fn handle_call<H: Host>(
    ctx: &Ctx<'_, H>,
    state: &Arc<AgentState>,
    hub: &Arc<McpHub>,
    p: &RunParams,
    call_id: &str,
    name: &str,
    args: &Value,
) -> String {
    let fail = |msg: String| {
        ctx.tool_event(call_id, name, args, "error", Some(&msg), None);
        format!("Erreur : {msg}")
    };
    if !agents::allows(p.profile.as_ref(), name) {
        return fail(format!("Outil « {name} » non autorisé pour cet agent."));
    }
    if matches!(name, "command_output" | "stop_command") {
        let id = args.get("id").and_then(Value::as_str).unwrap_or("");
        let result = if name == "stop_command" {
            state.background.stop(id).map(|()| format!("Arrêt demandé pour la commande {id}."))
        } else {
            state.background.output(id, 8_000)
        };
        return match result {
            Ok(text) => {
                ctx.tool_event(call_id, name, args, "done", Some(&text), None);
                text
            }
            Err(e) => fail(e),
        };
    }
    if name == "delegate" {
        ctx.tool_event(call_id, name, args, "running", None, None);
        return match delegate(ctx, state, hub, p, args).await {
            Ok(report) => {
                ctx.tool_event(call_id, name, args, "done", Some(&report), None);
                report
            }
            Err(e) => fail(e),
        };
    }

    // Coffre : la demande du modèle (noms approximatifs) est remplacée par la demande exacte, affichée à l'utilisateur telle quelle.
    let resolved_args;
    let args: &Value = if name == "vault_fill" {
        match crate::vault::global().resolve_fill(args) {
            Ok(v) => {
                resolved_args = v;
                &resolved_args
            }
            Err(e) => return fail(e),
        }
    } else if name == "vault_login" {
        // Connexion : on montre à l'utilisateur l'entrée exacte ET la page actuellement ouverte, avec le verdict du contrôle d'adresse.
        match crate::vault::global().resolve_login(args) {
            Ok(mut v) => {
                let page = crate::browser::page_info().await;
                let verdict = match &page {
                    Some(p) => crate::browser::host_allowed(v["url"].as_str().unwrap_or(""), &p.url),
                    None => Err("Le navigateur de Lama n'est pas ouvert (browser_open d'abord).".to_string()),
                };
                v["page"] = page.map(|p| p.to_json()).unwrap_or(Value::Null);
                v["allowed"] = verdict.as_ref().map(|_| true).unwrap_or(false).into();
                v["why"] = verdict.err().unwrap_or_default().into();
                resolved_args = v;
                &resolved_args
            }
            Err(e) => return fail(e),
        }
    } else {
        args
    };

    let prepared = if let Some((server, tool)) = hub.resolve(name) {
        if p.mode == "plan" {
            return fail("Mode Plan : outils externes désactivés.".into());
        }
        Prepared::Mcp { server, tool, args: args.clone() }
    } else {
        if p.mode == "plan" && matches!(name, "write_file" | "edit_file" | "run_command" | "verify" | "make_dir" | "move_path" | "delete_path" | "stop_command" | "open_target" | "clipboard" | "pc_control" | "computer" | "create_document" | "read_attachment" | "vault_fill" | "vault_login" | "browser_open" | "browser_click" | "browser_type") {
            return fail("Mode Plan : modifications interdites.".into());
        }
        if matches!(name, "use_skill" | "search_skills" | "remember" | "web_search" | "web_fetch" | "update_tasks" | "notify" | "reminders" | "system_info" | "open_target" | "clipboard" | "pc_control" | "screenshot" | "computer" | "find_files" | "create_document" | "read_attachment" | "search_project_docs" | "search_chats" | "vault_list" | "vault_fill" | "vault_login" | "browser_open" | "browser_read" | "browser_click" | "browser_type" | "browser_close") {
            Prepared::Meta { name: name.to_string(), args: args.clone() }
        } else {
            if p.workspace.is_none() && matches!(name, "verify" | "run_command" | "command_output" | "stop_command") {
                return fail("Outil indisponible sans dossier de travail.".into());
            }
            let Some(root) = tool_root(p) else { return fail("Outil indisponible sans dossier de travail.".into()) };
            match tools::prepare(&root, name, args) {
                Ok(x) => x,
                Err(e) => return fail(e),
            }
        }
    };

    let diff = if let Prepared::Write { diff, .. } = &prepared { Some(diff.clone()) } else { None };

    // Mode Auto : lancer les tests ou la compilation exécute du code du dossier ; on demande une fois par dossier.
    let root = tool_root(p);
    let untrusted_code = p.mode == "auto"
        && matches!(&prepared, Prepared::Exec { command } | Prepared::Spawn { command } if tools::runs_project_code(command))
        && root.as_ref().is_some_and(|r| !state.trusted.lock_safe().contains(r));
    if needs_approval(&p.mode, &prepared) || untrusted_code {
        let (tx, rx) = oneshot::channel();
        state.approvals.lock().await.insert(call_id.to_string(), tx);
        ctx.tool_event(call_id, name, args, "awaiting", None, diff.as_deref());
        if !rx.await.unwrap_or(false) {
            ctx.tool_event(call_id, name, args, "denied", None, diff.as_deref());
            if crate::journal::is_logged(name) {
                crate::journal::add(name, args, "denied");
            }
            return "L'utilisateur a refusé cette action.".into();
        }
        if untrusted_code {
            if let Some(r) = root {
                state.trusted.lock_safe().insert(r);
            }
        }
    }
    ctx.tool_event(call_id, name, args, "running", None, diff.as_deref());

    let result: Result<String, String> = match prepared {
        Prepared::Done(text) => Ok(text),
        Prepared::Write { path, before, after, .. } => {
            let write = || -> std::io::Result<()> {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&path, &after)
            };
            match write() {
                Ok(()) => {
                    state.changes.lock().await.push(Change::Text { path: path.clone(), before });
                    Ok(format!("Fichier écrit : {}", path.display()))
                }
                Err(e) => Err(format!("Écriture impossible : {e}")),
            }
        }
        Prepared::Fs(op) => {
            let (done, change, msg) = match op {
                FsOp::MakeDir(path) => (
                    std::fs::create_dir_all(&path).map_err(|e| format!("Création impossible : {e}")),
                    Change::NewDir(path.clone()),
                    format!("Dossier créé : {}", path.display()),
                ),
                FsOp::Delete(path) => (fsops::to_trash(&path), Change::Trashed(path.clone()), format!("Envoyé à la corbeille : {}", path.display())),
                FsOp::Move { from, to } => {
                    let msg = format!("Déplacé : {} -> {}", from.display(), to.display());
                    (fsops::do_move(&from, &to), Change::Moved { from, to }, msg)
                }
            };
            match done {
                Ok(()) => {
                    state.changes.lock().await.push(change);
                    Ok(msg)
                }
                Err(e) => Err(e),
            }
        }
        Prepared::Meta { name: n, args: a } => run_meta(ctx, state, p, &n, &a).await,
        Prepared::Exec { command } => {
            let root = p.workspace.as_deref().unwrap_or(std::path::Path::new("."));
            tools::exec_stream(root, &command, |live| {
                ctx.tool_event(call_id, name, args, "running", Some(live), None);
            })
            .await
        }
        Prepared::Spawn { command } => {
            let root = p.workspace.as_deref().unwrap_or(std::path::Path::new("."));
            match state.background.start(root, &command) {
                Ok(id) => {
                    // Laisse le temps de voir si elle démarre ou échoue aussitôt (port occupé, commande inconnue…).
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                    let first = state.background.output(&id, 2_000).unwrap_or_default();
                    Ok(format!("Lancée en arrière-plan (id {id}). Utilise command_output pour la suivre et stop_command pour l'arrêter.\n{first}"))
                }
                Err(e) => Err(e),
            }
        }
        Prepared::Mcp { server, tool, args } => hub.call(&server, &tool, args).await,
    };

    if crate::journal::is_logged(name) {
        crate::journal::add(name, args, if result.is_ok() { "done" } else { "error" });
    }
    if agent_is_main(ctx) {
        let mut w = state.work.lock_safe();
        match (&result, name) {
            (Ok(_), "write_file" | "edit_file" | "make_dir" | "move_path" | "delete_path") => {
                w.modified = true;
                w.verified = None;
            }
            (Ok(t), "verify") => w.verified = Some(t.contains("[code de sortie : 0]")),
            (Err(_), "verify") => w.verified = Some(false),
            _ => {}
        }
    }
    match result {
        Ok(text) => {
            ctx.tool_event(call_id, name, args, "done", Some(&text), diff.as_deref());
            text
        }
        Err(e) => fail(e),
    }
}
