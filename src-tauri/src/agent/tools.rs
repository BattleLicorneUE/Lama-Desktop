use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

use super::{fsops, project, sandbox};

/// Taille maximale d'une lecture (≈ 11 000 jetons) : un fichier plus gros se lit par morceaux avec offset/limit.
const MAX_READ: usize = 40_000;
const MAX_OUTPUT: usize = 20_000;
const MAX_GREP: usize = 100;
const EXEC_TIMEOUT: Duration = Duration::from_secs(120);

/// Ce que l'outil demande de faire, avant toute modification.
pub enum Prepared {
    /// Outil de lecture : déjà exécuté.
    Done(String),
    Write { path: PathBuf, before: Option<String>, after: String, diff: String },
    Exec { command: String },
    /// Commande lancée sans attendre sa fin (serveur de dev, surveillance…).
    Spawn { command: String },
    Mcp { server: String, tool: String, args: Value },
    /// Création, suppression ou déplacement de fichiers et dossiers.
    Fs(FsOp),
    /// Outils sans accès au dossier : skills, mémoire, web.
    Meta { name: String, args: Value },
}

pub enum FsOp {
    MakeDir(PathBuf),
    /// Envoyé à la corbeille (récupérable).
    Delete(PathBuf),
    Move { from: PathBuf, to: PathBuf },
}

impl FsOp {
    pub fn is_delete(&self) -> bool {
        matches!(self, FsOp::Delete(_))
    }
}

fn schema(name: &str, desc: &str, props: Value, required: &[&str]) -> Value {
    json!({ "type": "function", "function": {
        "name": name, "description": desc,
        "parameters": { "type": "object", "properties": props, "required": required }
    }})
}

pub fn definitions(read_only: bool) -> Vec<Value> {
    let mut t = vec![
        schema("read_file", "Lit un fichier texte du projet.",
            json!({ "path": {"type":"string","description":"Chemin relatif au projet"},
                    "offset": {"type":"integer","description":"Première ligne (1 = début)"},
                    "limit": {"type":"integer","description":"Nombre de lignes max"} }), &["path"]),
        schema("list_dir", "Liste le contenu d'un dossier du projet.",
            json!({ "path": {"type":"string","description":"Dossier relatif, '.' pour la racine"} }), &["path"]),
        schema("find_symbol", "Trouve où un nom (fonction, classe, variable, type) est défini et où il est utilisé dans le projet.",
            json!({ "name": {"type":"string","description":"Identifiant exact"} }), &["name"]),
        schema("grep", "Cherche une expression régulière dans les fichiers du projet (respecte .gitignore).",
            json!({ "pattern": {"type":"string"}, "path": {"type":"string","description":"Sous-dossier, optionnel"} }), &["pattern"]),
    ];
    if !read_only {
        t.push(schema("write_file", "Crée ou remplace entièrement un fichier.",
            json!({ "path": {"type":"string"}, "content": {"type":"string"} }), &["path", "content"]));
        t.push(schema("edit_file", "Remplace un extrait unique de texte dans un fichier existant.",
            json!({ "path": {"type":"string"}, "old": {"type":"string","description":"Texte exact à remplacer (unique)"},
                    "new": {"type":"string"} }), &["path", "old", "new"]));
        t.push(schema("make_dir", "Crée un dossier (et ses parents) dans le projet.",
            json!({ "path": {"type":"string"} }), &["path"]));
        t.push(schema("move_path", "Déplace ou renomme un fichier ou un dossier du projet. La destination ne doit pas exister.",
            json!({ "from": {"type":"string"}, "to": {"type":"string"} }), &["from", "to"]));
        t.push(schema("delete_path", "Supprime un fichier ou un dossier (avec son contenu) en l'envoyant à la corbeille.",
            json!({ "path": {"type":"string"} }), &["path"]));
        t.push(schema("verify", "Lance la vérification du projet (compilation/lint, tests ou formatage), détectée automatiquement.",
            json!({ "target": {"type":"string","enum":["check","test","format"],"description":"check par défaut"} }), &[]));
        t.push(schema("run_command", "Exécute une commande PowerShell dans le dossier du projet. Avec background=true, la lance sans attendre sa fin (serveur de développement, surveillance de fichiers) : utilise ensuite command_output et stop_command.",
            json!({ "command": {"type":"string"}, "background": {"type":"boolean","description":"Lancer en arrière-plan"} }), &["command"]));
        t.push(schema("command_output", "Lit l'état et la fin de la sortie d'une commande lancée en arrière-plan.",
            json!({ "id": {"type":"string"} }), &["id"]));
        t.push(schema("stop_command", "Arrête une commande lancée en arrière-plan (et ses sous-processus).",
            json!({ "id": {"type":"string"} }), &["id"]));
    }
    t
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key).and_then(Value::as_str).ok_or_else(|| format!("Argument manquant ou invalide : {key}"))
}

