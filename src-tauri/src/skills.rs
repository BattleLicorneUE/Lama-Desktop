//! Skills : dossiers contenant un `SKILL.md` (même format que Claude).
//! Seuls le nom et la description sont exposés au modèle ; le contenu est chargé à la demande.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

const MAX_BODY: usize = 30_000;

#[derive(Debug, Clone, Serialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub dir: PathBuf,
    /// "app" (dossier utilisateur) ou "project" (`.skills/` du projet).
    pub scope: String,
    pub enabled: bool,
    /// Indication des arguments attendus par `/nom` (champ `argument-hint`).
    pub argument_hint: Option<String>,
    /// Le modèle peut-il le choisir seul ? Faux avec `disable-model-invocation: true` (réservé à `/nom`).
    pub model_invocable: bool,
    /// Proposé dans la liste des `/commandes` ? Faux avec `user-invocable: false`.
    pub user_invocable: bool,
}

struct Meta {
    name: Option<String>,
    description: Option<String>,
    argument_hint: Option<String>,
    disable_model: bool,
    user_invocable: bool,
    body: String,
}

/// Valeur d'un champ de premier niveau de l'en-tête, y compris les blocs YAML `>` et `|` sur plusieurs lignes.
pub(crate) fn field_of(head: &str, key: &str) -> Option<String> {
    let lines: Vec<&str> = head.lines().collect();
    for (i, l) in lines.iter().enumerate() {
        if l.starts_with(char::is_whitespace) {
            continue;
        }
        let Some(v) = l.strip_prefix(key).and_then(|r| r.strip_prefix(':')) else { continue };
        let v = v.trim();
        if matches!(v, ">" | ">-" | ">+" | "|" | "|-" | "|+") {
            let block: Vec<&str> = lines[i + 1..]
                .iter()
                .take_while(|n| n.is_empty() || n.starts_with(char::is_whitespace))
                .map(|n| n.trim())
                .filter(|n| !n.is_empty())
                .collect();
            return Some(block.join(" ")).filter(|t| !t.is_empty());
        }
        // Liste YAML en blocs (« tools: » puis des lignes « - Read ») : éléments joints par des virgules.
        if v.is_empty() {
            let items: Vec<String> = lines[i + 1..]
                .iter()
                .take_while(|n| n.trim().is_empty() || n.starts_with(char::is_whitespace) || n.trim_start().starts_with("- "))
                .filter_map(|n| n.trim().strip_prefix("- ").map(|x| x.trim().trim_matches(['"', '\'']).to_string()))
                .filter(|x| !x.is_empty())
                .collect();
            return Some(items.join(", ")).filter(|t| !t.is_empty());
        }
        // Liste en ligne : « [Read, Grep] ».
        let v = v.strip_prefix('[').and_then(|r| r.strip_suffix(']')).unwrap_or(v);
        return Some(v.trim_matches(['"', '\'']).to_string()).filter(|v| !v.is_empty());
    }
    None
}

/// Sépare l'en-tête `---` ... `---` du corps.
pub(crate) fn split_front(text: &str) -> Option<(&str, &str)> {
    let text = text.trim_start_matches('\u{feff}');
    let rest = text.strip_prefix("---")?;
    let end = rest.find("\n---")?;
    Some((&rest[..end], rest[end + 4..].trim_start_matches(['\r', '\n'])))
}

/// Extrait de l'en-tête les champs utiles d'un skill.
fn parse_meta(text: &str) -> Meta {
    let text = text.trim_start_matches('\u{feff}');
    let plain = |body: &str| Meta { name: None, description: None, argument_hint: None, disable_model: false, user_invocable: true, body: body.to_string() };
    let Some((head, body)) = split_front(text) else { return plain(text) };
    let flag = |key: &str| field_of(head, key).map(|v| v.eq_ignore_ascii_case("true"));
    let description = match (field_of(head, "description"), field_of(head, "when_to_use")) {
        (Some(d), Some(w)) => Some(format!("{d} {w}")),
        (d, w) => d.or(w),
    };
    Meta {
        name: field_of(head, "name"),
        description,
        argument_hint: field_of(head, "argument-hint"),
        disable_model: flag("disable-model-invocation").unwrap_or(false),
        user_invocable: flag("user-invocable").unwrap_or(true),
        body: body.to_string(),
    }
}

fn parse(text: &str) -> (Option<String>, Option<String>, String) {
    let m = parse_meta(text);
    (m.name, m.description, m.body)
}

