//! Réglages de l'application (hors profil) : fonctionnement en arrière-plan et lancement avec Windows.

use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

const NO_WINDOW: u32 = 0x0800_0000;
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "LamaDesktop";

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct Settings {
    /// Fermer la fenêtre la cache dans la zone de notification au lieu de quitter.
    pub keep_background: bool,
}

pub fn load(dir: &Path) -> Settings {
    std::fs::read_to_string(dir.join("settings.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save(dir: &Path, s: &Settings) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("settings.json"), serde_json::to_string_pretty(s).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn reg(args: &[&str]) -> Result<std::process::Output, String> {
    Command::new("reg").args(args).creation_flags(NO_WINDOW).output().map_err(|e| format!("reg indisponible : {e}"))
}

pub fn autostart_enabled() -> bool {
    reg(&["query", RUN_KEY, "/v", VALUE]).is_ok_and(|o| o.status.success())
}

/// Ajoute ou retire l'entrée « lancer avec Windows » de l'utilisateur (aucun droit administrateur requis).
pub fn set_autostart(on: bool) -> Result<(), String> {
    let out = if on {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let cmd = format!("\"{}\" --background", exe.display());
        reg(&["add", RUN_KEY, "/v", VALUE, "/t", "REG_SZ", "/d", &cmd, "/f"])?
    } else if autostart_enabled() {
        reg(&["delete", RUN_KEY, "/v", VALUE, "/f"])?
    } else {
        return Ok(());
    };
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("Réglage du démarrage impossible : {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Touche au vrai registre de l'utilisateur : `cargo test autostart -- --ignored`.
    #[test]
    #[ignore]
    fn autostart_roundtrip_in_the_registry() {
        let before = autostart_enabled();
        set_autostart(true).unwrap();
        assert!(autostart_enabled());
        set_autostart(false).unwrap();
        assert!(!autostart_enabled());
        set_autostart(before).unwrap();
    }

    #[test]
    fn settings_roundtrip() {
        let d = std::env::temp_dir().join(format!("appset-{}", uuid::Uuid::new_v4()));
        assert_eq!(load(&d), Settings::default());
        save(&d, &Settings { keep_background: true }).unwrap();
        assert!(load(&d).keep_background);
        let _ = std::fs::remove_dir_all(d);
    }
}
