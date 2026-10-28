//! Profils d'agents : des fichiers `.md` avec un en-tête (nom, description, modèle, outils, permissions, skills)
//! et une consigne. Même format que les agents de Claude Code ; trois sources, la plus proche l'emporte :
//! intégrés < dossier de l'application < `.agents/` du projet.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::skills::{field_of, split_front};

#[derive(Debug, Clone, Serialize)]
pub struct AgentProfile {
    pub name: String,
    pub description: String,
    /// Consigne de rôle, ajoutée en tête du prompt système.
    pub prompt: String,
    pub model: Option<String>,
    /// Outils autorisés (noms de l'app) ; `None` = tous.
    pub tools: Option<Vec<String>>,
    /// Mode de permission imposé (`plan`, …) ; `None` = celui choisi par l'utilisateur.
    pub permission: Option<String>,
    /// Skills accessibles ; `None` = tous.
    pub skills: Option<Vec<String>>,
    /// "builtin", "app" ou "project".
    pub scope: String,
    pub file: Option<PathBuf>,
}

const READ_TOOLS: [&str; 3] = ["read_file", "list_dir", "grep"];

fn builtin(name: &str, description: &str, tools: Option<&[&str]>, prompt: &str) -> AgentProfile {
    AgentProfile {
        name: name.into(),
        description: description.into(),
        prompt: prompt.into(),
        model: None,
        tools: tools.map(|t| t.iter().map(|s| s.to_string()).collect()),
        permission: None,
        skills: None,
        scope: "builtin".into(),
        file: None,
    }
}

/// Agents fournis avec l'application.
pub fn builtins() -> Vec<AgentProfile> {
    let mut read_web: Vec<&str> = READ_TOOLS.to_vec();
    read_web.extend(["web_search", "web_fetch"]);
    let mut tester: Vec<&str> = READ_TOOLS.to_vec();
    tester.extend(["write_file", "edit_file", "make_dir", "run_command", "verify"]);
    vec![
        builtin(
            "explorateur",
            "Explore le code en lecture seule et résume ce qu'il trouve (structure, où se trouve quoi, comment ça marche).",
            Some(&READ_TOOLS),
            "Tu es un explorateur de code. Tu lis, cherches et comprends, sans jamais modifier quoi que ce soit. Réponds par un résumé court et précis : chemins de fichiers, fonctions clés, comment les pièces s'articulent. Ne recopie pas de longs extraits.",
        ),
        builtin(
            "relecteur",
            "Relit du code comme un relecteur exigeant : bugs, cas limites, sécurité, lisibilité. Lecture seule.",
            Some(&READ_TOOLS),
            "Tu es un relecteur de code exigeant. Lis les fichiers concernés puis liste les problèmes par ordre de gravité (bugs, cas limites, sécurité, lisibilité, code mort), chacun avec le fichier, la ligne et une correction proposée. Ne modifie rien. Si tout est correct, dis-le simplement.",
        ),
        builtin(
            "architecte",
            "Conçoit un plan d'implémentation détaillé avant de coder. Lecture seule, recherche web possible.",
            Some(&read_web),
            "Tu es un architecte logiciel. Explore le projet, puis propose un plan d'implémentation par étapes : fichiers à créer ou modifier, structures de données, risques et alternatives, ordre de réalisation, comment vérifier. Ne modifie rien.",
        ),
        builtin(
            "chercheur",
            "Cherche sur le web une question précise et rapporte des faits datés avec leurs sources (recherche approfondie).",
            Some(&["web_search", "web_fetch"]),
            "Tu es un chercheur méticuleux. On te confie une question précise : fais plusieurs recherches web avec des formulations différentes, ouvre les pages les plus fiables (sources officielles, presse reconnue, études, documentation) avec web_fetch et relève les faits utiles. Réponds en français par une synthèse courte : des faits précis (chiffres, dates, noms), chacun suivi de son URL ; signale les contradictions entre sources et ce que tu n'as pas trouvé. N'invente rien et ne cite jamais une page que tu n'as pas lue.",
        ),
        builtin(
            "testeur",
            "Écrit et exécute des tests, puis corrige ce qui échoue.",
            Some(&tester),
            "Tu es un ingénieur de test. Identifie le comportement à couvrir, écris des tests clairs (cas nominaux, limites, erreurs) dans le style du projet, exécute-les avec verify, et corrige jusqu'à ce que tout passe. Ne change pas le code de production sans le signaler.",
        ),
        builtin(
            "debogueur",
            "Trouve la cause racine d'un bug : reproduit, isole, corrige et vérifie.",
            None,
            "Tu es un débogueur méthodique. Reproduis le problème, formule des hypothèses, isole la cause racine (pas seulement le symptôme), applique la correction minimale puis vérifie avec verify. Explique la cause en une ou deux phrases.",
        ),
    ]
}