pub(super) fn truncate_tail(s: String, max: usize) -> String {
    if s.len() <= max {
        return s;
    }
    let mut start = s.len() - max;
    while !s.is_char_boundary(start) {
        start += 1;
    }
    format!("[… début tronqué …]\n{}", &s[start..])
}

/// Sortie longue (compilation, tests) : début, lignes d'erreur avec leur contexte, puis fin. Une sortie courte reste intacte.
pub(super) fn condense_output(text: String) -> String {
    const LIMIT: usize = 6_000;
    if text.len() <= LIMIT {
        return text;
    }
    let lines: Vec<&str> = text.lines().collect();
    let re = regex::Regex::new(r"(?i)\b(error|erreur|failed|fail|panic|exception|traceback|assert)|^\s*-->|✗|×").unwrap();
    let mut keep = vec![false; lines.len()];
    let head = lines.len().min(15);
    let tail_start = lines.len().saturating_sub(25);
    keep[..head].fill(true);
    keep[tail_start..].fill(true);
    let mut budget = 3_500usize;
    for (i, l) in lines.iter().enumerate() {
        if re.is_match(l) {
            for j in i.saturating_sub(1)..(i + 3).min(lines.len()) {
                if !keep[j] && budget >= lines[j].len() {
                    keep[j] = true;
                    budget -= lines[j].len();
                }
            }
        }
    }
    let mut out = String::new();
    let mut skipped = false;
    for (l, k) in lines.iter().zip(&keep) {
        if *k {
            out.push_str(l);
            out.push('\n');
            skipped = false;
        } else if !skipped {
            out.push_str("[… lignes sans intérêt retirées …]\n");
            skipped = true;
        }
    }
    out
}

pub fn unified_diff(path: &str, before: &str, after: &str) -> String {
    similar::TextDiff::from_lines(before, after)
        .unified_diff()
        .context_radius(3)
        .header(&format!("a/{path}"), &format!("b/{path}"))
        .to_string()
}

