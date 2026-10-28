//! Intégration git minimale pour le mode Code : état du dépôt, diff d'un fichier, validation (commit).
//! Passe par la commande `git` du PC, sans shell (arguments séparés) et sans jamais pousser vers un serveur.

use std::path::Path;
use std::process::{Command, Stdio};

use serde::Serialize;

use crate::agent::sandbox;

const MAX_DIFF: usize = 200_000;

#[derive(Debug, Serialize, PartialEq)]
pub struct GitFile {
    pub path: String,
    /// "M" modifié, "A" ajouté, "D" supprimé, "R" renommé, "?" non suivi, "U" conflit.
    pub status: String,
    pub staged: bool,
}

#[derive(Debug, Serialize, Default)]
pub struct GitStatus {
    pub repo: bool,
    pub branch: String,
    pub files: Vec<GitFile>,
}

fn run(root: &Path, args: &[&str], ok_codes: &[i32]) -> Result<String, String> {
    let mut cmd = Command::new("git");
    // Un dépôt piégé peut déclarer dans sa configuration un programme à lancer à chaque `git status` (fsmonitor)
    // ou à chaque diff (diff externe, filtres de texte) : on neutralise ces réglages, quelle que soit la configuration du dépôt.
    cmd.arg("-C").arg(root).args(["-c", "core.quotepath=false", "-c", "core.fsmonitor=false", "-c", "core.pager=cat"]).args(args);
    cmd.env("GIT_TERMINAL_PROMPT", "0").env("LC_ALL", "C").stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().map_err(|e| format!("git est introuvable sur ce PC : {e}"))?;
    let code = out.status.code().unwrap_or(-1);
    if out.status.success() || ok_codes.contains(&code) {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Err(if err.is_empty() { format!("git a échoué (code {code})") } else { err })
}

fn kind(c: char) -> &'static str {
    match c {
        'M' | 'T' => "M",
        'A' => "A",
        'D' => "D",
        'R' | 'C' => "R",
        'U' => "U",
        _ => "",
    }
}

/// Analyse la sortie de `git status --porcelain=v1 -b -z`.
fn parse_status(raw: &str) -> GitStatus {
    let mut parts = raw.split('\0').filter(|p| !p.is_empty());
    let mut st = GitStatus { repo: true, ..Default::default() };
    let mut skip_next = false;
    while let Some(entry) = parts.next() {
        if let Some(head) = entry.strip_prefix("## ") {
            let name = head.split("...").next().unwrap_or(head);
            st.branch = name.strip_prefix("No commits yet on ").or_else(|| name.strip_prefix("Initial commit on ")).unwrap_or(name).to_string();
            continue;
        }
        if skip_next {
            skip_next = false;
            continue;
        }
        let mut chars = entry.chars();
        let (x, y) = (chars.next().unwrap_or(' '), chars.next().unwrap_or(' '));
        let path: String = entry.chars().skip(3).collect();
        if x == 'R' || x == 'C' {
            skip_next = true; // l'ancien nom suit dans l'entrée suivante
        }
        if x == '?' && y == '?' {
            st.files.push(GitFile { path, status: "?".into(), staged: false });
        } else {
            let (code, staged) = if !kind(x).is_empty() { (kind(x), true) } else { (kind(y), false) };
            if !code.is_empty() {
                st.files.push(GitFile { path, status: code.into(), staged });
            }
        }
    }
    st
}

pub fn status(root: &Path) -> Result<GitStatus, String> {
    if run(root, &["rev-parse", "--is-inside-work-tree"], &[]).is_err() {
        return Ok(GitStatus::default());
    }
    Ok(parse_status(&run(root, &["status", "--porcelain=v1", "-b", "-z"], &[])?))
}

fn cap(mut text: String) -> String {
    if text.len() > MAX_DIFF {
        let cut = (0..=MAX_DIFF).rev().find(|i| text.is_char_boundary(*i)).unwrap_or(0);
        text.truncate(cut);
        text.push_str("\n[… diff tronqué …]");
    }
    text
}

/// Diff d'un fichier par rapport au dernier commit ; un fichier non suivi s'affiche comme entièrement ajouté.
pub fn diff(root: &Path, rel: &str) -> Result<String, String> {
    let path = sandbox::resolve(root, rel)?;
    if sandbox::is_protected(&path) {
        return Err("Le dossier .git est protégé.".into());
    }
    let root_c = root.canonicalize().map_err(|e| e.to_string())?;
    let rel = path.strip_prefix(&root_c).map_err(|_| "Chemin hors du projet.".to_string())?.to_string_lossy().replace('\\', "/");
    let tracked = run(root, &["ls-files", "--error-unmatch", "--", &rel], &[]).is_ok();
    let text = if tracked {
        run(root, &["diff", "--no-ext-diff", "--no-textconv", "HEAD", "--", &rel], &[]).or_else(|_| run(root, &["diff", "--no-ext-diff", "--no-textconv", "--", &rel], &[]))?
    } else {
        // Code de sortie 1 = « des différences existent » : normal ici.
        run(root, &["diff", "--no-index", "--no-ext-diff", "--no-textconv", "--", "/dev/null", &rel], &[1])?
    };
    Ok(cap(text))
}

