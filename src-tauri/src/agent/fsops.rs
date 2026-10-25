//! Opérations sur les fichiers et dossiers du projet, partagées par l'agent et par l'arbre de fichiers.
//! Tout chemin passe par le bac à sable : jamais en dehors du dossier de travail ni dans `.git`.

use std::path::{Path, PathBuf};

use super::sandbox;

/// Chemin résolu dans le projet, hors `.git`.
fn guard(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let p = sandbox::resolve(root, rel)?;
    if sandbox::is_protected(&p) {
        return Err("Le dossier .git est protégé.".into());
    }
    Ok(p)
}

fn refuse_root(root: &Path, p: &Path) -> Result<(), String> {
    if root.canonicalize().is_ok_and(|r| r == p) {
        return Err("Impossible de toucher à la racine du projet.".into());
    }
    Ok(())
}

/// Un nom d'élément seul (pas de chemin) et valide sous Windows.
pub fn check_name(name: &str) -> Result<&str, String> {
    let n = name.trim();
    if n.is_empty() || n == "." || n == ".." || n.ends_with('.') || n.chars().any(|c| "/\\<>:\"|?*".contains(c) || c.is_control()) {
        return Err(format!("Nom invalide : « {name} »"));
    }
    Ok(n)
}

/// Chemin d'un nouvel élément (n'existe pas encore).
pub fn new_target(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let p = guard(root, rel)?;
    refuse_root(root, &p)?;
    if p.exists() {
        return Err(format!("« {rel} » existe déjà."));
    }
    Ok(p)
}

/// Crée un fichier vide ou un dossier (avec ses dossiers parents).
pub fn create(root: &Path, rel: &str, is_dir: bool) -> Result<PathBuf, String> {
    let p = new_target(root, rel)?;
    if is_dir {
        std::fs::create_dir_all(&p)
    } else {
        p.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|()| std::fs::File::create_new(&p).map(|_| ()))
    }
    .map_err(|e| format!("Création impossible : {e}"))?;
    Ok(p)
}

/// Chemin d'un élément existant à supprimer ou déplacer.
pub fn existing_target(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let p = guard(root, rel)?;
    refuse_root(root, &p)?;
    if !p.exists() {
        return Err(format!("« {rel} » n'existe pas."));
    }
    Ok(p)
}

/// Source et destination d'un déplacement ; la destination ne doit pas exister.
pub fn move_targets(root: &Path, from: &str, to: &str) -> Result<(PathBuf, PathBuf), String> {
    let src = existing_target(root, from)?;
    let dst = new_target(root, to)?;
    if dst.starts_with(&src) {
        return Err("Impossible de déplacer un dossier dans lui-même.".into());
    }
    Ok((src, dst))
}

/// Source et destination d'un renommage (même dossier, nouveau nom).
pub fn rename_targets(root: &Path, rel: &str, new_name: &str) -> Result<(PathBuf, PathBuf), String> {
    let src = existing_target(root, rel)?;
    let dst = src.with_file_name(check_name(new_name)?);
    if dst.exists() {
        return Err(format!("« {new_name} » existe déjà dans ce dossier."));
    }
    Ok((src, dst))
}

pub fn do_move(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Déplacement impossible : {e}"))?;
    }
    std::fs::rename(from, to).map_err(|e| format!("Déplacement impossible : {e}"))
}

/// Envoie à la corbeille : récupérable, contrairement à une suppression définitive.
pub fn to_trash(p: &Path) -> Result<(), String> {
    trash::delete(p).map_err(|e| format!("Mise à la corbeille impossible : {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("fsops-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(d.join("src")).unwrap();
        std::fs::write(d.join("src/a.txt"), "x").unwrap();
        d
    }

    #[test]
    fn creates_files_and_folders_but_never_overwrites() {
        let d = tmp();
        create(&d, "docs/guide/intro.md", false).unwrap();
        assert!(d.join("docs/guide/intro.md").is_file());
        create(&d, "assets/img", true).unwrap();
        assert!(d.join("assets/img").is_dir());
        assert!(create(&d, "src/a.txt", false).is_err());
    }

    #[test]
    fn refuses_outside_git_and_root() {
        let d = tmp();
        assert!(create(&d, "../evil.txt", false).is_err());
        assert!(create(&d, ".git/hooks/x", false).is_err());
        assert!(existing_target(&d, "/").is_err());
        assert!(existing_target(&d, "nope.txt").is_err());
    }

    #[test]
    fn rename_validates_the_name() {
        let d = tmp();
        assert!(rename_targets(&d, "src/a.txt", "../b.txt").is_err());
        assert!(rename_targets(&d, "src/a.txt", "a:b").is_err());
        assert!(rename_targets(&d, "src/a.txt", " ").is_err());
        let (from, to) = rename_targets(&d, "src/a.txt", "b.txt").unwrap();
        do_move(&from, &to).unwrap();
        assert!(d.join("src/b.txt").is_file() && !d.join("src/a.txt").exists());
    }

    #[test]
    fn move_refuses_existing_target_and_self_nesting() {
        let d = tmp();
        std::fs::write(d.join("b.txt"), "y").unwrap();
        assert!(move_targets(&d, "src/a.txt", "b.txt").is_err());
        assert!(move_targets(&d, "src", "src/inner").is_err());
        let (from, to) = move_targets(&d, "src/a.txt", "lib/a.txt").unwrap();
        do_move(&from, &to).unwrap();
        assert!(d.join("lib/a.txt").is_file());
    }
}