fn scan_dir(root: &Path, scope: &str, out: &mut Vec<Skill>) {
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for e in entries.filter_map(Result::ok) {
        let dir = e.path();
        let Ok(text) = std::fs::read_to_string(dir.join("SKILL.md")) else { continue };
        let folder = e.file_name().to_string_lossy().into_owned();
        let meta = parse_meta(&text);
        out.push(Skill {
            name: meta.name.unwrap_or(folder),
            description: meta.description.unwrap_or_default(),
            dir,
            scope: scope.into(),
            enabled: true,
            argument_hint: meta.argument_hint,
            model_invocable: !meta.disable_model,
            user_invocable: meta.user_invocable,
        });
    }
}

/// Skills de l'application, puis ceux du projet (qui prennent le pas sur un homonyme).
pub fn scan(app_dir: &Path, workspace: Option<&Path>) -> Vec<Skill> {
    let mut skills = Vec::new();
    scan_dir(app_dir, "app", &mut skills);
    if let Some(ws) = workspace {
        let mut project = Vec::new();
        scan_dir(&ws.join(".skills"), "project", &mut project);
        skills.retain(|s| !project.iter().any(|p| p.name == s.name));
        skills.extend(project);
    }
    skills.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    skills
}

/// Instructions complètes d'un skill, suivies de la liste de ses fichiers annexes.
pub fn read_body(skill: &Skill, args: Option<&str>) -> Result<String, String> {
    let text = std::fs::read_to_string(skill.dir.join("SKILL.md")).map_err(|e| e.to_string())?;
    let (_, _, mut body) = parse(&text);
    body = with_arguments(&body, args);
    if body.len() > MAX_BODY {
        let cut = (0..=MAX_BODY).rev().find(|i| body.is_char_boundary(*i)).unwrap_or(0);
        body.truncate(cut);
        body.push_str("\n[… tronqué …]");
    }
    let files: Vec<String> = ignore::WalkBuilder::new(&skill.dir)
        .build()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_some_and(|t| t.is_file()) && e.file_name() != "SKILL.md")
        .filter_map(|e| e.path().strip_prefix(&skill.dir).ok().map(|p| p.to_string_lossy().replace('\\', "/")))
        .take(50)
        .collect();
    if !files.is_empty() {
        body.push_str(&format!(
            "\n\n[Dossier du skill : {}]\nFichiers annexes (lisibles avec use_skill et le paramètre file) :\n{}",
            skill.dir.display(),
            files.join("\n")
        ));
    }
    Ok(body)
}

/// Insère les arguments de `/nom arguments` : à la place de `$ARGUMENTS`, ou en fin de texte s'il n'y figure pas.
fn with_arguments(body: &str, args: Option<&str>) -> String {
    let args = args.map(str::trim).unwrap_or("");
    if body.contains("$ARGUMENTS") {
        return body.replace("$ARGUMENTS", args);
    }
    if args.is_empty() {
        body.to_string()
    } else {
        format!("{body}\n\nARGUMENTS: {args}")
    }
}

/// Lit un fichier annexe d'un skill, sans jamais sortir du dossier du skill.
pub fn read_file(skill: &Skill, rel: &str) -> Result<String, String> {
    let root = skill.dir.canonicalize().map_err(|e| e.to_string())?;
    let path = root.join(rel.trim_start_matches(['/', '\\'])).canonicalize().map_err(|e| format!("Fichier introuvable : {e}"))?;
    if !path.starts_with(&root) {
        return Err("Chemin hors du dossier du skill".into());
    }
    let mut text = std::fs::read_to_string(path).map_err(|e| format!("Lecture impossible : {e}"))?;
    if text.len() > MAX_BODY {
        let cut = (0..=MAX_BODY).rev().find(|i| text.is_char_boundary(*i)).unwrap_or(0);
        text.truncate(cut);
        text.push_str("\n[… tronqué …]");
    }
    Ok(text)
}

/// Mots sans valeur pour la recherche (français et anglais courants).
const STOP: [&str; 28] = [
    "les", "des", "une", "pour", "dans", "avec", "que", "qui", "sur", "est", "pas", "aux", "the", "and", "for",
    "with", "that", "this", "you", "use", "when", "from", "are", "not", "your", "can", "will", "all",
];