const MAX_REVIEW: usize = 120_000;

/// Fichiers qu'on ne fait pas relire : générés ou énormes sans intérêt.
fn is_noise(path: &str) -> bool {
    let p = path.to_lowercase();
    ["package-lock.json", "yarn.lock", "pnpm-lock.yaml", "cargo.lock", "composer.lock", "gemfile.lock", ".min.js", ".min.css", ".map", ".svg", ".png", ".jpg", ".ico"].iter().any(|s| p.ends_with(s))
}

/// Tous les changements en cours (indexés ou non, fichiers neufs compris) sous forme d'un seul diff à relire.
pub fn review_diff(root: &Path) -> Result<String, String> {
    if run(root, &["rev-parse", "--is-inside-work-tree"], &[]).is_err() {
        return Err("Ce dossier n'est pas un dépôt git : la revue relit les changements git.".into());
    }
    let mut text = match run(root, &["diff", "--no-ext-diff", "--no-textconv", "HEAD"], &[]) {
        Ok(t) => t,
        // Dépôt sans aucun commit : pas de HEAD.
        Err(_) => run(root, &["diff", "--no-ext-diff", "--no-textconv", "--cached"], &[])? + &run(root, &["diff", "--no-ext-diff", "--no-textconv"], &[])?,
    };
    let list = run(root, &["ls-files", "--others", "--exclude-standard", "-z"], &[])?;
    for rel in list.split('\0').filter(|s| !s.is_empty()).take(40) {
        // `symlink_metadata` : un lien (ou une jonction) vers un fichier privé hors du projet ne doit pas être lu et envoyé au modèle.
        let Ok(meta) = std::fs::symlink_metadata(root.join(rel)) else { continue };
        if !meta.is_file() || meta.len() > 100_000 || is_noise(rel) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(root.join(rel)) else { continue }; // binaire : ignoré
        text.push_str(&format!("diff --git a/{rel} b/{rel}\nnew file mode 100644\n--- /dev/null\n+++ b/{rel}\n@@ -0,0 +1,{} @@\n", content.lines().count()));
        for l in content.lines() {
            text.push('+');
            text.push_str(l);
            text.push('\n');
        }
    }
    // Retire les fichiers sans intérêt, section par section.
    let kept: Vec<&str> = text
        .split("\ndiff --git ")
        .enumerate()
        .filter(|(i, sec)| {
            let head = if *i == 0 { sec.strip_prefix("diff --git ").unwrap_or(sec) } else { sec };
            let path = head.split_whitespace().next().unwrap_or("").trim_start_matches("a/");
            !is_noise(path)
        })
        .map(|(_, s)| s)
        .collect();
    let mut out = kept.join("\ndiff --git ");
    if out.len() > MAX_REVIEW {
        let cut = (0..=MAX_REVIEW).rev().find(|i| out.is_char_boundary(*i)).unwrap_or(0);
        out.truncate(cut);
        out.push_str("\n[… diff tronqué : seuls les premiers fichiers sont relus …]");
    }
    Ok(out)
}

/// Ajoute tous les changements et crée un commit. Ne pousse rien.
/// Crée une branche de travail et s'y place (les fichiers modifiés suivent).
pub fn branch_create(root: &Path, name: &str) -> Result<String, String> {
    let name = name.trim();
    let ok = !name.is_empty()
        && name.len() <= 60
        && !name.starts_with(['-', '/', '.'])
        && !name.contains("..")
        && !name.ends_with(['/', '.'])
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'));
    if !ok {
        return Err("Nom de branche invalide (lettres, chiffres, . _ - /).".into());
    }
    run(root, &["checkout", "-b", name], &[])?;
    Ok(name.to_string())
}

