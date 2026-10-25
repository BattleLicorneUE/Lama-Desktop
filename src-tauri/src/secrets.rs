//! Secrets (clé API) dans le coffre de l'utilisateur (Gestionnaire d'identifiants Windows),
//! plutôt qu'en clair dans les données de l'application. Un secret par profil.

const SERVICE: &str = "ollama-desktop";

fn entry(profile: &str, name: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, &format!("{profile}:{name}")).map_err(|e| e.to_string())
}

pub fn get(profile: &str, name: &str) -> Result<Option<String>, String> {
    match entry(profile, name)?.get_password() {
        Ok(v) => Ok(Some(v)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Enregistre le secret ; une valeur vide le supprime.
pub fn set(profile: &str, name: &str, value: &str) -> Result<(), String> {
    let e = entry(profile, name)?;
    if value.is_empty() {
        return match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(err.to_string()),
        };
    }
    e.set_password(value).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Touche au vrai coffre Windows : `cargo test secrets -- --ignored`.
    #[test]
    #[ignore]
    fn roundtrip_in_the_real_credential_store() {
        let profile = format!("test-{}", uuid::Uuid::new_v4().simple());
        assert_eq!(get(&profile, "api-key").unwrap(), None);
        set(&profile, "api-key", "secret-é€").unwrap();
        assert_eq!(get(&profile, "api-key").unwrap().as_deref(), Some("secret-é€"));
        set(&profile, "api-key", "").unwrap();
        assert_eq!(get(&profile, "api-key").unwrap(), None);
        set(&profile, "api-key", "").unwrap();
    }
}