pub(crate) fn words(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in text.to_lowercase().split(|c: char| !c.is_alphanumeric()) {
        if w.chars().count() >= 3 && !STOP.contains(&w) && !out.iter().any(|x| x == w) {
            out.push(w.to_string());
        }
    }
    out
}

/// Pertinence d'un skill pour des mots-clés : le nom compte plus que la description.
fn score(skill: &Skill, ws: &[String]) -> u32 {
    let name = skill.name.to_lowercase();
    let desc = skill.description.to_lowercase();
    ws.iter()
        .map(|w| {
            let stem: String = w.chars().take(5).collect();
            let n = if name.contains(w.as_str()) { 4 } else if stem.len() >= 4 && name.contains(&stem) { 2 } else { 0 };
            let d = if desc.contains(w.as_str()) { 2 } else if stem.len() >= 4 && desc.contains(&stem) { 1 } else { 0 };
            n + d
        })
        .sum()
}

/// Les `limit` skills activés les plus proches de `query` (au moins un mot en commun).
pub fn search<'a>(skills: &'a [Skill], query: &str, limit: usize) -> Vec<&'a Skill> {
    let ws = words(query);
    if ws.is_empty() {
        return vec![];
    }
    let mut hits: Vec<(u32, &Skill)> = skills.iter().filter(|s| s.enabled && s.model_invocable).map(|s| (score(s, &ws), s)).filter(|(n, _)| *n > 0).collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase())));
    hits.into_iter().take(limit).map(|(_, s)| s).collect()
}

/// Taille (en caractères) réservée à la liste des skills : 3 % du contexte du modèle, entre 3 000 et 24 000.
pub fn index_budget(ctx_tokens: usize) -> usize {
    ((ctx_tokens as f64 * 3.5 * 0.03) as usize).clamp(3_000, 24_000)
}

fn short(text: &str, max: usize) -> String {
    let t: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() <= max {
        return t;
    }
    format!("{}…", t.chars().take(max).collect::<String>().trim_end())
}

fn line(s: &Skill, cap: usize) -> String {
    if cap == 0 || s.description.is_empty() {
        format!("- {}", s.name)
    } else {
        format!("- {} : {}", s.name, short(&s.description, cap))
    }
}

/// Texte de la consigne système listant les skills activés, quel que soit leur nombre :
/// les plus pertinents pour la demande sont détaillés, les descriptions des autres sont raccourcies
/// jusqu'à tenir dans `budget` caractères, et `search_skills` retrouve ceux qui n'y figurent pas.
pub fn prompt_section(skills: &[Skill], query: &str, budget: usize) -> String {
    let active: Vec<&Skill> = skills.iter().filter(|s| s.enabled && s.model_invocable).collect();
    if active.is_empty() {
        return String::new();
    }
    let hot = search(skills, query, 3);
    let others: Vec<&Skill> = active.iter().copied().filter(|s| !hot.iter().any(|h| h.name == s.name)).collect();
    let header = format!(
        "\n\nSkills disponibles ({}). Quand la demande correspond à l'un d'eux, appelle d'abord use_skill avec son nom pour obtenir ses instructions, puis suis-les. Si aucun ne ressort clairement dans la liste, cherche avec search_skills (mots-clés) avant de répondre.",
        active.len()
    );
    let mut text = header;
    let hot_block: Vec<String> = hot.iter().map(|s| line(s, 220)).collect();
    if !hot_block.is_empty() {
        text.push_str("\n\nLes plus pertinents pour cette demande :\n");
        text.push_str(&hot_block.join("\n"));
    }
    let used = text.len();
    let room = budget.saturating_sub(used);
    let size = |cap: usize| others.iter().map(|s| line(s, cap).len() + 1).sum::<usize>();
    let cap = [160, 110, 70, 40, 0].into_iter().find(|&c| size(c) <= room);
    text.push_str("\n\nTous les skills :\n");
    match cap {
        Some(c) => text.push_str(&others.iter().map(|s| line(s, c)).collect::<Vec<_>>().join("\n")),
        None => {
            // Même les noms seuls dépassent le budget : on garde ce qui rentre, le reste est accessible par recherche.
            let mut left = room;
            let mut shown = 0;
            let mut lines = Vec::new();
            for s in &others {
                let l = line(s, 0);
                if l.len() + 1 > left {
                    break;
                }
                left -= l.len() + 1;
                lines.push(l);
                shown += 1;
            }
            text.push_str(&lines.join("\n"));
            text.push_str(&format!("\n(+ {} autres skills activés : retrouve-les avec search_skills)", others.len() - shown));
        }
    }
    text
}