pub fn commit(root: &Path, message: &str) -> Result<String, String> {
    let message = message.trim();
    if message.is_empty() {
        return Err("Écris un message de validation.".into());
    }
    if message.len() > 4_000 {
        return Err("Message trop long.".into());
    }
    run(root, &["add", "-A"], &[])?;
    let out = run(root, &["commit", "-m", message], &[])?;
    Ok(out.lines().next().unwrap_or("Validé.").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_diff_covers_modified_new_and_skips_lockfiles() {
        if !git_available() {
            return;
        }
        let d = repo();
        std::fs::write(d.join("a.txt"), "un\n").unwrap();
        std::fs::write(d.join("package-lock.json"), "{}\n").unwrap();
        run(&d, &["add", "-A"], &[]).unwrap();
        run(&d, &["commit", "-q", "-m", "init"], &[]).unwrap();
        std::fs::write(d.join("a.txt"), "un\ndeux\n").unwrap();
        std::fs::write(d.join("package-lock.json"), "{\"x\":1}\n").unwrap();
        std::fs::write(d.join("neuf.rs"), "fn main() {}\n").unwrap();
        let out = review_diff(&d).unwrap();
        assert!(review_diff(&std::env::temp_dir().join("nexiste-pas-xyz")).unwrap_err().contains("pas un dépôt"));
        assert!(out.contains("+deux") && out.contains("b/neuf.rs") && out.contains("+fn main() {}"), "{out}");
        assert!(!out.contains("package-lock"), "{out}");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn branch_names_are_validated() {
        let d = std::env::temp_dir();
        for bad in ["", "-x", "a..b", "a b", "x;rm", "/x"] {
            assert!(branch_create(&d, bad).is_err(), "{bad}");
        }
    }

    fn git_available() -> bool {
        Command::new("git").arg("--version").output().is_ok()
    }

    fn repo() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("git-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        for args in [&["init", "-q"][..], &["config", "user.name", "Test"], &["config", "user.email", "t@example.com"], &["config", "commit.gpgsign", "false"]] {
            run(&d, args, &[]).unwrap();
        }
        d
    }

    /// Un dépôt qui déclare un programme à lancer à chaque `git status` ne doit rien lancer via l'application.
    #[test]
    fn a_booby_trapped_repository_cannot_start_programs() {
        if !git_available() {
            return;
        }
        let d = repo();
        std::fs::write(d.join("a.txt"), "un\n").unwrap();
        let marker = d.join("lance.txt");
        let hook = format!("touch {}", marker.display().to_string().replace('\\', "/"));
        run(&d, &["config", "core.fsmonitor", &hook], &[]).unwrap();
        // Contrôle : git lancé tel quel exécute bien le programme (sinon ce test ne prouverait rien).
        let _ = Command::new("git").arg("-C").arg(&d).args(["status", "--porcelain"]).output();
        let trap_works = marker.exists();
        let _ = std::fs::remove_file(&marker);
        status(&d).unwrap();
        review_diff(&d).ok();
        assert!(!marker.exists(), "le programme du dépôt a été lancé par l'application");
        if !trap_works {
            eprintln!("(cette version de git n'a pas lancé le piège de contrôle)");
        }
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn parses_status_entries_including_renames_and_untracked() {
        let raw = "## main...origin/main\0 M src/a.rs\0A  new.txt\0R  b.rs\0a.rs\0?? ignored dir/x.md\0 D gone.txt\0";
        let st = parse_status(raw);
        assert_eq!(st.branch, "main");
        let got: Vec<(&str, &str, bool)> = st.files.iter().map(|f| (f.path.as_str(), f.status.as_str(), f.staged)).collect();
        assert_eq!(
            got,
            [("src/a.rs", "M", false), ("new.txt", "A", true), ("b.rs", "R", true), ("ignored dir/x.md", "?", false), ("gone.txt", "D", false)]
        );
        assert_eq!(parse_status("## No commits yet on master\0").branch, "master");
    }

    #[test]
    fn not_a_repo_is_reported_without_error() {
        if !git_available() {
            return;
        }
        let d = std::env::temp_dir().join(format!("nogit-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        let st = status(&d).unwrap();
        assert!(!st.repo && st.files.is_empty());
    }

    #[test]
    fn full_cycle_status_diff_commit() {
        if !git_available() {
            return;
        }
        let d = repo();
        std::fs::write(d.join("a.txt"), "un\n").unwrap();
        let st = status(&d).unwrap();
        assert!(st.repo && st.files.len() == 1 && st.files[0].status == "?");
        assert!(diff(&d, "a.txt").unwrap().contains("+un"), "non suivi : tout est ajouté");
        assert!(commit(&d, "  ").is_err());
        commit(&d, "premier commit").unwrap();
        assert!(status(&d).unwrap().files.is_empty());

        std::fs::write(d.join("a.txt"), "un\ndeux\n").unwrap();
        let st = status(&d).unwrap();
        assert_eq!((st.files[0].status.as_str(), st.files[0].staged), ("M", false));
        let diff_text = diff(&d, "a.txt").unwrap();
        assert!(diff_text.contains("+deux") && diff_text.contains("@@"));
        assert!(diff(&d, "../a.txt").is_err() && diff(&d, ".git/config").is_err());
        commit(&d, "second").unwrap();
        assert!(status(&d).unwrap().files.is_empty());
    }
}
