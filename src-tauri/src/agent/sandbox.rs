use std::path::{Component, Path, PathBuf};
use std::sync::RwLock;

/// Dossiers que l'utilisateur autorise en plus du dossier de travail (Réglages → Ordinateur), pour le profil ouvert.
static EXTRA_ROOTS: RwLock<Vec<PathBuf>> = RwLock::new(Vec::new());

/// Remplace la liste des dossiers autorisés ; seuls les dossiers existants sont gardés (chemins canoniques).
pub fn set_extra_roots(dirs: &[String]) {
    let roots: Vec<PathBuf> = dirs.iter().filter_map(|d| Path::new(d.trim()).canonicalize().ok()).filter(|p| p.is_dir()).collect();
    if let Ok(mut w) = EXTRA_ROOTS.write() {
        *w = roots;
    }
}

pub fn extra_roots() -> Vec<PathBuf> {
    EXTRA_ROOTS.read().map(|r| r.clone()).unwrap_or_default()
}

/// Résout `rel` dans `root` et refuse tout chemin qui en sortirait
/// (`..`, chemin absolu extérieur, lien symbolique pointant dehors).
pub fn resolve(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|e| format!("Dossier de travail invalide ({}) : {e}", root.display()))?;
    // Les modèles écrivent souvent « / » pour la racine du projet : on le lit comme relatif.
    let rel = if Path::new(rel).components().any(|c| matches!(c, Component::Prefix(_))) {
        rel
    } else {
        let t = rel.trim_start_matches(['/', '\\']);
        if t.is_empty() { "." } else { t }
    };
    let p = Path::new(rel);
    let joined = if p.is_absolute() { p.to_path_buf() } else { root.join(p) };

    let mut out = PathBuf::new();
    for c in joined.components() {
        match c {
            Component::ParentDir => {
                if !out.pop() {
                    return Err(format!("Chemin hors du dossier de travail : {rel}"));
                }
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }

    // Résout les liens symboliques sur le plus long préfixe existant.
    let mut existing = out.clone();
    let mut tail = Vec::new();
    while !existing.exists() {
        match existing.file_name() {
            Some(n) => tail.push(n.to_owned()),
            None => break,
        }
        if !existing.pop() {
            break;
        }
    }
    let mut full = existing.canonicalize().map_err(|e| e.to_string())?;
    for t in tail.iter().rev() {
        full.push(t);
    }

    if !full.starts_with(&root) && !extra_roots().iter().any(|r| full.starts_with(r)) {
        return Err(format!("Chemin hors du dossier de travail et des dossiers autorisés : {rel}"));
    }
    Ok(full)
}

/// Les modifications ne doivent jamais toucher au dépôt git lui-même.
pub fn is_protected(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == ".git")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("sandbox-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(d.join("src")).unwrap();
        std::fs::write(d.join("src/a.txt"), "x").unwrap();
        d
    }

    #[test]
    fn accepts_paths_inside_root_including_new_files() {
        let d = tmp();
        assert!(resolve(&d, "src/a.txt").is_ok());
        assert!(resolve(&d, "src/new/deep/file.rs").is_ok());
        assert!(resolve(&d, "./src/../src/a.txt").is_ok());
    }

    #[test]
    fn leading_slash_means_project_root() {
        let d = tmp();
        assert_eq!(resolve(&d, "/").unwrap(), d.canonicalize().unwrap());
        assert!(resolve(&d, "/src/a.txt").is_ok());
        assert!(resolve(&d, "/../x").is_err());
    }

    #[test]
    fn rejects_traversal_and_outside_absolute() {
        let d = tmp();
        assert!(resolve(&d, "../etc/passwd").is_err());
        assert!(resolve(&d, "src/../../x").is_err());
        let outside = std::env::temp_dir().join("other.txt");
        assert!(resolve(&d, outside.to_str().unwrap()).is_err());
    }

    #[test]
    fn extra_roots_allow_absolute_paths_only_there() {
        let (a, b) = (tmp(), tmp());
        let f = b.join("src/a.txt");
        assert!(resolve(&a, f.to_str().unwrap()).is_err());
        set_extra_roots(&[b.to_string_lossy().into_owned(), "Z:\n-existe-pas".into()]);
        assert_eq!(extra_roots().len(), 1);
        assert!(resolve(&a, f.to_str().unwrap()).is_ok());
        assert!(resolve(&a, b.join("nouveau/x.txt").to_str().unwrap()).is_ok());
        let c = tmp();
        assert!(resolve(&a, c.join("src/a.txt").to_str().unwrap()).is_err());
        set_extra_roots(&[]);
        assert!(resolve(&a, f.to_str().unwrap()).is_err());
    }

    #[test]
    fn protects_git_dir() {
        assert!(is_protected(Path::new("C:/p/.git/config")));
        assert!(!is_protected(Path::new("C:/p/src/main.rs")));
    }
}