/// Réponse de l'outil `search_skills` : nom et description complète des meilleurs résultats.
pub fn search_report(skills: &[Skill], query: &str) -> String {
    let hits = search(skills, query, 8);
    if hits.is_empty() {
        return format!("Aucun skill ne correspond à « {query} ». Essaie d'autres mots-clés (en anglais aussi) ou continue sans skill.");
    }
    let list: Vec<String> = hits.iter().map(|s| format!("- {} : {}", s.name, short(&s.description, 400))).collect();
    format!("Skills trouvés (appelle use_skill avec le nom choisi) :\n{}", list.join("\n"))
}

/// Marque comme désactivés les skills dont le nom figure dans `disabled`.
pub fn apply_disabled(skills: &mut [Skill], disabled: &HashSet<String>) {
    for s in skills {
        s.enabled = !disabled.contains(&s.name);
    }
}

pub fn load_disabled(file: &Path) -> HashSet<String> {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
        .map(|v| v.into_iter().collect())
        .unwrap_or_default()
}

pub fn save_disabled(file: &Path, disabled: &HashSet<String>) -> Result<(), String> {
    let mut v: Vec<&String> = disabled.iter().collect();
    v.sort();
    std::fs::write(file, serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// Un skill trouvé ailleurs sur le PC (installé par Claude Desktop ou Claude Code), importable.
#[derive(Debug, Clone, Serialize)]
pub struct Found {
    pub name: String,
    pub description: String,
    pub dir: PathBuf,
    pub group: String,
    pub size: u64,
    pub installed: bool,
}

const SKIP_DIRS: [&str; 14] = [
    "node_modules", ".git", "projects", "todos", "shell-snapshots", "statsig", "ide", "file-history",
    "paste-cache", "telemetry", "cache", "logs", "sessions", "backups",
];

fn group_of(path: &Path) -> String {
    let p = path.to_string_lossy().to_lowercase().replace('\\', "/");
    if p.contains("/skills-plugin/") {
        "Claude Desktop — skills intégrés".into()
    } else if p.contains("/rpm/") {
        "Claude Desktop — plugins".into()
    } else if p.contains("/marketplaces/") {
        let plugin = p.split("/plugins/").nth(1).and_then(|r| r.split('/').next()).unwrap_or("plugin");
        format!("Claude Code — {plugin}")
    } else if p.contains("/.claude/skills/") {
        "Claude Code — mes skills".into()
    } else {
        "Autres".into()
    }
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 10 {
        return;
    }
    if dir.join("SKILL.md").is_file() {
        out.push(dir.to_path_buf());
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.filter_map(Result::ok) {
        let name = e.file_name().to_string_lossy().into_owned();
        if e.file_type().is_ok_and(|t| t.is_dir()) && !SKIP_DIRS.contains(&name.as_str()) {
            walk(&e.path(), depth + 1, out);
        }
    }
}

/// Cherche des skills dans `roots` (le premier trouvé l'emporte en cas d'homonyme).
pub fn discover(roots: &[PathBuf], installed: &[Skill]) -> Vec<Found> {
    let mut dirs = Vec::new();
    for r in roots {
        walk(r, 0, &mut dirs);
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(text) = std::fs::read_to_string(dir.join("SKILL.md")) else { continue };
        let (name, description, _) = parse(&text);
        let folder = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let name = name.unwrap_or(folder);
        if !seen.insert(name.to_lowercase()) {
            continue;
        }
        out.push(Found {
            installed: installed.iter().any(|s| s.name.eq_ignore_ascii_case(&name)),
            group: group_of(&dir),
            size: std::fs::metadata(dir.join("SKILL.md")).map(|m| m.len()).unwrap_or(0),
            description: description.unwrap_or_default(),
            name,
            dir,
        });
    }
    out.sort_by(|a, b| a.group.cmp(&b.group).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    out
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let target = dst.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &target)?;
        } else {
            std::fs::copy(e.path(), target)?;
        }
    }
    Ok(())
}

/// Copie un skill dans `dest_root` (jamais d'écrasement). Renvoie `Ok(false)` s'il existait déjà.
pub fn import(src: &Path, dest_root: &Path) -> Result<bool, String> {
    if !src.join("SKILL.md").is_file() {
        return Err("Ce dossier ne contient pas de SKILL.md".into());
    }
    let folder = src.file_name().ok_or("Dossier invalide")?;
    let dest = dest_root.join(folder);
    if dest.exists() {
        return Ok(false);
    }
    copy_dir(src, &dest).map_err(|e| format!("Copie impossible : {e}"))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(root: &Path, folder: &str, content: &str) {
        std::fs::create_dir_all(root.join(folder)).unwrap();
        std::fs::write(root.join(folder).join("SKILL.md"), content).unwrap();
    }

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("skills-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn parses_frontmatter_and_body() {
        let (n, d, b) = parse("---\nname: pdf-tools\ndescription: \"Manipule des PDF\"\n---\n# Titre\nCorps");
        assert_eq!(n.as_deref(), Some("pdf-tools"));
        assert_eq!(d.as_deref(), Some("Manipule des PDF"));
        assert!(b.starts_with("# Titre"));
        let (n, _, b) = parse("Sans en-tête");
        assert!(n.is_none() && b == "Sans en-tête");
    }

    #[test]
    fn project_skill_overrides_app_skill_and_folder_is_fallback_name() {
        let app = tmp();
        let ws = tmp();
        make(&app, "a", "---\nname: shared\ndescription: app\n---\nx");
        make(&app, "sans-nom", "juste du texte");
        make(&ws.join(".skills"), "b", "---\nname: shared\ndescription: projet\n---\ny");
        let skills = scan(&app, Some(&ws));
        assert_eq!(skills.len(), 2);
        assert_eq!(skills.iter().find(|s| s.name == "shared").unwrap().description, "projet");
        assert!(skills.iter().any(|s| s.name == "sans-nom"));
    }

    fn skill(name: &str, desc: &str, enabled: bool) -> Skill {
        Skill { name: name.into(), description: desc.into(), dir: PathBuf::new(), scope: "app".into(), enabled, argument_hint: None, model_invocable: true, user_invocable: true }
    }

    #[test]
    fn prompt_lists_only_enabled_skills() {
        assert!(prompt_section(&[skill("a", "d", false)], "", 5000).is_empty());
        let mut some = vec![skill("a", "d", true), skill("b", "d", true)];
        apply_disabled(&mut some, &HashSet::from(["b".to_string()]));
        let p = prompt_section(&some, "", 5000);
        assert!(p.contains("- a : d") && !p.contains("- b"));
    }

    #[test]
    fn index_keeps_every_skill_within_budget_by_shortening_descriptions() {
        let many: Vec<Skill> = (0..140).map(|i| skill(&format!("skill-{i:03}"), &"mot ".repeat(120), true)).collect();
        let budget = 6_000;
        let p = prompt_section(&many, "", budget);
        assert!(p.len() <= budget, "{} > {budget}", p.len());
        assert!((0..140).all(|i| p.contains(&format!("skill-{i:03}"))), "aucun skill ne doit manquer");
        assert!(!p.contains("autres skills"));
    }

    #[test]
    fn huge_collections_fall_back_to_search_but_relevant_skills_always_show() {
        let mut many: Vec<Skill> = (0..3000).map(|i| skill(&format!("generic-skill-{i:04}"), "outil divers", true)).collect();
        many.push(skill("pdf-tools", "Remplir et fusionner des fichiers PDF", true));
        let p = prompt_section(&many, "merge ce fichier pdf", 4_000);
        assert!(p.len() <= 4_000 + 400, "{}", p.len());
        assert!(p.contains("Les plus pertinents") && p.contains("- pdf-tools : Remplir"));
        assert!(p.contains("autres skills activés") && p.contains("search_skills"));
    }

    #[test]
    fn search_ranks_name_over_description_and_ignores_disabled_and_stopwords() {
        let skills = vec![
            skill("docx", "Create Word documents", true),
            skill("excel-helper", "Works with spreadsheets and docx exports", true),
            skill("pdf", "Fill PDF forms", false),
        ];
        let hits = search(&skills, "make a docx for me", 5);
        assert_eq!(hits.first().map(|s| s.name.as_str()), Some("docx"));
        assert!(search(&skills, "pdf", 5).is_empty(), "désactivé");
        assert!(search(&skills, "the and for", 5).is_empty(), "mots vides seuls");
        assert!(search_report(&skills, "docx").contains("- docx :"));
        assert!(search_report(&skills, "zzzz").contains("Aucun skill"));
    }

    #[test]
    fn reads_yaml_block_descriptions_when_to_use_and_flags() {
        let m = parse_meta("---\nname: academy-guide\ndescription: >\n  Guide pour\n  les nouveaux\nwhen_to_use: Quand on débute\nargument-hint: <sujet>\ndisable-model-invocation: true\nuser-invocable: false\n---\ncorps");
        assert_eq!(m.description.as_deref(), Some("Guide pour les nouveaux Quand on débute"));
        assert_eq!(m.argument_hint.as_deref(), Some("<sujet>"));
        assert!(m.disable_model && !m.user_invocable);
        assert_eq!(m.body, "corps");
        let simple = parse_meta("---\nname: a\ndescription: \"Court\"\n---\nx");
        assert!(!simple.disable_model && simple.user_invocable && simple.description.as_deref() == Some("Court"));
    }

    #[test]
    fn manual_only_skills_stay_out_of_the_model_index_and_search() {
        let mut manual = skill("deploy", "Déployer en production", true);
        manual.model_invocable = false;
        let skills = vec![manual, skill("lint", "Linter le code", true)];
        let p = prompt_section(&skills, "deploy", 5_000);
        assert!(p.contains("- lint") && !p.contains("deploy"));
        assert!(search(&skills, "deploy production", 5).is_empty());
    }

    #[test]
    fn arguments_replace_placeholder_or_get_appended() {
        assert_eq!(with_arguments("Fais $ARGUMENTS vite", Some(" ceci ")), "Fais ceci vite");
        assert_eq!(with_arguments("Texte", Some("a b")), "Texte\n\nARGUMENTS: a b");
        assert_eq!(with_arguments("Texte", None), "Texte");
    }

    #[test]
    fn budget_scales_with_context_within_bounds() {
        assert_eq!(index_budget(1_000), 3_000);
        assert!(index_budget(131_072) > index_budget(8_192));
        assert_eq!(index_budget(10_000_000), 24_000);
    }

    #[test]
    fn disabled_list_roundtrips_and_missing_file_is_empty() {
        let d = tmp();
        assert!(load_disabled(&d.join("none.json")).is_empty());
        save_disabled(&d.join("s.json"), &HashSet::from(["x".to_string()])).unwrap();
        assert!(load_disabled(&d.join("s.json")).contains("x"));
    }

    #[test]
    fn discovers_nested_skills_dedupes_and_imports_without_overwriting() {
        let root = tmp();
        make(&root.join("plugin-a").join("skills"), "alpha", "---\nname: alpha\ndescription: A\n---\nx");
        make(&root.join("plugin-b").join("deep").join("skills"), "alpha", "---\nname: alpha\ndescription: doublon\n---\ny");
        make(&root.join("plugin-b").join("skills"), "beta", "---\nname: beta\ndescription: B\n---\nz");
        std::fs::create_dir_all(root.join("node_modules").join("skills").join("evil")).unwrap();
        std::fs::write(root.join("node_modules").join("skills").join("evil").join("SKILL.md"), "x").unwrap();
        std::fs::write(root.join("plugin-a").join("skills").join("alpha").join("data.txt"), "annexe").unwrap();

        let installed = vec![skill("beta", "", true)];
        let found = discover(&[root.clone()], &installed);
        assert_eq!(found.len(), 2, "doublons et node_modules ignorés");
        assert!(found.iter().find(|f| f.name == "beta").unwrap().installed);

        let dest = tmp();
        let alpha = &found.iter().find(|f| f.name == "alpha").unwrap().dir;
        assert!(import(alpha, &dest).unwrap());
        assert_eq!(std::fs::read_to_string(dest.join("alpha").join("data.txt")).unwrap(), "annexe");
        assert!(!import(alpha, &dest).unwrap(), "pas d'écrasement");
        assert!(import(&root, &dest).is_err());
    }

    #[test]
    fn read_file_stays_inside_skill_dir() {
        let app = tmp();
        make(&app, "s", "---\nname: s\ndescription: d\n---\nbody");
        std::fs::write(app.join("secret.txt"), "x").unwrap();
        std::fs::write(app.join("s").join("notes.md"), "ok").unwrap();
        let s = &scan(&app, None)[0];
        assert_eq!(read_file(s, "notes.md").unwrap(), "ok");
        assert!(read_file(s, "../secret.txt").is_err());
        assert!(read_body(s, None).unwrap().contains("notes.md"));
    }
}