fn rel_display(root: &Path, p: &Path) -> String {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    p.strip_prefix(&root).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

/// Analyse l'appel d'outil. Les lectures sont exécutées ici ; les écritures et
/// commandes sont seulement décrites (diff, commande) pour être approuvées.
pub fn prepare(root: &Path, name: &str, args: &Value) -> Result<Prepared, String> {
    match name {
        "read_file" => {
            let path = sandbox::resolve(root, str_arg(args, "path")?)?;
            let text = std::fs::read_to_string(&path).map_err(|e| format!("Lecture impossible : {e}"))?;
            let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(1).max(1) as usize;
            let limit = args.get("limit").and_then(Value::as_u64).map(|l| l as usize);
            let mut out: String = text
                .lines()
                .enumerate()
                .skip(offset - 1)
                .take(limit.unwrap_or(usize::MAX))
                .map(|(i, l)| format!("{:>5}\t{l}\n", i + 1))
                .collect();
            if out.len() > MAX_READ {
                let mut cut = MAX_READ;
                while !out.is_char_boundary(cut) {
                    cut -= 1;
                }
                out.truncate(cut);
                out.push_str("\n[… tronqué, utilise offset/limit …]");
            }
            Ok(Prepared::Done(out))
        }
        "list_dir" => {
            let path = sandbox::resolve(root, args.get("path").and_then(Value::as_str).unwrap_or("."))?;
            let mut entries: Vec<String> = std::fs::read_dir(&path)
                .map_err(|e| format!("Lecture du dossier impossible : {e}"))?
                .filter_map(Result::ok)
                .map(|e| {
                    let dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    format!("{}{}", e.file_name().to_string_lossy(), if dir { "/" } else { "" })
                })
                .collect();
            entries.sort();
            Ok(Prepared::Done(if entries.is_empty() { "(dossier vide)".into() } else { entries.join("\n") }))
        }
        "find_symbol" => {
            let name = str_arg(args, "name")?.trim();
            if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') {
                return Err("Nom invalide : un identifiant (lettres, chiffres, _).".into());
            }
            let n = regex::escape(name);
            let def = regex::Regex::new(&format!(r"\b(?:fn|function|class|struct|enum|trait|interface|type|def|impl|mod|const|let|var|static|func|namespace|record)\s+{n}\b|\b{n}\s*[:=]\s*(?:async\s*)?(?:\(|function\b)")).map_err(|e| e.to_string())?;
            let usage = regex::Regex::new(&format!(r"\b{n}\b")).map_err(|e| e.to_string())?;
            let base = sandbox::resolve(root, ".")?;
            let (mut defs, mut uses) = (Vec::new(), Vec::new());
            for entry in ignore::WalkBuilder::new(&base).build().filter_map(Result::ok) {
                if !entry.file_type().is_some_and(|t| t.is_file()) {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(entry.path()) else { continue };
                for (i, line) in text.lines().enumerate() {
                    let item = || format!("{}:{}: {}", rel_display(root, entry.path()), i + 1, line.trim());
                    if def.is_match(line) {
                        if defs.len() < 20 {
                            defs.push(item());
                        }
                    } else if usage.is_match(line) && uses.len() < 40 {
                        uses.push(item());
                    }
                }
            }
            if defs.is_empty() && uses.is_empty() {
                return Ok(Prepared::Done(format!("« {name} » est introuvable dans le projet.")));
            }
            let list = |v: &[String]| if v.is_empty() { "(aucune)".to_string() } else { v.join("\n") };
            Ok(Prepared::Done(format!("Définitions :\n{}\n\nUtilisations (40 max) :\n{}", list(&defs), list(&uses))))
        }
        "grep" => {
            let re = regex::Regex::new(str_arg(args, "pattern")?).map_err(|e| format!("Regex invalide : {e}"))?;
            let base = sandbox::resolve(root, args.get("path").and_then(Value::as_str).unwrap_or("."))?;
            let mut hits = Vec::new();
            'files: for entry in ignore::WalkBuilder::new(&base).build().filter_map(Result::ok) {
                if !entry.file_type().is_some_and(|t| t.is_file()) {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(entry.path()) else { continue };
                for (i, line) in text.lines().enumerate() {
                    if re.is_match(line) {
                        hits.push(format!("{}:{}: {}", rel_display(root, entry.path()), i + 1, line.trim()));
                        if hits.len() >= MAX_GREP {
                            hits.push(format!("[… limité à {MAX_GREP} résultats …]"));
                            break 'files;
                        }
                    }
                }
            }
            Ok(Prepared::Done(if hits.is_empty() { "Aucun résultat.".into() } else { hits.join("\n") }))
        }
        "write_file" => {
            let path = sandbox::resolve(root, str_arg(args, "path")?)?;
            if sandbox::is_protected(&path) {
                return Err("Les fichiers .git sont protégés.".into());
            }
            let after = str_arg(args, "content")?.to_string();
            let before = std::fs::read_to_string(&path).ok();
            let diff = unified_diff(&rel_display(root, &path), before.as_deref().unwrap_or(""), &after);
            Ok(Prepared::Write { path, before, after, diff })
        }
        "edit_file" => {
            let path = sandbox::resolve(root, str_arg(args, "path")?)?;
            if sandbox::is_protected(&path) {
                return Err("Les fichiers .git sont protégés.".into());
            }
            let (old, new) = (str_arg(args, "old")?, str_arg(args, "new")?);
            let before = std::fs::read_to_string(&path).map_err(|e| format!("Lecture impossible : {e}"))?;
            let after = apply_edit(&before, old, new)?;
            let diff = unified_diff(&rel_display(root, &path), &before, &after);
            Ok(Prepared::Write { path, before: Some(before), after, diff })
        }
        "make_dir" => Ok(Prepared::Fs(FsOp::MakeDir(fsops::new_target(root, str_arg(args, "path")?)?))),
        "delete_path" => Ok(Prepared::Fs(FsOp::Delete(fsops::existing_target(root, str_arg(args, "path")?)?))),
        "move_path" => {
            let (from, to) = fsops::move_targets(root, str_arg(args, "from")?, str_arg(args, "to")?)?;
            Ok(Prepared::Fs(FsOp::Move { from, to }))
        }
        "verify" => {
            let target = args.get("target").and_then(Value::as_str).unwrap_or("check");
            let command = project::command_for(root, target).ok_or_else(|| {
                format!("Aucune commande « {target} » détectée pour ce projet. Utilise run_command avec la bonne commande.")
            })?;
            Ok(Prepared::Exec { command })
        }
        "run_command" => {
            let command = str_arg(args, "command")?.to_string();
            if args.get("background").and_then(Value::as_bool).unwrap_or(false) {
                Ok(Prepared::Spawn { command })
            } else {
                Ok(Prepared::Exec { command })
            }
        }
        other => Err(format!("Outil inconnu : {other}")),
    }
}

fn indent_of(line: &str) -> &str {
    &line[..line.len() - line.trim_start().len()]
}

fn line_starts(text: &str) -> Vec<usize> {
    let mut v = vec![0];
    v.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    v
}

/// Convertit les fins de ligne de `s` pour correspondre à celles du fichier.
fn match_eol(s: &str, crlf: bool) -> String {
    let unix = s.replace("\r\n", "\n");
    if crlf { unix.replace('\n', "\r\n") } else { unix }
}

/// Extrait numéroté autour de la première ligne de `old` retrouvée dans le fichier (aide à corriger l'appel).
fn not_found_hint(before: &str, old: &str) -> String {
    let first = old.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let lines: Vec<&str> = before.lines().collect();
    if let Some(k) = lines.iter().position(|l| !first.is_empty() && l.trim() == first) {
        let (a, b) = (k.saturating_sub(2), (k + 4).min(lines.len()));
        let excerpt: String = (a..b).map(|i| format!("{:>5}\t{}\n", i + 1, lines[i])).collect();
        format!(
            "Texte introuvable tel quel. Sa première ligne existe à la ligne {} mais la suite diffère :\n{excerpt}Relis le fichier avec read_file et recopie l'extrait exactement.",
            k + 1
        )
    } else {
        "Texte à remplacer introuvable. Utilise read_file ou grep pour retrouver l'endroit exact, puis recopie l'extrait tel quel.".into()
    }
}

/// Remplace `old` par `new` : d'abord à l'identique, sinon en ignorant les différences
/// d'indentation et de fins de ligne (le remplacement est alors ré-indenté comme le fichier).
pub fn apply_edit(before: &str, old: &str, new: &str) -> Result<String, String> {
    let crlf = before.contains("\r\n");
    if old.trim().is_empty() {
        return Err("Le texte à remplacer est vide.".into());
    }
    let exact: Vec<usize> = before.match_indices(old).map(|(i, _)| i).collect();
    if exact.len() == 1 {
        let new = match_eol(new, crlf);
        return Ok(format!("{}{}{}", &before[..exact[0]], new, &before[exact[0] + old.len()..]));
    }
    let starts = line_starts(before);
    if exact.len() > 1 {
        let lines: Vec<String> = exact.iter().map(|b| (starts.partition_point(|s| s <= b)).to_string()).collect();
        return Err(format!("Le texte apparaît {} fois (lignes {}). Fournis un extrait plus long et unique.", exact.len(), lines.join(", ")));
    }

    // Recherche tolérante, ligne à ligne.
    let old_raw: Vec<&str> = old.lines().map(|l| l.trim_end_matches('\r')).collect();
    let lo = old_raw.iter().position(|l| !l.trim().is_empty());
    let hi = old_raw.iter().rposition(|l| !l.trim().is_empty());
    let (Some(lo), Some(hi)) = (lo, hi) else { return Err(not_found_hint(before, old)) };
    let old_lines = &old_raw[lo..=hi];
    let file_lines: Vec<&str> = before.split('\n').map(|l| l.trim_end_matches('\r')).collect();
    let n = old_lines.len();
    let hits: Vec<usize> = (0..file_lines.len().saturating_sub(n - 1))
        .filter(|&i| (0..n).all(|j| file_lines[i + j].trim() == old_lines[j].trim()))
        .collect();
    match hits.len() {
        0 => Err(not_found_hint(before, old)),
        1 => {
            let i = hits[0];
            let (fi, oi) = (indent_of(file_lines[i]), indent_of(old_lines[0]));
            let eol = if crlf { "\r\n" } else { "\n" };
            let body: Vec<String> = new
                .lines()
                .map(|l| {
                    let l = l.trim_end_matches('\r');
                    match l.strip_prefix(oi) {
                        Some(rest) => format!("{fi}{rest}"),
                        None => l.to_string(),
                    }
                })
                .collect();
            let start = starts[i];
            let last = i + n - 1;
            let end = starts[last] + file_lines[last].len();
            // `\r` éventuel de la dernière ligne remplacée est conservé par `eol` du reste du fichier.
            Ok(format!("{}{}{}", &before[..start], body.join(eol), &before[end..]))
        }
        k => Err(format!(
            "Le texte (à l'indentation près) apparaît {k} fois (lignes {}). Fournis un extrait plus long et unique.",
            hits.iter().map(|h| (h + 1).to_string()).collect::<Vec<_>>().join(", ")
        )),
    }
}

/// Commandes considérées sans danger (lecture seule / vérification) pour le mode Auto.
/// Commandes qui ne font que lire.
const READ_ONLY: &[&str] = &["git status", "git diff", "git log", "git show", "ls", "dir", "pwd", "echo"];
/// Commandes de vérification : elles lancent du code du projet (scripts npm, build.rs, tests…).
const PROJECT_CODE: &[&str] = &[
    "cargo check", "cargo test", "cargo clippy", "cargo fmt", "npm test", "npm run test", "npm run lint",
    "npm run check", "npm run typecheck", "npm run build", "npm run format", "go build ./...", "go test ./...",
    "dotnet build", "dotnet test", "python -m pytest -q", "python -m compileall -q .",
];

fn starts_with_command(c: &str, s: &str) -> bool {
    c == s || c.starts_with(&format!("{s} "))
}

/// Options de git qui écrivent un fichier ou lancent un programme : une lecture ne doit pas en avoir.
fn git_option_is_risky(c: &str) -> bool {
    c.split_whitespace().skip(2).any(|t| ["--output", "--ext-diff", "--textconv", "--open-files-in-pager", "--exec"].iter().any(|r| t == *r || t.starts_with(&format!("{r}="))))
}

pub fn is_safe_command(cmd: &str) -> bool {
    let c = cmd.trim();
    if c.contains([';', '|', '&', '>', '<', '`']) || c.contains("$(") || c.contains('\n') {
        return false;
    }
    // « git branch » sans option de liste crée ou supprime des branches.
    if starts_with_command(c, "git branch") {
        return c.split_whitespace().skip(2).all(|t| ["-a", "-r", "-v", "-vv", "--list", "--all", "--remotes", "--show-current"].contains(&t));
    }
    if READ_ONLY.iter().any(|s| starts_with_command(c, s)) {
        return !(c.starts_with("git ") && git_option_is_risky(c));
    }
    PROJECT_CODE.iter().any(|s| starts_with_command(c, s))
}

/// Commande « sûre » qui exécute pourtant du code du dossier de travail : en mode Auto, elle demande l'accord une première fois
/// par dossier (un dépôt téléchargé peut y cacher n'importe quoi).
pub fn runs_project_code(cmd: &str) -> bool {
    let c = cmd.trim();
    is_safe_command(c) && PROJECT_CODE.iter().any(|s| starts_with_command(c, s))
}

pub async fn exec_command(root: &Path, command: &str) -> Result<String, String> {
    exec_stream(root, command, |_| {}).await
}

pub(super) fn pump<R>(reader: R, tx: tokio::sync::mpsc::UnboundedSender<String>)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    use tokio::io::{AsyncBufReadExt, BufReader};
    tokio::spawn(async move {
        let mut reader = BufReader::new(reader);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buf).trim_end_matches(['\r', '\n']).to_string();
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            }
        }
    });
}

