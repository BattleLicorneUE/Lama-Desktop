//! Coffre-fort du profil : identifiants et informations rapides (adresse, téléphone…) pour remplir des formulaires.
//!
//! Sécurité :
//! - tout le contenu est chiffré (XChaCha20-Poly1305) avec une clé dérivée du mot de passe maître (Argon2id, 64 Mo) ;
//!   les paramètres de dérivation sont authentifiés avec les données, un nonce neuf sert à chaque enregistrement ;
//! - le mot de passe maître n'est jamais écrit ; sans lui, le coffre est irrécupérable ;
//! - la clé et les secrets restent en mémoire seulement coffre ouvert, effacés au verrouillage (manuel, inactivité de 5 minutes,
//!   changement de profil, fermeture de l'application) ;
//! - le modèle d'IA ne voit jamais un secret : l'agent n'obtient que des titres, et la saisie passe directement par le clavier.

use crate::sync::MutexExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use zeroize::{Zeroize, Zeroizing};

use crate::winio::Piece;

const IDLE: Duration = Duration::from_secs(300);
const FILE: &str = "vault.json";

/// Coût de la dérivation de clé : mémoire en Kio, passes, parallélisme.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
pub struct Kdf {
    pub m: u32,
    pub t: u32,
    pub p: u32,
}

pub const KDF_DEFAULT: Kdf = Kdf { m: 65_536, t: 3, p: 1 };

#[derive(Serialize, Deserialize)]
struct Envelope {
    v: u8,
    kdf: Kdf,
    salt: String,
    nonce: String,
    ct: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Login {
    pub id: String,
    pub title: String,
    pub url: String,
    pub username: String,
    pub password: String,
    pub notes: String,
    pub created: u64,
    pub updated: u64,
}

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Info {
    pub id: String,
    pub label: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Clone, Default)]
struct Data {
    logins: Vec<Login>,
    infos: Vec<Info>,
    /// L'assistant peut-il lister les entrées et demander une saisie (jamais lire un secret) ?
    agent_access: bool,
}

impl Drop for Data {
    fn drop(&mut self) {
        for l in &mut self.logins {
            l.password.zeroize();
            l.notes.zeroize();
            l.username.zeroize();
        }
        for i in &mut self.infos {
            i.value.zeroize();
        }
    }
}

struct Open {
    key: Zeroizing<[u8; 32]>,
    salt: [u8; 16],
    kdf: Kdf,
    data: Data,
    last: Instant,
}

struct Inner {
    path: Option<PathBuf>,
    open: Option<Open>,
    failures: u32,
}

pub struct Vault {
    inner: Mutex<Inner>,
    kdf: Kdf,
}

fn b64(b: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(b)
}

fn unb64(s: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD.decode(s).map_err(|_| "Coffre illisible (données altérées).".to_string())
}

fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).expect("générateur aléatoire du système indisponible");
    b
}

/// Entier uniforme dans 0..n, sans biais (tirage rejeté s'il tombe dans la queue de la plage).
fn below(n: usize) -> usize {
    debug_assert!(n > 0 && n <= u32::MAX as usize);
    let n = n as u32;
    let limit = u32::MAX - (u32::MAX % n);
    loop {
        let x = u32::from_le_bytes(random::<4>());
        if x < limit {
            return (x % n) as usize;
        }
    }
}

fn derive(master: &str, salt: &[u8], kdf: Kdf) -> Result<Zeroizing<[u8; 32]>, String> {
    let params = Params::new(kdf.m, kdf.t, kdf.p, Some(32)).map_err(|e| format!("Paramètres de dérivation invalides : {e}"))?;
    let mut key = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params).hash_password_into(master.as_bytes(), salt, &mut *key).map_err(|e| e.to_string())?;
    Ok(key)
}

/// Données authentifiées avec le contenu : modifier la version ou le coût de dérivation dans le fichier fait échouer le déchiffrement.
fn aad(v: u8, k: Kdf) -> Vec<u8> {
    format!("lama-vault|{v}|{}|{}|{}", k.m, k.t, k.p).into_bytes()
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_string()
}

/// Règle minimale du mot de passe maître.
pub fn check_master(m: &str) -> Result<(), String> {
    if m.chars().count() < 10 {
        return Err("Le mot de passe maître doit faire au moins 10 caractères (une phrase de passe longue est idéale).".into());
    }
    let first = m.chars().next().unwrap();
    if m.chars().all(|c| c == first) {
        return Err("Le mot de passe maître ne peut pas être un seul caractère répété.".into());
    }
    Ok(())
}

// ---------------------------------------------------------------- générateur

#[derive(Deserialize, Clone, Debug)]
pub struct GenOpts {
    pub length: usize,
    pub upper: bool,
    pub lower: bool,
    pub digits: bool,
    pub symbols: bool,
    pub avoid_ambiguous: bool,
}

const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
const SYMBOLS: &str = "!@#$%^&*-_=+?.";
const AMBIGUOUS: &str = "Il1O0o|";

/// Mot de passe aléatoire (générateur cryptographique du système) et son entropie en bits.
/// Chaque famille cochée est présente au moins une fois ; l'ordre est mélangé sans biais.
pub fn generate(o: &GenOpts) -> Result<(String, f64), String> {
    let length = o.length.clamp(8, 128);
    let mut classes: Vec<Vec<char>> = Vec::new();
    for (on, set) in [(o.lower, LOWER), (o.upper, UPPER), (o.digits, DIGITS), (o.symbols, SYMBOLS)] {
        if on {
            let chars: Vec<char> = set.chars().filter(|c| !(o.avoid_ambiguous && AMBIGUOUS.contains(*c))).collect();
            if !chars.is_empty() {
                classes.push(chars);
            }
        }
    }
    if classes.is_empty() {
        return Err("Coche au moins un type de caractères.".into());
    }
    let pool: Vec<char> = classes.iter().flatten().copied().collect();
    let mut out: Vec<char> = classes.iter().map(|c| c[below(c.len())]).collect();
    while out.len() < length {
        out.push(pool[below(pool.len())]);
    }
    for i in (1..out.len()).rev() {
        out.swap(i, below(i + 1));
    }
    let bits = length as f64 * (pool.len() as f64).log2();
    Ok((out.into_iter().collect(), bits))
}

