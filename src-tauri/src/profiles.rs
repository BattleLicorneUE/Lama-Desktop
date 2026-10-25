//! Profils (comme sur une plateforme de streaming) : chaque profil a son propre dossier de données —
//! historique, projets, mémoire, skills, agents, connecteurs MCP — qui n'est jamais partagé avec les autres.
//!
//! Organisation : `<données>/profiles.json` (la liste) et `<données>/profiles/<id>/…` (les données de chacun).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub color: String,
    /// Profil créé à partir des données d'avant les profils : il récupère aussi les anciens réglages de l'interface.
    #[serde(default)]
    pub legacy: bool,
    /// Code PIN facultatif (empreinte salée). Jamais envoyé à l'interface : voir `PublicProfile`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pin_salt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pin_hash: Option<String>,
}

/// Ce que l'interface voit d'un profil : jamais l'empreinte du code, seulement s'il y en a un.
#[derive(Debug, Clone, Serialize)]
pub struct PublicProfile {
    pub id: String,
    pub name: String,
    pub color: String,
    pub legacy: bool,
    pub locked: bool,
}

impl From<&Profile> for PublicProfile {
    fn from(p: &Profile) -> Self {
        Self { id: p.id.clone(), name: p.name.clone(), color: p.color.clone(), legacy: p.legacy, locked: p.pin_hash.is_some() }
    }
}

fn hash_pin(salt: &str, pin: &str) -> String {
    let mut h = Sha256::digest(format!("{salt}:{pin}").as_bytes());
    for _ in 0..20_000 {
        h = Sha256::digest(h);
    }
    h.iter().map(|b| format!("{b:02x}")).collect()
}

fn check_pin_format(pin: &str) -> Result<&str, String> {
    let p = pin.trim();
    if !(4..=12).contains(&p.len()) || !p.chars().all(|c| c.is_ascii_digit()) {
        return Err("Le code doit contenir de 4 à 12 chiffres.".into());
    }
    Ok(p)
}

fn pin_matches(p: &Profile, pin: Option<&str>) -> bool {
    match (&p.pin_salt, &p.pin_hash) {
        (Some(salt), Some(hash)) => pin.is_some_and(|x| hash_pin(salt, x.trim()) == *hash),
        _ => true,
    }
}

/// Vérifie le code d'un profil (rien à vérifier s'il n'en a pas).
pub fn verify(root: &Path, id: &str, pin: Option<&str>) -> Result<(), String> {
    let p = list(root)?.into_iter().find(|p| p.id == id).ok_or("Profil introuvable.")?;
    if pin_matches(&p, pin) { Ok(()) } else { Err("Code PIN incorrect.".into()) }
}

/// Définit (`Some`) ou retire (`None`) le code d'un profil ; s'il en a déjà un, l'ancien est exigé.
pub fn set_pin(root: &Path, id: &str, new: Option<&str>, current: Option<&str>) -> Result<(), String> {
    let mut all = list(root)?;
    let p = all.iter_mut().find(|p| p.id == id).ok_or("Profil introuvable.")?;
    if !pin_matches(p, current) {
        return Err("Code PIN incorrect.".into());
    }
    match new {
        Some(pin) => {
            let pin = check_pin_format(pin)?;
            let salt = uuid::Uuid::new_v4().simple().to_string();
            p.pin_hash = Some(hash_pin(&salt, pin));
            p.pin_salt = Some(salt);
        }
        None => {
            p.pin_hash = None;
            p.pin_salt = None;
        }
    }
    save(root, &all)
}

#[derive(Serialize, Deserialize, Default)]
struct File {
    profiles: Vec<Profile>,
}

pub const COLORS: [&str; 8] = ["#c96442", "#3a78e0", "#2fa876", "#b08a3a", "#7c4ddb", "#d4507a", "#2a9d9d", "#6b7280"];
const MAX_NAME: usize = 24;

/// Éléments de l'ancienne organisation (tout à la racine) à ranger dans le premier profil.
const LEGACY: [&str; 7] = ["history.db", "history.db-wal", "history.db-shm", "skills", "skills-disabled.json", "skills-auto-imported.json", "mcp.json"];

pub fn data_dir(root: &Path, id: &str) -> PathBuf {
    root.join("profiles").join(id)
}

fn file(root: &Path) -> PathBuf {
    root.join("profiles.json")
}

fn load(root: &Path) -> Option<Vec<Profile>> {
    let text = std::fs::read_to_string(file(root)).ok()?;
    serde_json::from_str::<File>(&text).ok().map(|f| f.profiles)
}

