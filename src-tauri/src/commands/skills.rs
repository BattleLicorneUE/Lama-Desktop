//! Skills : analyse, import, édition.

use crate::*;

pub(crate) fn skills_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = profile_dir(app)?.join("skills");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

pub(crate) fn disabled_file(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(skills_dir(app)?.join("..").join("skills-disabled.json"))
}

/// Skills installés (avec leur état activé) pour l'application et, éventuellement, le projet.
pub(crate) fn scan_skills_fresh(app: &AppHandle, workspace: Option<&std::path::Path>) -> Result<Vec<skills::Skill>, String> {
    let mut all = skills::scan(&skills_dir(app)?, workspace);
    skills::apply_disabled(&mut all, &skills::load_disabled(&disabled_file(app)?));
    if let Ok(mut cache) = app.state::<AppState>().skills_cache.lock() {
        *cache = Some(SkillsCache { workspace: workspace.map(Into::into), at: std::time::Instant::now(), list: all.clone() });
    }
    Ok(all)
}

/// Skills (avec leur état activé), depuis le cache s'il est récent.
pub(crate) fn scan_skills(app: &AppHandle, workspace: Option<&std::path::Path>) -> Result<Vec<skills::Skill>, String> {
    if let Ok(cache) = app.state::<AppState>().skills_cache.lock() {
        if let Some(c) = cache.as_ref().filter(|c| c.workspace.as_deref() == workspace && c.at.elapsed() < SKILLS_CACHE_TTL) {
            return Ok(c.list.clone());
        }
    }
    scan_skills_fresh(app, workspace)
}

pub(crate) fn invalidate_skills(app: &AppHandle) {
    if let Ok(mut cache) = app.state::<AppState>().skills_cache.lock() {
        *cache = None;
    }
}

/// Poids du prompt système, des skills et des outils (en caractères) pour le panneau « Contexte ».
#[tauri::command]
pub(crate) async fn context_report(app: AppHandle, state: State<'_, AppState>, workspace: Option<String>, mode: String, web: bool, ctx: usize) -> Result<agent::ContextReport, String> {
    let workspace = workspace.filter(|w| !w.trim().is_empty()).map(std::path::PathBuf::from);
    let skills: Vec<skills::Skill> = scan_skills(&app, workspace.as_deref())?.into_iter().filter(|s| s.enabled).collect();
    let hub = state.mcp.lock().await.clone();
    Ok(agent::context_report(workspace, &mode, web, skills, ctx, &hub))
}

#[tauri::command]
pub(crate) fn skills_list(app: AppHandle, workspace: Option<String>) -> Result<Vec<skills::Skill>, String> {
    let ws = workspace.filter(|w| !w.trim().is_empty()).map(std::path::PathBuf::from);
    // La liste du panneau (bouton « Actualiser » compris) relit toujours le disque.
    scan_skills_fresh(&app, ws.as_deref())
}

#[tauri::command]
pub(crate) fn skill_set_enabled(app: AppHandle, name: String, enabled: bool) -> Result<(), String> {
    invalidate_skills(&app);
    let file = disabled_file(&app)?;
    let mut set = skills::load_disabled(&file);
    if enabled { set.remove(&name) } else { set.insert(name) };
    skills::save_disabled(&file, &set)
}

pub(crate) fn auto_imported_file(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    Ok(skills_dir(app)?.join("..").join("skills-auto-imported.json"))
}

/// Au démarrage : copie les skills de Claude Desktop et Claude Code absents de l'application.
/// Chaque skill n'est importé qu'une fois : un skill supprimé ensuite ne revient pas. Renvoie le nombre importé.
#[tauri::command]
pub(crate) async fn skills_auto_import(app: AppHandle) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut roots = Vec::new();
        if let Some(p) = std::env::var_os("USERPROFILE") {
            roots.push(std::path::PathBuf::from(p).join(".claude"));
        }
        if let Some(p) = std::env::var_os("APPDATA") {
            roots.push(std::path::PathBuf::from(p).join("Claude"));
        }
        let installed = scan_skills_fresh(&app, None)?;
        let record_file = auto_imported_file(&app)?;
        let mut record = skills::load_disabled(&record_file);
        let dest = skills_dir(&app)?;
        let mut imported = 0;
        for found in skills::discover(&roots, &installed) {
            let key = found.name.to_lowercase();
            // Déjà installé : on le note aussi, pour qu'un skill supprimé plus tard ne revienne pas.
            if found.installed || record.contains(&key) {
                record.insert(key);
                continue;
            }
            if skills::import(&found.dir, &dest) == Ok(true) {
                imported += 1;
            }
            record.insert(key);
        }
        skills::save_disabled(&record_file, &record)?;
        if imported > 0 {
            invalidate_skills(&app);
        }
        Ok(imported)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Classement skills.sh (page publique) : `all`, `trending` ou `hot`.
#[tauri::command]
pub(crate) async fn skills_hub_browse(view: String) -> Result<Vec<skills_hub::Entry>, String> {
    skills_hub::browse(&view).await
}

#[derive(serde::Serialize)]
pub(crate) struct RepoSkills {
    pub(crate) owner: String,
    pub(crate) repo: String,
    /// Skill demandé dans l'entrée (URL ou commande), s'il y en avait un.
    pub(crate) wanted: Option<String>,
    pub(crate) skills: Vec<skills::Found>,
}