// ---------------------------------------------------------------- vues pour l'interface

#[derive(Serialize)]
pub struct LoginView {
    pub id: String,
    pub title: String,
    pub url: String,
    pub username: String,
    pub updated: u64,
}

#[derive(Serialize)]
pub struct Status {
    pub exists: bool,
    pub unlocked: bool,
    pub agent_access: bool,
    pub logins: usize,
    pub infos: usize,
    /// Secondes avant le verrouillage automatique (0 si verrouillé).
    pub locks_in: u64,
    pub failures: u32,
}

fn host_of(url: &str) -> String {
    let u = url.trim();
    let u = u.split("://").nth(1).unwrap_or(u);
    u.split(['/', '?', '#']).next().unwrap_or("").trim_start_matches("www.").to_lowercase()
}

impl Vault {
    pub fn new(kdf: Kdf) -> Self {
        Self { inner: Mutex::new(Inner { path: None, open: None, failures: 0 }), kdf }
    }

    /// Fichier du profil ouvert ; verrouille toujours le coffre (on ne garde pas la clé d'un autre profil).
    pub fn set_dir(&self, dir: Option<&Path>) {
        let mut g = self.inner.lock_safe();
        g.open = None;
        g.failures = 0;
        g.path = dir.map(|d| d.join(FILE));
    }

    fn path(g: &Inner) -> Result<PathBuf, String> {
        g.path.clone().ok_or_else(|| "Aucun profil ouvert.".to_string())
    }

    pub fn status(&self) -> Status {
        let g = self.inner.lock_safe();
        let exists = g.path.as_ref().is_some_and(|p| p.is_file());
        match &g.open {
            Some(o) => Status {
                exists,
                unlocked: true,
                agent_access: o.data.agent_access,
                logins: o.data.logins.len(),
                infos: o.data.infos.len(),
                locks_in: IDLE.saturating_sub(o.last.elapsed()).as_secs(),
                failures: g.failures,
            },
            None => Status { exists, unlocked: false, agent_access: false, logins: 0, infos: 0, locks_in: 0, failures: g.failures },
        }
    }

    pub fn failures(&self) -> u32 {
        self.inner.lock_safe().failures
    }