/// Convertit un nom d'outil de Claude (Read, Bash…) ou de l'app en noms d'outils de l'app.
fn canonical(tool: &str) -> Vec<String> {
    let t = tool.trim().trim_matches(['"', '\'', '[', ']']).to_lowercase();
    let mapped: &[&str] = match t.as_str() {
        "read" | "notebookread" => &["read_file"],
        "grep" => &["grep"],
        "glob" | "ls" => &["list_dir"],
        "write" => &["write_file", "make_dir"],
        "edit" | "multiedit" | "notebookedit" => &["edit_file"],
        "bash" | "powershell" | "shell" => &["run_command", "verify"],
        "webfetch" => &["web_fetch"],
        "websearch" => &["web_search"],
        "task" | "agent" => &["delegate"],
        "" => &[],
        _ => return vec![t],
    };
    mapped.iter().map(|s| s.to_string()).collect()
}

fn list_of(head: &str, key: &str, f: impl Fn(&str) -> Vec<String>) -> Option<Vec<String>> {
    let v = field_of(head, key)?;
    let mut out: Vec<String> = Vec::new();
    for part in v.split(',') {
        for item in f(part) {
            if !out.contains(&item) {
                out.push(item);
            }
        }
    }
    Some(out)
}

/// Mode de permission d'un agent Claude → mode de l'app.
fn permission_of(v: &str) -> Option<String> {
    match v.trim().to_lowercase().as_str() {
        "plan" => Some("plan".into()),
        "default" | "manual" => Some("manual".into()),
        "acceptedits" | "accept-edits" => Some("accept-edits".into()),
        _ => None,
    }
}

pub fn parse(text: &str, fallback_name: &str, scope: &str, file: Option<PathBuf>) -> AgentProfile {
    let (head, body) = split_front(text).unwrap_or(("", text));
    AgentProfile {
        name: field_of(head, "name").unwrap_or_else(|| fallback_name.to_string()),
        description: field_of(head, "description").unwrap_or_default(),
        prompt: body.trim().to_string(),
        model: field_of(head, "model").filter(|m| !matches!(m.as_str(), "inherit" | "sonnet" | "opus" | "haiku")),
        tools: list_of(head, "tools", canonical),
        permission: field_of(head, "permissionMode").as_deref().and_then(permission_of),
        skills: list_of(head, "skills", |s| vec![s.trim().to_string()]),
        scope: scope.into(),
        file,
    }
}

fn scan_dir(dir: &Path, scope: &str, out: &mut Vec<AgentProfile>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.filter_map(Result::ok) {
        let path = e.path();
        if path.extension().is_none_or(|x| x != "md") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        out.push(parse(&text, &stem, scope, Some(path)));
    }
}

/// Tous les agents disponibles, triés par nom ; un agent plus proche remplace un homonyme.
pub fn scan(app_dir: &Path, workspace: Option<&Path>) -> Vec<AgentProfile> {
    let mut all = builtins();
    for (dir, scope) in [(Some(app_dir.to_path_buf()), "app"), (workspace.map(|w| w.join(".agents")), "project")] {
        let Some(dir) = dir else { continue };
        let mut found = Vec::new();
        scan_dir(&dir, scope, &mut found);
        all.retain(|a| !found.iter().any(|f| f.name.eq_ignore_ascii_case(&a.name)));
        all.extend(found);
    }
    all.sort_by_key(|a| a.name.to_lowercase());
    all
}

pub fn find<'a>(agents: &'a [AgentProfile], name: &str) -> Option<&'a AgentProfile> {
    agents.iter().find(|a| a.name.eq_ignore_ascii_case(name))
}

/// Un agent peut-il utiliser cet outil ? Les outils de gestion (tâches, skills) restent toujours permis.
pub fn allows(profile: Option<&AgentProfile>, tool: &str) -> bool {
    let Some(list) = profile.and_then(|p| p.tools.as_ref()) else { return true };
    matches!(tool, "update_tasks" | "use_skill" | "search_skills" | "read_attachment" | "search_project_docs" | "search_chats")
        || list.iter().any(|t| t == tool || (t == "mcp" && tool.starts_with("mcp__")))
}