/// Exécute une commande en renvoyant la sortie au fil de l'eau (`on_output` reçoit la fin cumulée).
pub async fn exec_stream<F>(root: &Path, command: &str, mut on_output: F) -> Result<String, String>
where
    F: FnMut(&str),
{
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

    let deadline = tokio::time::sleep(EXEC_TIMEOUT);
    tokio::pin!(deadline);
    let mut text = String::new();
    loop {
        tokio::select! {
            line = rx.recv() => match line {
                Some(l) => {
                    text.push_str(&l);
                    text.push('\n');
                    if text.len() > 2 * MAX_OUTPUT {
                        text = truncate_tail(std::mem::take(&mut text), MAX_OUTPUT);
                    }
                    on_output(&truncate_tail(text.clone(), MAX_OUTPUT));
                }
                None => break,
            },
            _ = &mut deadline => {
                return Err(format!(
                    "Commande interrompue après {} s.\n{}",
                    EXEC_TIMEOUT.as_secs(),
                    truncate_tail(text, MAX_OUTPUT)
                ));
            }
        }
    }
    let code = child.wait().await.ok().and_then(|s| s.code()).unwrap_or(-1);
    Ok(truncate_tail(format!("{}[code de sortie : {code}]", condense_output(text)), MAX_OUTPUT))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> PathBuf {
        let d = std::env::temp_dir().join(format!("tools-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("a.txt"), "un\ndeux\ntrois\n").unwrap();
        d
    }

    #[test]
    fn read_file_numbers_lines() {
        let d = project();
        let Prepared::Done(out) = prepare(&d, "read_file", &json!({"path":"a.txt","offset":2,"limit":1})).unwrap() else { panic!() };
        assert!(out.contains("2\tdeux") && !out.contains("trois"));
    }

    #[test]
    fn edit_requires_unique_match_and_builds_diff() {
        let d = project();
        assert!(prepare(&d, "edit_file", &json!({"path":"a.txt","old":"absent","new":"x"})).is_err());
        let Prepared::Write { after, diff, .. } =
            prepare(&d, "edit_file", &json!({"path":"a.txt","old":"deux","new":"2"})).unwrap() else { panic!() };
        assert_eq!(after, "un\n2\ntrois\n");
        assert!(diff.contains("-deux") && diff.contains("+2"));
    }

    #[test]
    fn tools_refuse_paths_outside_and_git() {
        let d = project();
        assert!(prepare(&d, "read_file", &json!({"path":"../x"})).is_err());
        assert!(prepare(&d, "write_file", &json!({"path":".git/config","content":"x"})).is_err());
    }

    #[test]
    fn grep_finds_matches() {
        let d = project();
        let Prepared::Done(out) = prepare(&d, "grep", &json!({"pattern":"tro.s"})).unwrap() else { panic!() };
        assert!(out.contains("a.txt:3"));
    }

    #[tokio::test]
    async fn exec_stream_reports_output_and_exit_code() {
        let d = project();
        let mut seen = Vec::new();
        let out = exec_stream(&d, "Write-Output un; Write-Output deux; exit 3", |t| seen.push(t.to_string()))
            .await
            .unwrap();
        assert!(out.contains("un") && out.contains("deux") && out.contains("[code de sortie : 3]"));
        assert!(seen.len() >= 2, "la sortie doit arriver ligne par ligne");
    }

    #[test]
    fn apply_edit_exact_and_multiline_eol() {
        assert_eq!(apply_edit("a\nb\nc\n", "b", "B").unwrap(), "a\nB\nc\n");
        // un fichier CRLF garde ses fins de ligne
        assert_eq!(apply_edit("a\r\nb\r\nc\r\n", "b", "x\ny").unwrap(), "a\r\nx\r\ny\r\nc\r\n");
    }

    #[test]
    fn apply_edit_tolerates_indentation_and_reindents() {
        let file = "fn main() {\n    if ok {\n        run();\n    }\n}\n";
        // le modèle a cité l'extrait sans la bonne indentation
        let out = apply_edit(file, "if ok {\n    run();\n}", "if ok {\n    run();\n    log();\n}").unwrap();
        assert_eq!(out, "fn main() {\n    if ok {\n        run();\n        log();\n    }\n}\n");
    }

    #[test]
    fn apply_edit_reports_ambiguity_and_helpful_hint() {
        let e = apply_edit("x\ny\nx\n", "x", "z").unwrap_err();
        assert!(e.contains("2 fois") && e.contains("1, 3"), "{e}");
        let e = apply_edit("a\nlet v = 1;\nb\n", "let v = 1;\nlet w = 2;", "q").unwrap_err();
        assert!(e.contains("ligne 2") && e.contains("read_file"), "{e}");
        assert!(apply_edit("a", "  ", "b").is_err());
    }

    #[test]
    fn find_symbol_separates_definitions_and_uses() {
        let d = project();
        std::fs::write(d.join("a.rs"), "pub fn compute_total(x: u32) -> u32 { x }\n").unwrap();
        std::fs::write(d.join("b.js"), "const total = compute_total(3);\nfunction other() {}\n").unwrap();
        let Prepared::Done(out) = prepare(&d, "find_symbol", &json!({"name":"compute_total"})).unwrap() else { panic!() };
        let (defs, uses) = out.split_once("Utilisations").unwrap();
        assert!(defs.contains("a.rs:1") && !defs.contains("b.js"), "{out}");
        assert!(uses.contains("b.js:1"), "{out}");
        assert!(prepare(&d, "find_symbol", &json!({"name":"a b"})).is_err());
    }

    #[test]
    fn condense_keeps_errors_and_drops_noise() {
        let mut s = String::new();
        for i in 0..400 {
            s.push_str(&format!("   Compiling crate{i} v0.1.0 (some/long/path)\n"));
        }
        s.push_str("error[E0308]: mismatched types\n --> src/main.rs:4:5\n");
        for _ in 0..100 {
            s.push_str("remplissage sans intérêt\n");
        }
        s.push_str("aborting due to previous error\n");
        let out = condense_output(s.clone());
        assert!(out.len() < s.len() / 2 && out.contains("E0308") && out.contains("src/main.rs:4:5") && out.contains("aborting"));
        assert_eq!(condense_output("court".into()), "court");
    }

    #[test]
    fn verify_uses_detected_command() {
        let d = project();
        assert!(prepare(&d, "verify", &json!({})).is_err());
        std::fs::write(d.join("Cargo.toml"), "[package]").unwrap();
        let Prepared::Exec { command } = prepare(&d, "verify", &json!({"target":"test"})).unwrap() else { panic!() };
        assert_eq!(command, "cargo test");
    }

    #[test]
    fn safe_command_allowlist() {
        assert!(is_safe_command("git status"));
        assert!(is_safe_command("cargo test --lib"));
        assert!(is_safe_command("npm run check"));
        assert!(!is_safe_command("git status; rm -rf x"));
        assert!(!is_safe_command("npm install"));
        assert!(!is_safe_command("echo hi > f.txt"));
        // Lectures de git : jamais d'écriture de fichier ni de programme externe.
        assert!(is_safe_command("git diff HEAD~1 -- src") && is_safe_command("git log --oneline -5"));
        assert!(!is_safe_command("git diff --output=C:\\x.bat") && !is_safe_command("git log --output x") && !is_safe_command("git diff --ext-diff"));
        // Branches : lister oui, créer ou supprimer non.
        assert!(is_safe_command("git branch") && is_safe_command("git branch -a") && is_safe_command("git branch --show-current"));
        assert!(!is_safe_command("git branch -D main") && !is_safe_command("git branch nouvelle"));
        // Les commandes qui lancent du code du projet sont sûres mais à confirmer une fois par dossier.
        assert!(runs_project_code("cargo test --lib") && runs_project_code("npm run build"));
        assert!(!runs_project_code("git status") && !runs_project_code("echo hi") && !runs_project_code("rm -rf x"));
    }
}