    fn write(path: &Path, o: &Open) -> Result<(), String> {
        let plain = Zeroizing::new(serde_json::to_vec(&o.data).map_err(|e| e.to_string())?);
        let nonce: [u8; 24] = random();
        let cipher = XChaCha20Poly1305::new(o.key.as_slice().into());
        let ct = cipher.encrypt(XNonce::from_slice(&nonce), Payload { msg: &plain, aad: &aad(1, o.kdf) }).map_err(|_| "Chiffrement impossible.".to_string())?;
        let env = Envelope { v: 1, kdf: o.kdf, salt: b64(&o.salt), nonce: b64(&nonce), ct: b64(&ct) };
        let text = serde_json::to_string(&env).map_err(|e| e.to_string())?;
        // Écriture atomique ; la version précédente reste en .bak le temps d'un enregistrement.
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, text).map_err(|e| format!("Écriture du coffre impossible : {e}"))?;
        if path.exists() {
            let _ = std::fs::copy(path, path.with_extension("bak"));
        }
        std::fs::rename(&tmp, path).map_err(|e| format!("Écriture du coffre impossible : {e}"))
    }

    fn save(g: &mut Inner) -> Result<(), String> {
        let path = Self::path(g)?;
        let o = g.open.as_ref().ok_or("Coffre verrouillé.")?;
        Self::write(&path, o)
    }

    /// Crée le coffre avec un mot de passe maître. Il s'ouvre aussitôt.
    pub fn create(&self, master: &str) -> Result<(), String> {
        check_master(master)?;
        let mut g = self.inner.lock_safe();
        let path = Self::path(&g)?;
        if path.exists() {
            return Err("Un coffre existe déjà pour ce profil.".into());
        }
        let salt: [u8; 16] = random();
        let key = derive(master, &salt, self.kdf)?;
        g.open = Some(Open { key, salt, kdf: self.kdf, data: Data::default(), last: Instant::now() });
        g.failures = 0;
        if let Err(e) = Self::save(&mut g) {
            g.open = None;
            return Err(e);
        }
        Ok(())
    }

    pub fn unlock(&self, master: &str) -> Result<(), String> {
        let mut g = self.inner.lock_safe();
        let path = Self::path(&g)?;
        let text = std::fs::read_to_string(&path).map_err(|_| "Aucun coffre pour ce profil.".to_string())?;
        let env: Envelope = serde_json::from_str(&text).map_err(|_| "Coffre illisible (fichier altéré).".to_string())?;
        if env.v != 1 {
            return Err("Ce coffre vient d'une version plus récente de Lama.".into());
        }
        // Refuse un coût de dérivation délirant (fichier piégé) avant de s'y fier.
        if env.kdf.m > 1_048_576 || env.kdf.t > 20 || env.kdf.p > 8 || env.kdf.m < 8 {
            return Err("Coffre illisible (paramètres invalides).".into());
        }
        let salt = unb64(&env.salt)?;
        let nonce = unb64(&env.nonce)?;
        let ct = unb64(&env.ct)?;
        if nonce.len() != 24 || salt.len() != 16 {
            return Err("Coffre illisible (données altérées).".into());
        }
        let key = derive(master, &salt, env.kdf)?;
        let cipher = XChaCha20Poly1305::new(key.as_slice().into());
        let plain = match cipher.decrypt(XNonce::from_slice(&nonce), Payload { msg: &ct, aad: &aad(env.v, env.kdf) }) {
            Ok(p) => Zeroizing::new(p),
            Err(_) => {
                g.failures += 1;
                return Err("Mot de passe maître incorrect (ou coffre altéré).".into());
            }
        };
        let data: Data = serde_json::from_slice(&plain).map_err(|_| "Coffre illisible (contenu invalide).".to_string())?;
        let mut s = [0u8; 16];
        s.copy_from_slice(&salt);
        g.open = Some(Open { key, salt: s, kdf: env.kdf, data, last: Instant::now() });
        g.failures = 0;
        Ok(())
    }

    pub fn lock(&self) {
        self.inner.lock_safe().open = None;
    }

    /// Verrouille si le coffre est resté inutilisé trop longtemps. Renvoie vrai s'il vient de se verrouiller.
    pub fn tick(&self) -> bool {
        let mut g = self.inner.lock_safe();
        if g.open.as_ref().is_some_and(|o| o.last.elapsed() >= IDLE) {
            g.open = None;
            return true;
        }
        false
    }

    /// Accès au coffre ouvert (remet le minuteur d'inactivité à zéro).
    fn with<T>(&self, f: impl FnOnce(&mut Data) -> Result<T, String>) -> Result<T, String> {
        let mut g = self.inner.lock_safe();
        let o = g.open.as_mut().ok_or("Le coffre est verrouillé.")?;
        o.last = Instant::now();
        f(&mut o.data)
    }

    /// Modifie puis enregistre ; si l'enregistrement échoue, la modification est annulée en mémoire aussi.
    fn change<T>(&self, f: impl FnOnce(&mut Data) -> Result<T, String>) -> Result<T, String> {
        let mut g = self.inner.lock_safe();
        let before = g.open.as_ref().ok_or("Le coffre est verrouillé.")?.data.clone();
        let out = {
            let o = g.open.as_mut().unwrap();
            o.last = Instant::now();
            f(&mut o.data)
        };
        let out = out?;
        if let Err(e) = Self::save(&mut g) {
            g.open.as_mut().unwrap().data = before;
            return Err(e);
        }
        Ok(out)
    }

    pub fn list(&self) -> Result<(Vec<LoginView>, Vec<Info>), String> {
        self.with(|d| {
            let logins = d.logins.iter().map(|l| LoginView { id: l.id.clone(), title: l.title.clone(), url: l.url.clone(), username: l.username.clone(), updated: l.updated }).collect();
            Ok((logins, d.infos.clone()))
        })
    }

    pub fn get_login(&self, id: &str) -> Result<Login, String> {
        self.with(|d| d.logins.iter().find(|l| l.id == id).cloned().ok_or_else(|| "Entrée introuvable.".to_string()))
    }

    /// Ajoute (id vide) ou modifie une entrée.
    pub fn save_login(&self, mut l: Login) -> Result<Login, String> {
        l.title = l.title.trim().to_string();
        if l.title.is_empty() {
            return Err("Donne un nom à l'entrée (par exemple le site).".into());
        }
        self.change(|d| {
            let t = now();
            if l.id.is_empty() {
                l.id = new_id();
                l.created = t;
                l.updated = t;
                d.logins.push(l.clone());
            } else {
                let cur = d.logins.iter_mut().find(|x| x.id == l.id).ok_or("Entrée introuvable.")?;
                l.created = cur.created;
                l.updated = t;
                *cur = l.clone();
            }
            Ok(l)
        })
    }

    pub fn delete_login(&self, id: &str) -> Result<(), String> {
        self.change(|d| {
            let n = d.logins.len();
            d.logins.retain(|l| l.id != id);
            if d.logins.len() == n { Err("Entrée introuvable.".into()) } else { Ok(()) }
        })
    }

    pub fn save_info(&self, mut i: Info) -> Result<Info, String> {
        i.label = i.label.trim().to_string();
        if i.label.is_empty() {
            return Err("Donne un nom à l'information (Téléphone, Adresse…).".into());
        }
        self.change(|d| {
            if i.id.is_empty() {
                i.id = new_id();
                d.infos.push(i.clone());
            } else {
                let cur = d.infos.iter_mut().find(|x| x.id == i.id).ok_or("Information introuvable.")?;
                *cur = i.clone();
            }
            Ok(i)
        })
    }

    pub fn delete_info(&self, id: &str) -> Result<(), String> {
        self.change(|d| {
            let n = d.infos.len();
            d.infos.retain(|i| i.id != id);
            if d.infos.len() == n { Err("Information introuvable.".into()) } else { Ok(()) }
        })
    }

    pub fn set_agent_access(&self, on: bool) -> Result<(), String> {
        self.change(|d| {
            d.agent_access = on;
            Ok(())
        })
    }

    /// Change le mot de passe maître : nouveau sel, nouvelle clé, tout est rechiffré.
    pub fn change_master(&self, old: &str, new: &str) -> Result<(), String> {
        check_master(new)?;
        let mut g = self.inner.lock_safe();
        let o = g.open.as_ref().ok_or("Le coffre est verrouillé.")?;
        if *derive(old, &o.salt, o.kdf)? != *o.key {
            g.failures += 1;
            return Err("L'ancien mot de passe maître est incorrect.".into());
        }
        let salt: [u8; 16] = random();
        let kdf = self.kdf;
        let key = derive(new, &salt, kdf)?;
        let o = g.open.as_mut().unwrap();
        let backup = (std::mem::replace(&mut o.key, key), o.salt, o.kdf);
        o.salt = salt;
        o.kdf = kdf;
        o.last = Instant::now();
        if let Err(e) = Self::save(&mut g) {
            let o = g.open.as_mut().unwrap();
            o.key = backup.0;
            o.salt = backup.1;
            o.kdf = backup.2;
            return Err(e);
        }
        Ok(())
    }

    /// Supprime définitivement le coffre (mot de passe maître exigé s'il est ouvert ; sinon on vérifie en l'ouvrant).
    pub fn destroy(&self, master: &str) -> Result<(), String> {
        if !self.status().unlocked {
            self.unlock(master)?;
        } else {
            let g = self.inner.lock_safe();
            let o = g.open.as_ref().unwrap();
            if *derive(master, &o.salt, o.kdf)? != *o.key {
                return Err("Mot de passe maître incorrect.".into());
            }
        }
        let mut g = self.inner.lock_safe();
        let path = Self::path(&g)?;
        g.open = None;
        for ext in ["json", "bak", "tmp"] {
            let _ = std::fs::remove_file(path.with_extension(ext));
        }
        Ok(())
    }

    // ------------------------------------------------------------ pour l'assistant et la saisie

    pub fn agent_ready(&self) -> bool {
        self.inner.lock_safe().open.as_ref().is_some_and(|o| o.data.agent_access)
    }

    /// Ce que l'assistant a le droit de voir : titres, sites et identifiants. Jamais un mot de passe ni une note.
    pub fn list_for_agent(&self) -> Result<String, String> {
        self.with(|d| {
            if !d.agent_access {
                return Err("L'assistant n'a pas accès au coffre (Réglages → Coffre).".into());
            }
            let mut lines = Vec::new();
            for l in &d.logins {
                let user = if l.username.is_empty() { String::new() } else { format!(", identifiant « {} »", l.username) };
                lines.push(format!("- identifiant : « {} » ({}{})", l.title, host_of(&l.url), user).replace(" ()", ""));
            }
            for i in &d.infos {
                lines.push(format!("- info rapide : « {} »", i.label));
            }
            Ok(if lines.is_empty() { "Le coffre est vide.".to_string() } else { lines.join("\n") })
        })
    }

    fn find<'a, T>(items: &'a [T], query: &str, id_of: impl Fn(&T) -> &str, names: impl Fn(&T) -> Vec<String>, what: &str) -> Result<&'a T, String> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Err(format!("Précise quel(le) {what}."));
        }
        if let Some(x) = items.iter().find(|x| id_of(x) == query.trim()) {
            return Ok(x);
        }
        if let Some(x) = items.iter().find(|x| names(x).iter().any(|n| n.to_lowercase() == q)) {
            return Ok(x);
        }
        let hits: Vec<&T> = items.iter().filter(|x| names(x).iter().any(|n| n.to_lowercase().contains(&q))).collect();
        match hits.as_slice() {
            [one] => Ok(one),
            [] => Err(format!("Aucun(e) {what} ne correspond à « {query} ».")),
            many => Err(format!("Plusieurs correspondances pour « {query} » : {}. Précise le nom.", many.iter().map(|x| format!("« {} »", names(x)[0])).collect::<Vec<_>>().join(", "))),
        }
    }

    /// Transforme la demande du modèle (noms approximatifs) en demande exacte, par identifiants, avec de quoi l'afficher à
    /// l'utilisateur avant qu'il accepte : c'est ce qui sera réellement saisi, pas ce que le modèle prétend.
    pub fn resolve_fill(&self, args: &Value) -> Result<Value, String> {
        let items = args.get("items").and_then(Value::as_array).ok_or("Argument manquant : items")?;
        if items.is_empty() || items.len() > 6 {
            return Err("Entre 1 et 6 éléments à saisir.".into());
        }
        self.with(|d| {
            if !d.agent_access {
                return Err("L'assistant n'a pas accès au coffre (Réglages → Coffre).".into());
            }
            let mut out = Vec::new();
            for it in items {
                let kind = it.get("type").and_then(Value::as_str).unwrap_or("");
                let entry = it.get("entry").and_then(Value::as_str).unwrap_or("");
                match kind {
                    "username" | "password" | "login" => {
                        let l = Self::find(&d.logins, entry, |l| &l.id, |l| vec![l.title.clone(), host_of(&l.url)], "identifiant")?;
                        if kind != "username" && l.password.is_empty() {
                            return Err(format!("« {} » n'a pas de mot de passe enregistré.", l.title));
                        }
                        out.push(json!({ "kind": kind, "id": l.id, "label": l.title, "site": host_of(&l.url), "username": if kind == "password" { "" } else { l.username.as_str() } }));
                    }
                    "info" => {
                        let i = Self::find(&d.infos, entry, |i| &i.id, |i| vec![i.label.clone()], "information")?;
                        out.push(json!({ "kind": "info", "id": i.id, "label": i.label, "site": "", "username": "" }));
                    }
                    other => return Err(format!("Type inconnu : « {other} » (username, password, login ou info).")),
                }
            }
            let wait = args.get("wait_seconds").and_then(Value::as_f64).unwrap_or(3.0).clamp(0.5, 15.0);
            let submit = args.get("submit").and_then(Value::as_bool).unwrap_or(false);
            let tab = args.get("separator").and_then(Value::as_str) != Some("none");
            Ok(json!({ "items": out, "wait": wait, "submit": submit, "tab": tab }))
        })
    }

    /// Demande de connexion du modèle (nom approximatif) → demande exacte : entrée, site, identifiant, ce qui sera saisi.
    /// L'adresse complète de l'entrée sert au contrôle de la page ; ce n'est pas un secret.
    pub fn resolve_login(&self, args: &Value) -> Result<Value, String> {
        let entry = args.get("entry").and_then(Value::as_str).ok_or("Argument manquant : entry")?;
        let mode = match args.get("mode").and_then(Value::as_str) {
            Some(m @ ("username" | "password")) => m,
            Some("login") | None => "login",
            Some(other) => return Err(format!("Mode inconnu : « {other} » (login, username ou password).")),
        };
        let submit = args.get("submit").and_then(Value::as_bool).unwrap_or(false);
        self.with(|d| {
            if !d.agent_access {
                return Err("L'assistant n'a pas accès au coffre (Réglages → Coffre).".into());
            }
            let l = Self::find(&d.logins, entry, |l| &l.id, |l| vec![l.title.clone(), host_of(&l.url)], "identifiant")?;
            if l.url.trim().is_empty() {
                return Err(format!("« {} » n'a pas d'adresse de site : ajoute-la dans le coffre pour autoriser la connexion automatique.", l.title));
            }
            if mode != "username" && l.password.is_empty() {
                return Err(format!("« {} » n'a pas de mot de passe enregistré.", l.title));
            }
            Ok(json!({ "id": l.id, "label": l.title, "site": host_of(&l.url), "url": l.url, "username": if mode == "password" { "" } else { l.username.as_str() }, "mode": mode, "submit": submit }))
        })
    }

    /// Adresse de l'entrée et les secrets à saisir selon le mode (« login », « username » ou « password »).
    pub fn login_credentials(&self, id: &str, mode: &str) -> Result<(String, Option<Zeroizing<String>>, Option<Zeroizing<String>>), String> {
        self.with(|d| {
            let l = d.logins.iter().find(|l| l.id == id).ok_or("Entrée introuvable.")?;
            let user = (mode != "password").then(|| Zeroizing::new(l.username.clone()));
            let pass = (mode != "username").then(|| Zeroizing::new(l.password.clone()));
            Ok((l.url.clone(), user.filter(|u| !u.is_empty()), pass))
        })
    }

    /// Les touches à taper pour une demande résolue, et son résumé sans secret.
    pub fn pieces_for(&self, resolved: &Value) -> Result<(Vec<Piece>, String), String> {
        let items = resolved.get("items").and_then(Value::as_array).ok_or("Demande invalide.")?;
        let tab = resolved.get("tab").and_then(Value::as_bool).unwrap_or(true);
        let submit = resolved.get("submit").and_then(Value::as_bool).unwrap_or(false);
        self.with(|d| {
            let mut pieces: Vec<Piece> = Vec::new();
            let mut parts: Vec<String> = Vec::new();
            for (n, it) in items.iter().enumerate() {
                let kind = it.get("kind").and_then(Value::as_str).unwrap_or("");
                let id = it.get("id").and_then(Value::as_str).unwrap_or("");
                if n > 0 && tab {
                    pieces.push(Piece::Tab);
                }
                match kind {
                    "username" | "password" | "login" => {
                        let l = d.logins.iter().find(|l| l.id == id).ok_or("Entrée introuvable.")?;
                        if kind != "password" {
                            pieces.push(Piece::Text(Zeroizing::new(l.username.clone())));
                        }
                        if kind == "login" {
                            pieces.push(Piece::Tab);
                        }
                        if kind != "username" {
                            pieces.push(Piece::Text(Zeroizing::new(l.password.clone())));
                        }
                        parts.push(match kind {
                            "username" => format!("identifiant de « {} »", l.title),
                            "password" => format!("mot de passe de « {} »", l.title),
                            _ => format!("identifiant et mot de passe de « {} »", l.title),
                        });
                    }
                    "info" => {
                        let i = d.infos.iter().find(|i| i.id == id).ok_or("Information introuvable.")?;
                        pieces.push(Piece::Text(Zeroizing::new(i.value.clone())));
                        parts.push(format!("« {} »", i.label));
                    }
                    other => return Err(format!("Type inconnu : {other}")),
                }
            }
            if submit {
                pieces.push(Piece::Enter);
            }
            Ok((pieces, parts.join(", ")))
        })
    }

    /// Une valeur à copier : mot de passe, identifiant ou information rapide.
    pub fn secret(&self, id: &str, field: &str) -> Result<Zeroizing<String>, String> {
        self.with(|d| match field {
            "password" => d.logins.iter().find(|l| l.id == id).map(|l| Zeroizing::new(l.password.clone())).ok_or_else(|| "Entrée introuvable.".to_string()),
            "username" => d.logins.iter().find(|l| l.id == id).map(|l| Zeroizing::new(l.username.clone())).ok_or_else(|| "Entrée introuvable.".to_string()),
            "info" => d.infos.iter().find(|i| i.id == id).map(|i| Zeroizing::new(i.value.clone())).ok_or_else(|| "Information introuvable.".to_string()),
            other => Err(format!("Champ inconnu : {other}")),
        })
    }
}