/// Mode effectif : un agent peut restreindre (plan) mais jamais élargir les permissions choisies.
pub fn effective_mode(user: &str, profile: Option<&AgentProfile>) -> String {
    match profile.and_then(|p| p.permission.as_deref()) {
        Some("plan") => "plan".into(),
        _ => user.into(),
    }
}

/// Ce que l'utilisateur écrit dans l'éditeur d'agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Draft {
    pub name: String,
    pub description: String,
    pub prompt: String,
    /// Modèle imposé ; vide = celui de la conversation.
    #[serde(default)]
    pub model: String,
    /// Outils autorisés ; `None` = tous.
    #[serde(default)]
    pub tools: Option<Vec<String>>,
}

const MAX_NAME: usize = 48;
const MAX_DESCRIPTION: usize = 500;
const MAX_PROMPT: usize = 20_000;
const RESERVED: &[&str] = &["con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9"];

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Nom de fichier et d'agent : minuscules, chiffres et tirets.
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().to_lowercase().chars() {
        let c = if c.is_alphanumeric() { c } else { '-' };
        if c == '-' && (out.is_empty() || out.ends_with('-')) {
            continue;
        }
        out.push(c);
    }
    out.trim_matches('-').to_string()
}

pub fn validate(d: &Draft) -> Result<String, String> {
    let slug = slugify(&d.name);
    if slug.is_empty() {
        return Err("Donne un nom à l'agent (lettres, chiffres et tirets).".into());
    }
    if slug.chars().count() > MAX_NAME {
        return Err(format!("Le nom est trop long ({MAX_NAME} caractères au plus)."));
    }
    if RESERVED.contains(&slug.as_str()) {
        return Err(format!("« {slug} » est un nom réservé par Windows : choisis-en un autre."));
    }
    let desc = one_line(&d.description);
    if desc.chars().count() < 8 {
        return Err("Explique en une phrase quand confier une tâche à cet agent : c'est ce qui permet à Lama de le choisir.".into());
    }
    if desc.chars().count() > MAX_DESCRIPTION {
        return Err(format!("La description est trop longue ({MAX_DESCRIPTION} caractères au plus)."));
    }
    let prompt = d.prompt.trim();
    if prompt.chars().count() < 10 {
        return Err("Écris la consigne de l'agent : son rôle et sa méthode.".into());
    }
    if prompt.len() > MAX_PROMPT {
        return Err(format!("La consigne est trop longue ({} Ko au plus).", MAX_PROMPT / 1000));
    }
    if d.model.trim().chars().any(|c| c.is_control() || c == '\n') || d.model.len() > 120 {
        return Err("Nom de modèle invalide.".into());
    }
    if let Some(tools) = &d.tools {
        if tools.len() > 60 || tools.iter().any(|t| t.is_empty() || t.len() > 60 || !t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')) {
            return Err("Liste d'outils invalide.".into());
        }
    }
    Ok(slug)
}

/// Contenu du fichier `.md` de l'agent.
pub fn render(d: &Draft, slug: &str) -> String {
    let mut s = format!("---\nname: {slug}\ndescription: >-\n  {}\n", one_line(&d.description));
    let model = d.model.trim();
    if !model.is_empty() {
        s.push_str(&format!("model: {model}\n"));
    }
    if let Some(tools) = &d.tools {
        s.push_str(&format!("tools: {}\n", tools.join(", ")));
    }
    s.push_str(&format!("---\n{}\n", d.prompt.replace("\r\n", "\n").trim()));
    s
}

