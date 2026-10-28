//! `uv` et `uvx` pour les connecteurs MCP écrits en Python (Blender, pages web, Unity…).
//! S'ils ne sont pas installés sur le PC, Lama télécharge sa propre copie, vérifiée par somme de contrôle,
//! dans son dossier de données : l'utilisateur n'a rien à installer.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::OnceLock;

const VERSION: &str = "0.9.7";
/// Somme SHA-256 de `uv-x86_64-pc-windows-msvc.zip` (publiée par l'éditeur avec la version).
const SHA256: &str = "5d250c32d3604e28dbe18dc65c668ff628c53e00dde2c642576e831e4a60da64";

static DIR: OnceLock<PathBuf> = OnceLock::new();
static BUSY: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Dossier de données de l'application, appelé une fois au démarrage.
pub fn set_data_dir(data: &Path) {
    let _ = DIR.set(data.join("tools").join("uv"));
}

fn installed(dir: &Path) -> bool {
    dir.join("uvx.exe").is_file() && dir.join("uv.exe").is_file()
}

/// Dossier qui contient `uvx.exe`, après l'avoir téléchargé si besoin.
pub async fn ensure() -> Result<PathBuf, String> {
    let dir = DIR.get().ok_or("Dossier de données inconnu.")?.clone();
    // Plusieurs connecteurs peuvent démarrer ensemble : un seul télécharge, les autres attendent.
    let _one_at_a_time = BUSY.lock().await;
    if installed(&dir) {
        return Ok(dir);
    }
    let zip = dir.with_file_name(format!("uv-{VERSION}.zip"));
    let url = format!("https://github.com/astral-sh/uv/releases/download/{VERSION}/uv-x86_64-pc-windows-msvc.zip");
    crate::localai::download(&url, &zip, SHA256, &AtomicBool::new(false), |_, _, _| {}).await.map_err(|e| format!("Téléchargement de uv impossible : {e}"))?;
    let (z, d) = (zip.clone(), dir.clone());
    let unpacked = tokio::task::spawn_blocking(move || {
        let _ = std::fs::remove_dir_all(&d);
        crate::localai::unzip(&z, &d)
    })
    .await
    .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&zip);
    if let Err(e) = unpacked {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(format!("Décompression de uv impossible : {e}"));
    }
    if installed(&dir) {
        Ok(dir)
    } else {
        let _ = std::fs::remove_dir_all(&dir);
        Err("L'archive de uv ne contient pas uvx.exe.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pinned_hash_looks_like_a_sha256() {
        assert_eq!(SHA256.len(), 64);
        assert!(SHA256.chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// Télécharge vraiment uv (20 Mo) et le lance : `cargo test downloads_a_working_uv -- --ignored`.
    #[test]
    #[ignore]
    fn downloads_a_working_uv() {
        let d = std::env::temp_dir().join(format!("uvrt-{}", uuid::Uuid::new_v4()));
        let _ = DIR.set(d.join("tools").join("uv"));
        let dir = tauri::async_runtime::block_on(ensure()).unwrap();
        let out = std::process::Command::new(dir.join("uvx.exe")).arg("--version").output().unwrap();
        assert!(String::from_utf8_lossy(&out.stdout).contains(VERSION), "{out:?}");
        let _ = std::fs::remove_dir_all(d);
    }
}