/// Télécharge un dépôt GitHub (depuis une commande, une URL skills.sh/GitHub ou `owner/repo`)
/// et liste les skills qu'il contient, sans rien installer.
#[tauri::command]
pub(crate) async fn skills_hub_fetch(app: AppHandle, input: String) -> Result<RepoSkills, String> {
    let spec = skills_hub::parse_spec(&input)?;
    let dest = std::env::temp_dir().join("ollama-desktop-skills").join(format!("{}__{}", spec.owner, spec.repo));
    skills_hub::fetch_repo(&spec, &dest).await?;
    let installed = scan_skills(&app, None)?;
    let mut found = skills::discover(&[dest], &installed);
    for f in &mut found {
        f.group = format!("{}/{}", spec.owner, spec.repo);
    }
    if found.is_empty() {
        return Err(format!("Aucun skill (SKILL.md) dans {}/{}", spec.owner, spec.repo));
    }
    Ok(RepoSkills { owner: spec.owner, repo: spec.repo, wanted: spec.skill, skills: found })
}

#[derive(serde::Serialize)]
pub(crate) struct ImportReport {
    pub(crate) imported: Vec<String>,
    pub(crate) skipped: Vec<String>,
    pub(crate) errors: Vec<String>,
}

/// Copie des skills (dossiers) dans le dossier de l'application. Aucun écrasement.
#[tauri::command]
pub(crate) fn skills_import(app: AppHandle, dirs: Vec<String>) -> Result<ImportReport, String> {
    let dest = skills_dir(&app)?;
    let mut report = ImportReport { imported: vec![], skipped: vec![], errors: vec![] };
    for d in dirs {
        let path = std::path::PathBuf::from(&d);
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or(d.clone());
        match skills::import(&path, &dest) {
            Ok(true) => report.imported.push(name),
            Ok(false) => report.skipped.push(name),
            Err(e) => report.errors.push(format!("{name} : {e}")),
        }
    }
    Ok(report)
}

/// Instructions d'un skill, pour la commande `/nom` côté interface.
#[tauri::command]
pub(crate) fn skill_read(app: AppHandle, workspace: Option<String>, name: String, args: Option<String>) -> Result<String, String> {
    let ws = workspace.filter(|w| !w.trim().is_empty()).map(std::path::PathBuf::from);
    let all = scan_skills(&app, ws.as_deref())?;
    let skill = all.iter().find(|s| s.name.eq_ignore_ascii_case(&name)).ok_or("Skill introuvable")?;
    skills::read_body(skill, args.as_deref())
}

#[tauri::command]
pub(crate) fn skills_open_dir(app: AppHandle) -> Result<String, String> {
    let dir = skills_dir(&app)?;
    std::process::Command::new("explorer").arg(&dir).spawn().map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().into_owned())
}

/// Un skill ouvert dans l'éditeur : son contenu, sa portée et son dossier.
#[derive(serde::Serialize)]
pub(crate) struct SkillEdit {
    pub(crate) draft: skills::Draft,
    pub(crate) scope: String,
    pub(crate) dir: String,
}

pub(crate) fn find_skill(app: &AppHandle, workspace: Option<String>, name: &str) -> Result<skills::Skill, String> {
    let ws = workspace.filter(|w| !w.trim().is_empty()).map(std::path::PathBuf::from);
    scan_skills_fresh(app, ws.as_deref())?.into_iter().find(|s| s.name.eq_ignore_ascii_case(name)).ok_or_else(|| "Skill introuvable.".to_string())
}

/// Contenu d'un skill pour l'éditeur.
#[tauri::command]
pub(crate) fn skill_get(app: AppHandle, workspace: Option<String>, name: String) -> Result<SkillEdit, String> {
    let s = find_skill(&app, workspace, &name)?;
    Ok(SkillEdit { draft: skills::read_draft(&s.dir)?, scope: s.scope, dir: s.dir.to_string_lossy().into_owned() })
}

/// Crée ou modifie un skill (`original` : le nom du skill modifié). `scope` : « app » (tous les projets) ou « project » (`.skills/` du dossier de travail).
#[tauri::command]
pub(crate) fn skill_save(app: AppHandle, workspace: Option<String>, draft: skills::Draft, scope: String, original: Option<String>) -> Result<String, String> {
    let existing = match original.as_deref().filter(|n| !n.trim().is_empty()) {
        Some(n) => Some(find_skill(&app, workspace.clone(), n)?),
        None => None,
    };
    let root = match &existing {
        Some(s) => s.dir.parent().map(std::path::Path::to_path_buf).ok_or("Dossier du skill introuvable.")?,
        None if scope == "project" => {
            let ws = workspace.filter(|w| !w.trim().is_empty()).ok_or("Ouvre d'abord un dossier de travail pour créer un skill de projet.")?;
            std::path::PathBuf::from(ws).join(".skills")
        }
        None => skills_dir(&app)?,
    };
    let dir = skills::save(&root, &draft, existing.as_ref().map(|s| s.dir.as_path()))?;
    invalidate_skills(&app);
    Ok(dir.to_string_lossy().into_owned())
}

/// Supprime un skill (son dossier entier).
#[tauri::command]
pub(crate) fn skill_delete(app: AppHandle, workspace: Option<String>, name: String) -> Result<(), String> {
    let s = find_skill(&app, workspace, &name)?;
    let root = s.dir.parent().map(std::path::Path::to_path_buf).ok_or("Dossier du skill introuvable.")?;
    skills::delete(&root, &s.dir)?;
    invalidate_skills(&app);
    Ok(())
}