pub fn draft_of(p: &AgentProfile) -> Draft {
    Draft { name: p.name.clone(), description: p.description.clone(), prompt: p.prompt.clone(), model: p.model.clone().unwrap_or_default(), tools: p.tools.clone() }
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// Enregistre un agent dans `dir`. `original` : le fichier de l'agent modifié (renommé si le nom change).
pub fn save(dir: &Path, d: &Draft, original: Option<&Path>) -> Result<PathBuf, String> {
    let slug = validate(d)?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{slug}.md"));
    if path.exists() && !original.is_some_and(|o| same_file(o, &path)) {
        return Err(format!("Un agent « {slug} » existe déjà : choisis un autre nom, ou modifie celui-là."));
    }
    if let Some(o) = original {
        if !same_file(o, &path) {
            if o.parent().map(|p| same_file(p, dir)) != Some(true) {
                return Err("Cet agent ne peut pas être renommé d'ici.".into());
            }
            let _ = std::fs::remove_file(o);
        }
    }
    let tmp = dir.join(format!("{slug}.md.tmp"));
    std::fs::write(&tmp, render(d, &slug)).map_err(|e| format!("Écriture impossible : {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("Écriture impossible : {e}"))?;
    Ok(path)
}

/// Supprime un agent : seulement un fichier `.md` placé directement dans `dir`.
pub fn delete(dir: &Path, file: &Path) -> Result<(), String> {
    if file.parent().map(|p| same_file(p, dir)) != Some(true) || file.extension().is_none_or(|x| x != "md") {
        return Err("Ce fichier n'est pas un agent de la liste.".into());
    }
    std::fs::remove_file(file).map_err(|e| format!("Suppression impossible : {e}"))
}

/// Copie les agents de Claude Code (`~/.claude/agents/*.md`) absents de `dest`. Renvoie le nombre copié.
pub fn import_from(src: &Path, dest: &Path) -> Result<usize, String> {
    let Ok(entries) = std::fs::read_dir(src) else { return Ok(0) };
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let mut n = 0;
    for e in entries.filter_map(Result::ok) {
        let path = e.path();
        let target = dest.join(e.file_name());
        if path.extension().is_some_and(|x| x == "md") && !target.exists() {
            std::fs::copy(&path, &target).map_err(|e| e.to_string())?;
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("agents-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn researcher_is_web_only() {
        let all = builtins();
        let r = find(&all, "chercheur").expect("agent chercheur");
        assert!(allows(Some(r), "web_search") && allows(Some(r), "web_fetch"));
        assert!(!allows(Some(r), "write_file") && !allows(Some(r), "run_command") && !allows(Some(r), "read_file"));
    }

    #[test]
    fn parses_yaml_list_forms_of_tools() {
        let block = parse("---
name: a
description: d
tools:
  - Read
  - Grep
  - Bash
model: m
---
corps", "x", "app", None);
        assert_eq!(block.tools.as_deref().unwrap(), ["read_file", "grep", "run_command", "verify"]);
        assert_eq!(block.model.as_deref(), Some("m"));
        let flow = parse("---
name: b
description: d
tools: [Read, Grep]
---
corps", "x", "app", None);
        assert_eq!(flow.tools.as_deref().unwrap(), ["read_file", "grep"]);
    }

    #[test]
    fn parses_claude_agent_format_and_maps_tools() {
        let a = parse(
            "---\nname: reviewer\ndescription: Relit le code\nmodel: gpt-oss:120b\ntools: Read, Grep, Bash\npermissionMode: plan\nskills: pdf, docx\n---\nTu es un relecteur.",
            "x",
            "app",
            None,
        );
        assert_eq!(a.name, "reviewer");
        assert_eq!(a.model.as_deref(), Some("gpt-oss:120b"));
        assert_eq!(a.tools.as_deref().unwrap(), ["read_file", "grep", "run_command", "verify"]);
        assert_eq!(a.permission.as_deref(), Some("plan"));
        assert_eq!(a.skills.as_deref().unwrap(), ["pdf", "docx"]);
        assert_eq!(a.prompt, "Tu es un relecteur.");
        let b = parse("---\ndescription: d\nmodel: sonnet\n---\nx", "fichier", "app", None);
        assert_eq!(b.name, "fichier");
        assert!(b.model.is_none(), "les alias de modèles Claude ne sont pas des modèles Ollama");
        assert!(b.tools.is_none());
    }

    #[test]
    fn scan_merges_sources_with_closest_winning() {
        let app = tmp();
        let ws = tmp();
        std::fs::write(app.join("relecteur.md"), "---\nname: relecteur\ndescription: perso\n---\nA").unwrap();
        std::fs::create_dir_all(ws.join(".agents")).unwrap();
        std::fs::write(ws.join(".agents").join("relecteur.md"), "---\nname: relecteur\ndescription: projet\n---\nB").unwrap();
        std::fs::write(ws.join(".agents").join("autre.md"), "---\ndescription: autre\n---\nC").unwrap();
        let all = scan(&app, Some(&ws));
        assert_eq!(all.iter().filter(|a| a.name == "relecteur").count(), 1);
        assert_eq!(find(&all, "RELECTEUR").unwrap().description, "projet");
        assert!(find(&all, "autre").is_some() && find(&all, "explorateur").is_some());
        assert_eq!(find(&scan(&app, None), "relecteur").unwrap().description, "perso");
    }

    #[test]
    fn allowlist_and_modes_can_only_restrict() {
        let ro = find(&builtins(), "relecteur").cloned().unwrap();
        assert!(allows(Some(&ro), "read_file") && allows(Some(&ro), "use_skill"));
        assert!(!allows(Some(&ro), "write_file") && !allows(Some(&ro), "mcp__x__y"));
        assert!(allows(None, "write_file"));
        let mut plan = ro.clone();
        plan.permission = Some("plan".into());
        assert_eq!(effective_mode("bypass", Some(&plan)), "plan");
        assert_eq!(effective_mode("bypass", Some(&ro)), "bypass");
        assert_eq!(effective_mode("auto", None), "auto");
    }

    fn draft() -> Draft {
        Draft {
            name: "Relecteur sécurité".into(),
            description: "Utilise-le quand on veut une relecture : failles, secrets oubliés, entrées non vérifiées.".into(),
            prompt: "Tu relis le code à la recherche de failles. Réponds par une liste classée par gravité.".into(),
            model: String::new(),
            tools: Some(vec!["read_file".into(), "list_dir".into(), "grep".into()]),
        }
    }

    #[test]
    fn a_saved_agent_is_read_back_by_the_scanner() {
        let d = tmp();
        let path = save(&d, &draft(), None).unwrap();
        assert!(path.ends_with("relecteur-sécurité.md"));
        let all = scan(&d, None);
        let a = find(&all, "relecteur-sécurité").unwrap();
        assert_eq!(a.description, draft().description);
        assert_eq!(a.prompt, draft().prompt);
        assert_eq!(a.tools.as_deref().unwrap(), ["read_file", "list_dir", "grep"]);
        assert!(a.model.is_none());
        assert_eq!(draft_of(a).tools, draft().tools);
        let with_model = Draft { model: "gpt-oss:120b".into(), tools: None, ..draft() };
        save(&d, &with_model, Some(&path)).unwrap();
        let again = scan(&d, None);
        let a = find(&again, "relecteur-sécurité").unwrap();
        assert_eq!(a.model.as_deref(), Some("gpt-oss:120b"));
        assert!(a.tools.is_none());
    }

    #[test]
    fn saving_never_overwrites_another_agent_and_renaming_moves_the_file() {
        let d = tmp();
        let path = save(&d, &draft(), None).unwrap();
        assert!(save(&d, &draft(), None).unwrap_err().contains("existe déjà"));
        let other = save(&d, &Draft { name: "Autre".into(), ..draft() }, None).unwrap();
        assert!(save(&d, &Draft { name: "relecteur sécurité".into(), ..draft() }, Some(&other)).unwrap_err().contains("existe déjà"));
        assert!(other.exists() && path.exists());
        let renamed = save(&d, &Draft { name: "Relecture".into(), ..draft() }, Some(&path)).unwrap();
        assert!(renamed.exists() && !path.exists());
    }

    #[test]
    fn incomplete_agents_are_refused_and_only_listed_files_are_deleted() {
        for (bad, hint) in [
            (Draft { name: "".into(), ..draft() }, "nom"),
            (Draft { name: "nul".into(), ..draft() }, "réservé"),
            (Draft { description: "court".into(), ..draft() }, "quand confier"),
            (Draft { prompt: " ".into(), ..draft() }, "consigne"),
            (Draft { tools: Some(vec!["rm -rf".into()]), ..draft() }, "outils"),
        ] {
            let e = validate(&bad).unwrap_err();
            assert!(e.to_lowercase().contains(hint), "{e} doit contenir « {hint} »");
        }
        let d = tmp();
        let path = save(&d, &draft(), None).unwrap();
        let outside = tmp().join("x.md");
        std::fs::write(&outside, "x").unwrap();
        assert!(delete(&d, &outside).is_err() && outside.exists());
        assert!(delete(&d, &d.join("notes.txt")).is_err());
        delete(&d, &path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn import_skips_existing_agents() {
        let d = tmp();
        std::fs::write(d.join("mon-agent.md"), "x").unwrap();
        let src = tmp();
        std::fs::write(src.join("a.md"), "x").unwrap();
        std::fs::write(src.join("mon-agent.md"), "y").unwrap();
        std::fs::write(src.join("notes.txt"), "z").unwrap();
        assert_eq!(import_from(&src, &d).unwrap(), 1);
        assert_eq!(import_from(&src, &d).unwrap(), 0);
        assert_eq!(import_from(&d.join("absent"), &d).unwrap(), 0);
    }
}
