//! Outils « méta » : mémoire, skills, tâches, recherche dans les conversations, documents du projet, pièces jointes…

use super::*;

pub(super) async fn run_meta<H: Host>(ctx: &Ctx<'_, H>, state: &AgentState, p: &RunParams, name: &str, args: &Value) -> Result<String, String> {
    let arg = |k: &str| args.get(k).and_then(Value::as_str).ok_or_else(|| format!("Argument manquant : {k}"));
    match name {
        "use_skill" => {
            let wanted = arg("name")?;
            let skill = p
                .skills
                .iter()
                .find(|s| s.name.eq_ignore_ascii_case(wanted))
                .ok_or_else(|| format!("Skill inconnu : {wanted}"))?;
            if !skill.model_invocable {
                return Err(format!("Le skill « {wanted} » est réservé à l'utilisateur (commande /{wanted})."));
            }
            match args.get("file").and_then(Value::as_str) {
                Some(f) => skills::read_file(skill, f),
                None => skills::read_body(skill, args.get("args").and_then(Value::as_str)),
            }
        }
        "search_skills" => Ok(skills::search_report(&p.skills, arg("query")?)),
        "update_tasks" => {
            let tasks = args.get("tasks").and_then(Value::as_array).ok_or("Argument manquant : tasks")?;
            let clean: Vec<Value> = tasks
                .iter()
                .filter_map(|t| {
                    let content = t.get("content")?.as_str()?.trim();
                    let status = t.get("status").and_then(Value::as_str).unwrap_or("pending");
                    let status = if matches!(status, "pending" | "in_progress" | "completed") { status } else { "pending" };
                    (!content.is_empty()).then(|| json!({ "content": content, "status": status }))
                })
                .collect();
            ctx.app.emit_json("agent:tasks", json!({ "id": ctx.request_id, "tasks": clean }));
            Ok(format!("Liste de tâches mise à jour ({} tâches).", clean.len()))
        }
        "read_attachment" => read_attachment(&p.attachments, &args),
        "search_project_docs" => {
            let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(4) as usize;
            Ok(crate::docsearch::search(&p.docs, arg("query")?, limit))
        }
        "search_chats" => ctx.app.search_chats(arg("query")?),
        "vault_list" => crate::vault::global().list_for_agent(),
        "vault_fill" => ctx.app.fill(args).await,
        "vault_login" => crate::vault::browser_login(args).await,
        "browser_open" if system::enabled() => crate::browser::open(arg("url")?).await,
        "browser_read" if system::enabled() => crate::browser::read().await,
        "browser_click" if system::enabled() => crate::browser::click(args.get("index").and_then(Value::as_u64).ok_or("Argument manquant : index")? as u32).await,
        "browser_type" if system::enabled() => {
            let index = args.get("index").and_then(Value::as_u64).ok_or("Argument manquant : index")? as u32;
            crate::browser::type_text(index, arg("text")?, args.get("submit").and_then(Value::as_bool).unwrap_or(false)).await
        }
        "browser_close" => crate::browser::close().await,
        "web_search" if p.web => crate::web::search(arg("query")?).await,
        "web_fetch" if p.web => crate::web::fetch(arg("url")?).await,
        "create_document" => {
            if !system::enabled() {
                return Err("L'accès à l'ordinateur est désactivé (Réglages → Ordinateur).".into());
            }
            let (title, content) = (arg("title")?.trim().to_string(), arg("content")?.to_string());
            let ext = arg("format")?.to_lowercase();
            if !matches!(ext.as_str(), "pdf" | "docx" | "xlsx" | "csv" | "html" | "md") {
                return Err(format!("Format inconnu : {ext} (pdf, docx, xlsx, csv, html, md)"));
            }
            let stem: String = args.get("filename").and_then(Value::as_str).unwrap_or(&title).chars()
                .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' { c } else { ' ' }).collect();
            let stem = stem.split_whitespace().collect::<Vec<_>>().join(" ");
            let stem = if stem.is_empty() { "document".to_string() } else { stem.chars().take(60).collect() };
            let home = std::env::var("USERPROFILE").map_err(|_| "Dossier personnel introuvable.".to_string())?;
            let dir = PathBuf::from(home).join("Documents").join("Lama Desktop");
            // Ne jamais écraser un document existant : « nom (2).pdf ».
            let mut path = dir.join(format!("{stem}.{ext}"));
            let mut n = 2;
            while path.exists() {
                path = dir.join(format!("{stem} ({n}).{ext}"));
                n += 1;
            }
            let out = path.clone();
            let opts = crate::docs::DocOpts {
                subtitle: args.get("subtitle").and_then(Value::as_str).map(String::from),
                toc: args.get("toc").and_then(Value::as_bool),
            };
            tokio::task::spawn_blocking(move || crate::export::write_with(&path, &title, &content, &opts)).await.map_err(|e| e.to_string())??;
            Ok(format!("Document créé : {}", out.display()))
        }
        "notify" => {
            let message = arg("message")?.trim().to_string();
            if message.is_empty() {
                return Err("Message vide.".into());
            }
            let title = args.get("title").and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()).unwrap_or("Lama").to_string();
            let minutes = args.get("in_minutes").and_then(Value::as_f64).unwrap_or(0.0).clamp(0.0, 60.0 * 24.0 * 30.0);
            if minutes < 0.05 {
                ctx.app.notify(&title, &message);
                Ok("Notification envoyée.".into())
            } else {
                let r = crate::reminders::add(&title, &message, (minutes * 60.0).round() as u64)?;
                Ok(format!("Rappel programmé : {}. Il ne sonne que si Lama Desktop est ouvert à ce moment (sinon à la prochaine ouverture).", crate::reminders::describe(&r)))
            }
        }
        "reminders" => match arg("action")? {
            "list" => {
                let l = crate::reminders::list()?;
                Ok(if l.is_empty() { "Aucun rappel en attente.".into() } else { l.iter().map(crate::reminders::describe).collect::<Vec<_>>().join("\n") })
            }
            "cancel" => {
                let id = arg("id")?;
                if crate::reminders::cancel(id)? { Ok("Rappel annulé.".into()) } else { Err(format!("Rappel introuvable : {id}")) }
            }
            other => Err(format!("Action inconnue : {other} (list ou cancel)")),
        },
        "system_info" | "open_target" | "clipboard" | "pc_control" | "screenshot" | "computer" | "find_files" if !system::enabled() => {
            Err("L'accès à l'ordinateur est désactivé (Réglages → Ordinateur).".into())
        }
        "system_info" => {
            let section = args.get("section").and_then(Value::as_str).unwrap_or("tout").to_string();
            tokio::task::spawn_blocking(move || system::info(&section)).await.map_err(|e| e.to_string())?
        }
        "open_target" => {
            let target = arg("target")?.to_string();
            tokio::task::spawn_blocking(move || system::open(&target)).await.map_err(|e| e.to_string())?
        }
        "clipboard" => {
            let (action, text) = (arg("action")?.to_string(), args.get("text").and_then(Value::as_str).map(String::from));
            tokio::task::spawn_blocking(move || system::clipboard(&action, text.as_deref())).await.map_err(|e| e.to_string())?
        }
        "pc_control" => {
            let action = arg("action")?.to_string();
            let (value, on) = (args.get("value").and_then(Value::as_i64), args.get("on").and_then(Value::as_bool));
            tokio::task::spawn_blocking(move || system::control(&action, value, on)).await.map_err(|e| e.to_string())?
        }
        "screenshot" => {
            let (png, text) = tokio::task::spawn_blocking(system::screenshot).await.map_err(|e| e.to_string())??;
            state.pending_images.lock_safe().push(png);
            Ok(text)
        }
        "computer" => {
            let action = arg("action")?.to_string();
            let num = |k: &str| args.get(k).and_then(Value::as_f64);
            let (x, y, amount) = (num("x"), num("y"), args.get("amount").and_then(Value::as_i64));
            let text = args.get("text").and_then(Value::as_str).map(String::from);
            let keys = args.get("keys").and_then(Value::as_str).map(String::from);
            tokio::task::spawn_blocking(move || system::computer(&action, x, y, text.as_deref(), keys.as_deref(), amount)).await.map_err(|e| e.to_string())?
        }
        "find_files" => {
            let mut roots = sandbox::extra_roots();
            if let Some(w) = &p.workspace {
                roots.insert(0, w.clone());
            }
            let q = arg("query")?.to_string();
            tokio::task::spawn_blocking(move || system::find_files(&roots, &q)).await.map_err(|e| e.to_string())?
        }
        "remember" => {
            let text = arg("text")?.trim();
            if text.is_empty() {
                return Err("Texte vide".into());
            }
            let project = if args.get("scope").and_then(Value::as_str) == Some("project") { p.project_id.as_deref() } else { None };
            ctx.app.remember_in(text, project).map(|()| if project.is_some() { "Mémorisé pour ce projet.".to_string() } else { "Mémorisé.".to_string() })
        }
        other => Err(format!("Outil indisponible : {other}")),
    }
}