/// Le coffre de l'application (un seul : celui du profil ouvert).
pub fn global() -> &'static Vault {
    static V: OnceLock<Vault> = OnceLock::new();
    V.get_or_init(|| Vault::new(KDF_DEFAULT))
}

/// Connexion dans le navigateur de Lama pour une demande résolue (`id`, `mode`, `submit`) : l'adresse de la page doit être celle de l'entrée.
pub async fn browser_login(resolved: &Value) -> Result<String, String> {
    let id = resolved.get("id").and_then(Value::as_str).ok_or("Demande invalide.")?;
    let mode = resolved.get("mode").and_then(Value::as_str).unwrap_or("login");
    let submit = resolved.get("submit").and_then(Value::as_bool).unwrap_or(false);
    let (url, user, pass) = global().login_credentials(id, mode)?;
    let label = resolved.get("label").and_then(Value::as_str).unwrap_or("l'entrée");
    let done = crate::browser::login(&url, user, pass, submit).await?;
    Ok(format!("« {label} » : {done}"))
}

/// Saisit une demande résolue dans la fenêtre active : Lama se réduit pour rendre la main à l'application précédente,
/// attend que l'utilisateur ait le champ sous le curseur, tape, puis revient. Rien n'est tapé si Lama reste au premier plan.
pub async fn fill_flow<R: tauri::Runtime>(app: &tauri::AppHandle<R>, resolved: &Value) -> Result<String, String> {
    use tauri::Manager;
    let (pieces, summary) = global().pieces_for(resolved)?;
    let wait = resolved.get("wait").and_then(Value::as_f64).unwrap_or(3.0).clamp(0.5, 15.0);
    let win = app.get_webview_window("main");
    if let Some(w) = &win {
        let _ = w.minimize();
    }
    tokio::time::sleep(Duration::from_secs_f64(wait)).await;
    let restore = || {
        if let Some(w) = &win {
            let _ = w.unminimize();
            let _ = w.set_focus();
        }
    };
    if crate::winio::foreground_is_us() {
        restore();
        return Err("Lama est resté au premier plan : rien n'a été saisi. Clique dans le champ à remplir, puis relance.".into());
    }
    let title = crate::winio::foreground_title();
    let typed = tokio::task::spawn_blocking(move || crate::winio::type_pieces(&pieces)).await.map_err(|e| e.to_string())?;
    restore();
    typed?;
    Ok(if title.is_empty() { format!("{summary} : saisi.") } else { format!("{summary} : saisi dans la fenêtre « {title} ».") })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Coût réduit : les tests ne doivent pas attendre 64 Mo de dérivation à chaque ouverture.
    const FAST: Kdf = Kdf { m: 64, t: 1, p: 1 };

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("vault-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn vault(dir: &Path) -> Vault {
        let v = Vault::new(FAST);
        v.set_dir(Some(dir));
        v
    }

    fn login(title: &str, url: &str, user: &str, pass: &str) -> Login {
        Login { title: title.into(), url: url.into(), username: user.into(), password: pass.into(), notes: "note secrète".into(), ..Default::default() }
    }

    #[test]
    fn create_lock_unlock_roundtrip_and_wrong_password() {
        let dir = tmp();
        let v = vault(&dir);
        assert!(!v.status().exists);
        assert!(v.create("court").unwrap_err().contains("10 caractères"));
        assert!(v.create("aaaaaaaaaaaa").is_err());
        v.create("phrase de passe très longue").unwrap();
        assert!(v.status().exists && v.status().unlocked);
        assert!(v.create("autre phrase de passe longue").is_err(), "pas d'écrasement");
        v.save_login(login("GitHub", "https://github.com/login", "samuel", "p@ss-Ünïcode-€-123")).unwrap();
        v.save_info(Info { label: "Téléphone".into(), value: "06 12 34 56 78".into(), ..Default::default() }).unwrap();
        v.lock();
        assert!(!v.status().unlocked && v.list().err().unwrap().contains("verrouillé"));
        assert!(v.unlock("mauvais mot de passe").unwrap_err().contains("incorrect"));
        assert_eq!(v.failures(), 1);
        v.unlock("phrase de passe très longue").unwrap();
        assert_eq!(v.failures(), 0);
        let (logins, infos) = v.list().unwrap();
        assert_eq!((logins.len(), infos.len()), (1, 1));
        let full = v.get_login(&logins[0].id).unwrap();
        assert_eq!((full.password.as_str(), full.notes.as_str()), ("p@ss-Ünïcode-€-123", "note secrète"));
        // Un autre processus qui rouvre le fichier retrouve tout.
        let again = vault(&dir);
        again.unlock("phrase de passe très longue").unwrap();
        assert_eq!(again.list().unwrap().0.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_file_contains_no_readable_secret_and_resists_tampering() {
        let dir = tmp();
        let v = vault(&dir);
        v.create("phrase de passe très longue").unwrap();
        v.save_login(login("Banque", "https://banque.example", "jean.dupont", "Tr0ub4dor&3-secret")).unwrap();
        v.lock();
        let raw = std::fs::read_to_string(dir.join("vault.json")).unwrap();
        for leak in ["Tr0ub4dor", "jean.dupont", "Banque", "banque.example", "note secrète", "phrase de passe"] {
            assert!(!raw.contains(leak), "« {leak} » ne doit pas apparaître en clair");
        }
        // Les mêmes données chiffrées deux fois donnent des fichiers différents (nonce neuf).
        v.unlock("phrase de passe très longue").unwrap();
        v.save_info(Info { label: "x".into(), value: "y".into(), ..Default::default() }).unwrap();
        v.lock();
        assert_ne!(raw, std::fs::read_to_string(dir.join("vault.json")).unwrap());

        let good = std::fs::read_to_string(dir.join("vault.json")).unwrap();
        let mut env: Envelope = serde_json::from_str(&good).unwrap();
        // 1. un octet du contenu modifié
        let mut ct = unb64(&env.ct).unwrap();
        ct[3] ^= 1;
        env.ct = b64(&ct);
        std::fs::write(dir.join("vault.json"), serde_json::to_string(&env).unwrap()).unwrap();
        assert!(v.unlock("phrase de passe très longue").is_err(), "contenu altéré");
        // 2. coût de dérivation falsifié (attaque par rétrogradation)
        let mut env: Envelope = serde_json::from_str(&good).unwrap();
        env.kdf.t = 2;
        std::fs::write(dir.join("vault.json"), serde_json::to_string(&env).unwrap()).unwrap();
        assert!(v.unlock("phrase de passe très longue").is_err(), "paramètres falsifiés");
        // 3. fichier piégé avec un coût énorme : refusé sans calcul
        let mut env: Envelope = serde_json::from_str(&good).unwrap();
        env.kdf.m = 4_000_000_000;
        std::fs::write(dir.join("vault.json"), serde_json::to_string(&env).unwrap()).unwrap();
        assert!(v.unlock("phrase de passe très longue").unwrap_err().contains("paramètres"));
        // 4. fichier d'origine : toujours lisible
        std::fs::write(dir.join("vault.json"), &good).unwrap();
        v.unlock("phrase de passe très longue").unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn master_password_change_and_destruction() {
        let dir = tmp();
        let v = vault(&dir);
        v.create("ancienne phrase de passe").unwrap();
        v.save_login(login("Site", "https://site.example", "moi", "secret-1")).unwrap();
        assert!(v.change_master("pas le bon", "nouvelle phrase de passe").unwrap_err().contains("ancien"));
        assert!(v.change_master("ancienne phrase de passe", "court").is_err());
        let before = std::fs::read_to_string(dir.join("vault.json")).unwrap();
        v.change_master("ancienne phrase de passe", "nouvelle phrase de passe").unwrap();
        assert_ne!(before, std::fs::read_to_string(dir.join("vault.json")).unwrap());
        v.lock();
        assert!(v.unlock("ancienne phrase de passe").is_err());
        v.unlock("nouvelle phrase de passe").unwrap();
        assert_eq!(v.list().unwrap().0.len(), 1);
        assert!(v.destroy("pas le bon").is_err() && v.status().exists);
        v.destroy("nouvelle phrase de passe").unwrap();
        assert!(!v.status().exists && !v.status().unlocked);
        assert!(!dir.join("vault.json").exists() && !dir.join("vault.bak").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn switching_profile_locks_and_idle_locks() {
        let (a, b) = (tmp(), tmp());
        let v = vault(&a);
        v.create("phrase de passe très longue").unwrap();
        v.set_dir(Some(&b));
        assert!(!v.status().unlocked && !v.status().exists, "l'autre profil n'a pas de coffre et rien n'est resté ouvert");
        v.set_dir(Some(&a));
        v.unlock("phrase de passe très longue").unwrap();
        assert!(!v.tick(), "pas verrouillé tant qu'il sert");
        {
            let mut g = v.inner.lock_safe();
            g.open.as_mut().unwrap().last = Instant::now() - IDLE - Duration::from_secs(1);
        }
        assert!(v.tick() && !v.status().unlocked, "verrouillé après l'inactivité");
        for d in [a, b] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn failed_save_rolls_back_memory() {
        let dir = tmp();
        let v = vault(&dir);
        v.create("phrase de passe très longue").unwrap();
        // Le dossier devient un fichier : l'écriture échoue.
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::write(&dir, "x").unwrap();
        assert!(v.save_login(login("A", "", "", "x")).is_err());
        assert_eq!(v.status().logins, 0, "l'entrée n'est pas restée en mémoire");
        let _ = std::fs::remove_file(dir);
    }

    #[test]
    fn the_assistant_sees_titles_never_secrets_and_only_when_allowed() {
        let dir = tmp();
        let v = vault(&dir);
        v.create("phrase de passe très longue").unwrap();
        v.save_login(login("GitHub", "https://www.github.com/login", "samuel", "MOT-DE-PASSE-SECRET")).unwrap();
        v.save_login(login("GitLab", "https://gitlab.com", "samuel", "AUTRE-SECRET")).unwrap();
        v.save_info(Info { label: "Adresse".into(), value: "12 rue des Lilas".into(), ..Default::default() }).unwrap();
        assert!(!v.agent_ready());
        assert!(v.list_for_agent().unwrap_err().contains("pas accès"));
        assert!(v.resolve_fill(&json!({ "items": [{ "type": "password", "entry": "GitHub" }] })).is_err());
        v.set_agent_access(true).unwrap();
        assert!(v.agent_ready());
        let listing = v.list_for_agent().unwrap();
        assert!(listing.contains("GitHub") && listing.contains("github.com") && listing.contains("samuel") && listing.contains("Adresse"), "{listing}");
        for secret in ["MOT-DE-PASSE-SECRET", "AUTRE-SECRET", "12 rue des Lilas", "note secrète"] {
            assert!(!listing.contains(secret), "{secret} ne doit jamais être montré au modèle");
        }
        // Résolution : nom approximatif → identifiant exact, ambiguïté refusée, rien de secret dans la demande résolue.
        let r = v.resolve_fill(&json!({ "items": [{ "type": "login", "entry": "github" }, { "type": "info", "entry": "adresse" }], "submit": true })).unwrap();
        let text = r.to_string();
        assert!(text.contains("GitHub") && text.contains("github.com") && !text.contains("SECRET") && !text.contains("Lilas"), "{text}");
        assert_eq!(r["submit"], true);
        assert!(v.resolve_fill(&json!({ "items": [{ "type": "login", "entry": "git" }] })).unwrap_err().contains("Plusieurs"));
        assert!(v.resolve_fill(&json!({ "items": [{ "type": "login", "entry": "inconnu" }] })).is_err());
        assert!(v.resolve_fill(&json!({ "items": [{ "type": "voler", "entry": "GitHub" }] })).is_err());
        assert!(v.resolve_fill(&json!({ "items": [] })).is_err());
        // Les touches : identifiant, Tab, mot de passe, Tab, adresse, Entrée.
        let (pieces, summary) = v.pieces_for(&r).unwrap();
        let shape: Vec<String> = pieces.iter().map(|p| match p { Piece::Text(t) => format!("T:{}", t.as_str()), Piece::Tab => "TAB".into(), Piece::Enter => "ENTER".into() }).collect();
        assert_eq!(shape, ["T:samuel", "TAB", "T:MOT-DE-PASSE-SECRET", "TAB", "T:12 rue des Lilas", "ENTER"]);
        assert!(!summary.contains("SECRET") && summary.contains("GitHub") && summary.contains("Adresse"), "{summary}");
        // Verrouillé : plus rien.
        v.lock();
        assert!(v.pieces_for(&r).is_err() && v.list_for_agent().is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn browser_login_requests_are_resolved_and_refuse_entries_without_a_site() {
        let dir = tmp();
        let v = vault(&dir);
        v.create("phrase de passe très longue").unwrap();
        v.save_login(login("GitHub", "https://github.com/login", "samuel", "SECRET-GH")).unwrap();
        v.save_login(login("Sans site", "", "moi", "SECRET-X")).unwrap();
        v.save_login(login("Sans mot de passe", "https://exemple.org", "moi", "")).unwrap();
        let ask = |entry: &str, mode: &str| json!({ "entry": entry, "mode": mode, "submit": true });
        assert!(v.resolve_login(&ask("GitHub", "login")).unwrap_err().contains("pas accès"), "refusé tant que l'assistant n'est pas autorisé");
        v.set_agent_access(true).unwrap();
        let r = v.resolve_login(&ask("github", "login")).unwrap();
        assert_eq!((r["label"].as_str(), r["site"].as_str(), r["mode"].as_str(), r["submit"].as_bool()), (Some("GitHub"), Some("github.com"), Some("login"), Some(true)));
        assert!(!r.to_string().contains("SECRET"), "aucun secret dans la demande résolue");
        assert_eq!(v.resolve_login(&ask("github", "password")).unwrap()["username"], "", "mot de passe seul : pas d'identifiant");
        assert!(v.resolve_login(&ask("Sans site", "login")).unwrap_err().contains("pas d'adresse de site"));
        assert!(v.resolve_login(&ask("Sans mot de passe", "login")).unwrap_err().contains("pas de mot de passe"));
        assert!(v.resolve_login(&ask("Sans mot de passe", "username")).is_ok());
        assert!(v.resolve_login(&ask("GitHub", "voler")).is_err());
        let (url, user, pass) = v.login_credentials(r["id"].as_str().unwrap(), "login").unwrap();
        assert_eq!((url.as_str(), user.as_deref().map(String::as_str), pass.as_deref().map(String::as_str)), ("https://github.com/login", Some("samuel"), Some("SECRET-GH")));
        let (_, user, pass) = v.login_credentials(r["id"].as_str().unwrap(), "username").unwrap();
        assert!(user.is_some() && pass.is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn generator_respects_options_and_is_not_biased() {
        let o = GenOpts { length: 20, upper: true, lower: true, digits: true, symbols: true, avoid_ambiguous: true };
        let mut counts = std::collections::HashMap::<char, usize>::new();
        for _ in 0..2000 {
            let (p, bits) = generate(&o).unwrap();
            assert_eq!(p.chars().count(), 20);
            assert!(p.chars().any(|c| c.is_ascii_lowercase()) && p.chars().any(|c| c.is_ascii_uppercase()) && p.chars().any(|c| c.is_ascii_digit()) && p.chars().any(|c| SYMBOLS.contains(c)), "{p}");
            assert!(!p.chars().any(|c| AMBIGUOUS.contains(c)), "{p}");
            assert!(bits > 100.0 && bits < 140.0, "{bits}");
            for c in p.chars() {
                *counts.entry(c).or_default() += 1;
            }
        }
        // 40 000 tirages sur ~66 symboles : ~600 chacun ; un biais grossier (modulo naïf, mélange raté) se verrait.
        let (min, max) = (counts.values().min().unwrap(), counts.values().max().unwrap());
        assert!(*min > 350 && *max < 900, "répartition suspecte : {min}..{max}");
        let digits_only = generate(&GenOpts { length: 12, upper: false, lower: false, digits: true, symbols: false, avoid_ambiguous: false }).unwrap().0;
        assert!(digits_only.chars().all(|c| c.is_ascii_digit()));
        assert!(generate(&GenOpts { length: 12, upper: false, lower: false, digits: false, symbols: false, avoid_ambiguous: false }).is_err());
        assert_eq!(generate(&GenOpts { length: 3, ..o.clone() }).unwrap().0.chars().count(), 8, "longueur minimale");
        assert_eq!(generate(&GenOpts { length: 9999, ..o }).unwrap().0.chars().count(), 128);
        assert_ne!(generate(&GenOpts { length: 24, upper: true, lower: true, digits: true, symbols: true, avoid_ambiguous: false }).unwrap().0, generate(&GenOpts { length: 24, upper: true, lower: true, digits: true, symbols: true, avoid_ambiguous: false }).unwrap().0);
    }

    #[test]
    fn real_cost_parameters_work() {
        // Les vrais paramètres (64 Mo, 3 passes) ouvrent un coffre en un temps raisonnable.
        let dir = tmp();
        let v = Vault::new(KDF_DEFAULT);
        v.set_dir(Some(&dir));
        let t = Instant::now();
        v.create("phrase de passe très longue").unwrap();
        v.lock();
        v.unlock("phrase de passe très longue").unwrap();
        assert!(t.elapsed() < Duration::from_secs(20), "{:?}", t.elapsed());
        let _ = std::fs::remove_dir_all(dir);
    }
}
