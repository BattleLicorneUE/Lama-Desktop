//! Détection du type de projet et contexte du dépôt donné à l'agent
//! (commandes de vérification, état git, fichiers de style).

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Commands {
    pub kind: &'static str,
    pub check: Option<String>,
    pub test: Option<String>,
    pub format: Option<String>,
}

fn npm_commands(root: &Path) -> Option<Commands> {
    let text = std::fs::read_to_string(root.join("package.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let scripts = v.get("scripts").and_then(|s| s.as_object());
    let has = |n: &str| scripts.is_some_and(|s| s.get(n).and_then(|v| v.as_str()).is_some_and(|c| !c.contains("no test specified")));
    let run = |n: &str| if n == "test" { "npm test".to_string() } else { format!("npm run {n}") };
    let pick = |names: &[&str]| names.iter().find(|n| has(n)).map(|n| run(n));
    Some(Commands {
        kind: "Node.js",
        check: pick(&["check", "typecheck", "lint", "build"]),
        test: pick(&["test"]),
        format: pick(&["format"]),
    })
}

/// Écosystèmes détectés à la racine du projet.
pub fn detect(root: &Path) -> Vec<Commands> {
    let mut out = Vec::new();
    if root.join("Cargo.toml").exists() {
        out.push(Commands {
            kind: "Rust",
            check: Some("cargo check".into()),
            test: Some("cargo test".into()),
            format: Some("cargo fmt".into()),
        });
    }
    if let Some(c) = npm_commands(root) {
        out.push(c);
    }
    let py = ["pyproject.toml", "requirements.txt", "setup.py"].iter().any(|f| root.join(f).exists());
    if py {
        let has_tests = root.join("tests").is_dir() || root.join("test").is_dir();
        out.push(Commands {
            kind: "Python",
            check: Some("python -m compileall -q .".into()),
            test: has_tests.then(|| "python -m pytest -q".to_string()),
            format: None,
        });
    }
    if root.join("go.mod").exists() {
        out.push(Commands {
            kind: "Go",
            check: Some("go build ./...".into()),
            test: Some("go test ./...".into()),
            format: Some("gofmt -l .".into()),
        });
    }
    let dotnet = std::fs::read_dir(root).is_ok_and(|d| {
        d.filter_map(Result::ok).any(|e| {
            let n = e.file_name().to_string_lossy().to_lowercase();
            n.ends_with(".sln") || n.ends_with(".csproj")
        })
    });
    if dotnet {
        out.push(Commands { kind: ".NET", check: Some("dotnet build".into()), test: Some("dotnet test".into()), format: None });
    }
    out
}

/// Commande de vérification demandée (`check`, `test` ou `format`), du premier écosystème qui la fournit.
pub fn command_for(root: &Path, target: &str) -> Option<String> {
    detect(root).into_iter().find_map(|c| match target {
        "check" => c.check,
        "test" => c.test,
        "format" => c.format,
        _ => None,
    })
}

const HIDDEN: [&str; 6] = [".git", "node_modules", "target", ".svelte-kit", "build", "dist"];
const STYLE_FILES: [&str; 6] = [".editorconfig", "rustfmt.toml", ".prettierrc", ".prettierrc.json", "eslint.config.js", ".eslintrc.json"];

fn git_status(root: &Path) -> Option<String> {
    use std::os::windows::process::CommandExt;
    if !root.join(".git").exists() {
        return None;
    }
    let out = std::process::Command::new("git")
        .args(["status", "--short", "--branch"])
        .current_dir(root)
        .creation_flags(0x0800_0000)
        .output()
        .ok()?;
    let mut s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    s.truncate(s.char_indices().nth(1500).map_or(s.len(), |(i, _)| i));
    (!s.is_empty()).then_some(s)
}

/// Bloc de contexte ajouté à la consigne système de l'agent.
pub fn context(root: &Path) -> String {
    let mut s = String::new();

    if let Ok(rd) = std::fs::read_dir(root) {
        let mut names: Vec<String> = rd
            .filter_map(Result::ok)
            .map(|e| {
                let dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                format!("{}{}", e.file_name().to_string_lossy(), if dir { "/" } else { "" })
            })
            .filter(|n| !HIDDEN.contains(&n.trim_end_matches('/')))
            .collect();
        names.sort();
        names.truncate(40);
        s.push_str(&format!("\n\nRacine du projet :\n{}", names.join("  ")));
    }

    let cmds = detect(root);
    if !cmds.is_empty() {
        s.push_str("\n\nCommandes de vérification détectées (outil verify) :");
        for c in &cmds {
            s.push_str(&format!(
                "\n- {} : vérifier = {}, tests = {}, formater = {}",
                c.kind,
                c.check.as_deref().unwrap_or("—"),
                c.test.as_deref().unwrap_or("—"),
                c.format.as_deref().unwrap_or("—"),
            ));
        }
    }

    if let Some(g) = git_status(root) {
        s.push_str(&format!("\n\nÉtat git :\n{g}"));
    }

    let mut style = String::new();
    for f in STYLE_FILES {
        if let Ok(mut text) = std::fs::read_to_string(root.join(f)) {
            text.truncate(text.char_indices().nth(600).map_or(text.len(), |(i, _)| i));
            style.push_str(&format!("\n[{f}]\n{}\n", text.trim()));
        }
    }
    if !style.is_empty() {
        s.push_str("\n\nConfiguration de style du projet (à respecter) :");
        s.push_str(&style);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("proj-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn detects_rust_and_node_scripts() {
        let d = tmp();
        std::fs::write(d.join("Cargo.toml"), "[package]").unwrap();
        std::fs::write(
            d.join("package.json"),
            r#"{"scripts":{"check":"svelte-check","test":"echo \"Error: no test specified\" && exit 1","format":"prettier -w ."}}"#,
        )
        .unwrap();
        let c = detect(&d);
        assert_eq!(c.len(), 2);
        assert_eq!(c[1].check.as_deref(), Some("npm run check"));
        assert_eq!(c[1].test, None, "le script de test par défaut doit être ignoré");
        assert_eq!(command_for(&d, "check").as_deref(), Some("cargo check"));
        assert_eq!(command_for(&d, "format").as_deref(), Some("cargo fmt"));
    }

    #[test]
    fn empty_project_has_no_commands_and_context_lists_files() {
        let d = tmp();
        std::fs::write(d.join("notes.txt"), "x").unwrap();
        std::fs::create_dir(d.join("node_modules")).unwrap();
        assert!(detect(&d).is_empty());
        let ctx = context(&d);
        assert!(ctx.contains("notes.txt") && !ctx.contains("node_modules"));
    }

    #[test]
    fn context_includes_style_files() {
        let d = tmp();
        std::fs::write(d.join(".editorconfig"), "indent_size = 2").unwrap();
        assert!(context(&d).contains("indent_size = 2"));
    }
}