/// Passage d'un fichier joint : recherche par mot, ou tranche de caractères.
pub(super) fn read_attachment(list: &[(String, String)], args: &Value) -> Result<String, String> {
    const CONTEXT: usize = 400;
    let wanted = args.get("name").and_then(Value::as_str).unwrap_or("").trim().to_lowercase();
    let (name, content) = if list.len() == 1 || wanted.is_empty() {
        list.first()
    } else {
        list.iter().find(|(n, _)| n.to_lowercase() == wanted).or_else(|| list.iter().find(|(n, _)| n.to_lowercase().contains(&wanted)))
    }
    .ok_or_else(|| format!("Fichier introuvable : {wanted}. Fichiers joints : {}", list.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ")))?;
    let chars: Vec<char> = content.chars().collect();
    let total = chars.len();
    if let Some(q) = args.get("search").and_then(Value::as_str).map(str::trim).filter(|q| !q.is_empty()) {
        let fold = |c: char| c.to_lowercase().next().unwrap_or(c);
        let low: Vec<char> = chars.iter().map(|&c| fold(c)).collect();
        let needle: Vec<char> = q.chars().map(fold).collect();
        let mut out = Vec::new();
        let mut i = 0;
        while i + needle.len() <= low.len() && out.len() < 5 {
            if low[i..i + needle.len()] == needle[..] {
                let (a, b) = (i.saturating_sub(CONTEXT), (i + needle.len() + CONTEXT).min(total));
                out.push(format!("— position {i} —\n{}", chars[a..b].iter().collect::<String>()));
                i = b;
            } else {
                i += 1;
            }
        }
        return Ok(if out.is_empty() { format!("« {q} » n'apparaît pas dans {name} ({total} caractères).") } else { format!("[{name}] {} passage(s) pour « {q} » :\n\n{}", out.len(), out.join("\n\n")) });
    }
    let start = args.get("start").and_then(Value::as_u64).unwrap_or(0) as usize;
    let length = (args.get("length").and_then(Value::as_u64).unwrap_or(6000) as usize).clamp(1, 12_000);
    if start >= total {
        return Err(format!("Position {start} au-delà de la fin de {name} ({total} caractères)."));
    }
    let end = (start + length).min(total);
    let more = if end < total { format!("\n[Suite : start = {end}]") } else { "\n[Fin du fichier]".to_string() };
    Ok(format!("[{name} — caractères {start} à {end} sur {total}]\n{}{more}", chars[start..end].iter().collect::<String>()))
}