fn save(root: &Path, profiles: &[Profile]) -> Result<(), String> {
    std::fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&File { profiles: profiles.to_vec() }).map_err(|e| e.to_string())?;
    std::fs::write(file(root), text).map_err(|e| e.to_string())
}

fn clean_name(name: &str) -> Result<String, String> {
    let n = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if n.is_empty() {
        return Err("Donne un nom au profil.".into());
    }
    if n.chars().count() > MAX_NAME {
        return Err(format!("Nom trop long ({MAX_NAME} caractères maximum)."));
    }
    Ok(n)
}

fn check_color(color: &str) -> String {
    COLORS.iter().find(|c| c.eq_ignore_ascii_case(color)).unwrap_or(&COLORS[0]).to_string()
}

/// Liste des profils. Sans profil et sans données d'une ancienne version (installation neuve), la liste est vide :
/// l'interface fait créer le premier profil. Avec des données de l'ancienne organisation, « Principal » les reprend.
pub fn list(root: &Path) -> Result<Vec<Profile>, String> {
    if let Some(p) = load(root).filter(|p| !p.is_empty()) {
        return Ok(p);
    }
    if !LEGACY.iter().any(|item| root.join(item).exists()) {
        return Ok(vec![]);
    }
    let first = Profile { id: uuid::Uuid::new_v4().simple().to_string(), name: "Principal".into(), color: COLORS[0].into(), legacy: true, pin_salt: None, pin_hash: None };
    let dir = data_dir(root, &first.id);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for item in LEGACY {
        let from = root.join(item);
        if from.exists() {
            std::fs::rename(&from, dir.join(item)).map_err(|e| format!("Migration de « {item} » impossible : {e}"))?;
        }
    }
    save(root, std::slice::from_ref(&first))?;
    Ok(vec![first])
}

pub fn find(root: &Path, id: &str) -> Result<Profile, String> {
    list(root)?.into_iter().find(|p| p.id == id).ok_or_else(|| "Profil introuvable.".to_string())
}

pub fn create(root: &Path, name: &str, color: &str) -> Result<Profile, String> {
    let mut all = list(root)?;
    let name = clean_name(name)?;
    if all.iter().any(|p| p.name.to_lowercase() == name.to_lowercase()) {
        return Err(format!("Un profil « {name} » existe déjà."));
    }
    let p = Profile { id: uuid::Uuid::new_v4().simple().to_string(), name, color: check_color(color), legacy: false, pin_salt: None, pin_hash: None };
    std::fs::create_dir_all(data_dir(root, &p.id)).map_err(|e| e.to_string())?;
    all.push(p.clone());
    save(root, &all)?;
    Ok(p)
}

pub fn update(root: &Path, id: &str, name: &str, color: &str, pin: Option<&str>) -> Result<Profile, String> {
    let mut all = list(root)?;
    let name = clean_name(name)?;
    if all.iter().any(|p| p.id != id && p.name.to_lowercase() == name.to_lowercase()) {
        return Err(format!("Un profil « {name} » existe déjà."));
    }
    let p = all.iter_mut().find(|p| p.id == id).ok_or("Profil introuvable.")?;
    if !pin_matches(p, pin) {
        return Err("Code PIN incorrect.".into());
    }
    p.name = name;
    p.color = check_color(color);
    let updated = p.clone();
    save(root, &all)?;
    Ok(updated)
}

/// Supprime un profil et ses données (`discard` les met à la corbeille). Le dernier profil ne peut pas être supprimé.
pub fn remove(root: &Path, id: &str, pin: Option<&str>, discard: impl Fn(&Path) -> Result<(), String>) -> Result<(), String> {
    let mut all = list(root)?;
    if all.len() <= 1 {
        return Err("Il faut garder au moins un profil.".into());
    }
    let Some(target) = all.iter().find(|p| p.id == id) else { return Err("Profil introuvable.".into()) };
    if !pin_matches(target, pin) {
        return Err("Code PIN incorrect.".into());
    }
    let dir = data_dir(root, id);
    if dir.exists() {
        discard(&dir)?;
    }
    all.retain(|p| p.id != id);
    save(root, &all)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("profiles-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn first_run_moves_existing_data_into_the_first_profile() {
        let root = tmp();
        std::fs::write(root.join("history.db"), "db").unwrap();
        std::fs::write(root.join("mcp.json"), "{}").unwrap();
        std::fs::create_dir_all(root.join("skills").join("a")).unwrap();
        std::fs::write(root.join("other.txt"), "reste").unwrap();
        let all = list(&root).unwrap();
        assert_eq!(all.len(), 1);
        assert!(all[0].legacy && all[0].name == "Principal");
        let dir = data_dir(&root, &all[0].id);
        assert!(dir.join("history.db").is_file() && dir.join("mcp.json").is_file() && dir.join("skills").join("a").is_dir());
        assert!(!root.join("history.db").exists() && root.join("other.txt").exists());
        assert_eq!(list(&root).unwrap(), all, "stable aux lancements suivants");
    }

    #[test]
    fn profiles_have_separate_directories_and_unique_valid_names() {
        let root = tmp();
        assert!(list(&root).unwrap().is_empty(), "installation neuve : aucun profil créé d'office");
        let a = create(&root, "  Léa  ", "#3a78e0").unwrap();
        let b = create(&root, "Tom", "n'importe quoi").unwrap();
        assert_eq!(a.name, "Léa");
        assert_eq!(b.color, COLORS[0], "couleur inconnue remplacée");
        assert_ne!(data_dir(&root, &a.id), data_dir(&root, &b.id));
        assert!(data_dir(&root, &a.id).is_dir() && data_dir(&root, &b.id).is_dir());
        assert!(create(&root, "léa", "#3a78e0").is_err_and(|e| e.contains("existe déjà")));
        assert!(create(&root, "LÉA", "").is_err(), "casse non ASCII comprise");
        assert!(create(&root, "tom", "").is_err());
        assert!(create(&root, "   ", "").is_err());
        assert!(create(&root, &"x".repeat(30), "").is_err());
        assert!(!a.legacy && list(&root).unwrap().len() == 2);
    }

    #[test]
    fn rename_keeps_names_unique_and_remove_keeps_at_least_one() {
        let root = tmp();
        let main = create(&root, "Principal", "").unwrap();
        let other = create(&root, "Autre", "").unwrap();
        assert!(update(&root, &other.id, "principal", "", None).is_err());
        let renamed = update(&root, &other.id, "Autre 2", "#7c4ddb", None).unwrap();
        assert_eq!((renamed.name.as_str(), renamed.color.as_str()), ("Autre 2", "#7c4ddb"));
        assert!(update(&root, "inconnu", "X", "", None).is_err());

        std::fs::write(data_dir(&root, &other.id).join("history.db"), "x").unwrap();
        remove(&root, &other.id, None, |p| std::fs::remove_dir_all(p).map_err(|e| e.to_string())).unwrap();
        assert!(!data_dir(&root, &other.id).exists() && find(&root, &other.id).is_err());
        assert!(data_dir(&root, &main.id).exists(), "les données des autres profils sont intactes");
        assert!(remove(&root, &main.id, None, |_| Ok(())).is_err(), "le dernier profil reste");
    }

    #[test]
    fn pin_locks_opening_editing_and_deleting_and_is_never_exposed() {
        let root = tmp();
        let main = create(&root, "Principal", "").unwrap();
        let other = create(&root, "Léa", "").unwrap();
        assert!(verify(&root, &other.id, None).is_ok(), "sans code : libre");

        assert!(set_pin(&root, &other.id, Some("12"), None).is_err(), "trop court");
        assert!(set_pin(&root, &other.id, Some("12ab"), None).is_err(), "chiffres seulement");
        set_pin(&root, &other.id, Some("4821"), None).unwrap();
        assert!(verify(&root, &other.id, None).is_err() && verify(&root, &other.id, Some("0000")).is_err());
        assert!(verify(&root, &other.id, Some(" 4821 ")).is_ok());
        assert!(verify(&root, &main.id, None).is_ok(), "les autres profils restent libres");

        // Modifier ou supprimer un profil verrouillé exige le code.
        assert!(update(&root, &other.id, "Léa 2", "", None).is_err());
        assert!(update(&root, &other.id, "Léa 2", "", Some("4821")).is_ok());
        assert!(set_pin(&root, &other.id, Some("9999"), Some("0000")).is_err(), "ancien code exigé");
        assert!(remove(&root, &other.id, Some("1111"), |_| Ok(())).is_err());

        // Le fichier ne contient ni le code ni de quoi le lire facilement, et l'interface ne voit que « locked ».
        let file = std::fs::read_to_string(super::file(&root)).unwrap();
        assert!(!file.contains("4821") && file.contains("pin_hash"));
        let json = serde_json::to_string(&PublicProfile::from(&list(&root).unwrap()[1])).unwrap();
        assert!(json.contains("\"locked\":true") && !json.contains("pin_hash") && !json.contains("pin_salt"));

        set_pin(&root, &other.id, None, Some("4821")).unwrap();
        assert!(verify(&root, &other.id, None).is_ok(), "code retiré");
    }
}
