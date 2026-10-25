//! IA locale, indépendante d'Ollama : installe le moteur `llama-server` (llama.cpp) et des modèles GGUF choisis dans le
//! catalogue, puis lance le moteur à la demande. Rien n'est téléchargé hors des adresses du catalogue et du moteur épinglé :
//! l'utilisateur ne fournit jamais d'URL, et chaque fichier est vérifié par sa somme SHA-256.

use crate::hardware::Hardware;
use crate::localmodels::{self, Model};
use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const NO_WINDOW: u32 = 0x0800_0000;
/// Version du moteur : épinglée pour que ce qui a été testé soit ce qui est installé.
const ENGINE_BUILD: &str = "b11878";
/// Le moteur est arrêté (et la mémoire rendue) après ce temps sans demande.
const IDLE: Duration = Duration::from_secs(15 * 60);
const SPARE_DISK_MB: u64 = 1024;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Variant {
    Vulkan,
    Cpu,
}

impl Variant {
    fn dir(self) -> &'static str {
        match self {
            Variant::Vulkan => "vulkan",
            Variant::Cpu => "cpu",
        }
    }
    /// Nom du zip, taille et somme de contrôle publiés avec la version `ENGINE_BUILD`.
    fn asset(self) -> (String, &'static str) {
        match self {
            Variant::Vulkan => (format!("llama-{ENGINE_BUILD}-bin-win-vulkan-x64.zip"), "c3e02c08ffda6ae812bc2852829fd5c3040fe1d50ed83176074ba39a10a2600e"),
            Variant::Cpu => (format!("llama-{ENGINE_BUILD}-bin-win-cpu-x64.zip"), "bda163c6bd226a5eba8228abe88ab092aa6cd77743e161fc317953f355c5a8a2"),
        }
    }
}

/// Vulkan marche sur NVIDIA, AMD et Intel Arc sans dépendre de la version du pilote ; sans carte dédiée, le processeur.
pub fn variant_for(hw: &Hardware) -> Variant {
    if hw.best_gpu().is_some() { Variant::Vulkan } else { Variant::Cpu }
}

#[derive(Clone, Serialize, Debug)]
pub struct Progress {
    pub id: String,
    /// « moteur », « modèle », « vérification » ou « décompression ».
    pub phase: String,
    pub file: String,
    pub done: u64,
    pub total: u64,
}

fn root(data: &Path) -> PathBuf {
    data.join("local-ai")
}
fn models_dir(data: &Path) -> PathBuf {
    root(data).join("models")
}
fn engine_dir(data: &Path, v: Variant) -> PathBuf {
    root(data).join("engine").join(format!("{ENGINE_BUILD}-{}", v.dir()))
}

fn find_server(dir: &Path) -> Option<PathBuf> {
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if p.is_dir() {
            if let Some(f) = find_server(&p) {
                return Some(f);
            }
        } else if p.file_name().is_some_and(|n| n.eq_ignore_ascii_case("llama-server.exe")) {
            return Some(p);
        }
    }
    None
}

/// Une installation à la fois ; l'annulation passe par un drapeau.
static CANCEL: Mutex<Option<(String, Arc<AtomicBool>)>> = Mutex::new(None);

pub fn cancel(id: &str) {
    if let Some((cur, flag)) = CANCEL.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        if cur == id {
            flag.store(true, Ordering::SeqCst);
        }
    }
}

pub fn installing() -> Option<String> {
    CANCEL.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|(id, _)| id.clone())
}

struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        *CANCEL.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

fn sha256_of(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Télécharge `url` vers `dest` en reprenant un `.part` interrompu, puis vérifie la somme. Ne laisse `dest` que si tout est bon.
async fn download(url: &str, dest: &Path, sha256: &str, cancel: &AtomicBool, mut progress: impl FnMut(&str, u64, u64)) -> Result<(), String> {
    if dest.exists() {
        return Ok(());
    }
    if let Some(dir) = dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let part = dest.with_extension("part");
    let client = reqwest::Client::builder().connect_timeout(Duration::from_secs(20)).build().map_err(|e| e.to_string())?;
    let mut have = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    let mut req = client.get(url);
    if have > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
    }
    let resp = req.send().await.map_err(|e| format!("Téléchargement impossible : {e}"))?;
    let status = resp.status();
    if status.as_u16() == 416 {
        // Le .part est inutilisable (plus grand que le fichier) : on le jette, la prochaine tentative repart de zéro.
        let _ = std::fs::remove_file(&part);
        return Err("Reprise impossible, relance l'installation.".into());
    }
    if !status.is_success() {
        return Err(format!("Le serveur a répondu {status}."));
    }
    let resumed = status.as_u16() == 206;
    if !resumed {
        have = 0;
    }
    let total = resp.content_length().map(|l| l + have).unwrap_or(0);
    use std::io::Write;
    let mut file = if resumed { std::fs::OpenOptions::new().append(true).open(&part) } else { std::fs::File::create(&part) }.map_err(|e| e.to_string())?;
    let mut stream = resp.bytes_stream();
    let mut done = have;
    let mut last = Instant::now();
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err("Installation annulée.".into());
        }
        let next = tokio::time::timeout(Duration::from_secs(60), stream.next()).await.map_err(|_| "Le téléchargement s'est arrêté (plus de données depuis 60 s).".to_string())?;
        let Some(bytes) = next else { break };
        let bytes = bytes.map_err(|e| format!("Téléchargement interrompu : {e}"))?;
        file.write_all(&bytes).map_err(|e| format!("Écriture impossible (disque plein ?) : {e}"))?;
        done += bytes.len() as u64;
        if last.elapsed() >= Duration::from_millis(250) {
            progress("download", done, total);
            last = Instant::now();
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    progress("download", done, total.max(done));
    let part2 = part.clone();
    let want = sha256.to_string();
    progress("verify", done, done);
    let result = tokio::task::spawn_blocking(move || sha256_of(&part2)).await.map_err(|e| e.to_string())??;
    if !result.eq_ignore_ascii_case(&want) {
        let _ = std::fs::remove_file(&part);
        return Err("Le fichier téléchargé est corrompu (la somme de contrôle ne correspond pas) : il a été supprimé, réessaie.".into());
    }
    std::fs::rename(&part, dest).map_err(|e| e.to_string())
}

fn unzip(zip_path: &Path, to: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        // Refuse les chemins qui sortiraient du dossier (« ../ », chemins absolus).
        let Some(rel) = entry.enclosed_name() else { continue };
        let out = to.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        } else {
            if let Some(p) = out.parent() {
                std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            let mut f = std::fs::File::create(&out).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut f).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

async fn ensure_engine(data: &Path, variant: Variant, id: &str, cancel: &AtomicBool, emit: &(dyn Fn(Progress) + Send + Sync)) -> Result<PathBuf, String> {
    let dir = engine_dir(data, variant);
    if let Some(exe) = find_server(&dir) {
        return Ok(exe);
    }
    let (name, sha) = variant.asset();
    let zip_path = root(data).join("engine").join(&name);
    let url = format!("https://github.com/ggml-org/llama.cpp/releases/download/{ENGINE_BUILD}/{name}");
    let pid = id.to_string();
    download(&url, &zip_path, sha, cancel, |_, done, total| emit(Progress { id: pid.clone(), phase: "moteur".into(), file: name.clone(), done, total })).await?;
    emit(Progress { id: id.into(), phase: "décompression".into(), file: name.clone(), done: 0, total: 0 });
    let (z, d) = (zip_path.clone(), dir.clone());
    let unpacked = tokio::task::spawn_blocking(move || {
        let _ = std::fs::remove_dir_all(&d);
        unzip(&z, &d)
    })
    .await
    .map_err(|e| e.to_string())?;
    if let Err(e) = unpacked {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(e);
    }
    let _ = std::fs::remove_file(&zip_path);
    find_server(&dir).ok_or_else(|| "Le moteur téléchargé ne contient pas llama-server.exe.".to_string())
}

/// Installe un modèle du catalogue (et le moteur s'il manque). `emit` reçoit la progression.
pub async fn install(data: &Path, model: &Model, hw: &Hardware, emit: Arc<dyn Fn(Progress) + Send + Sync>) -> Result<(), String> {
    let flag = Arc::new(AtomicBool::new(false));
    {
        let mut cur = CANCEL.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((other, _)) = cur.as_ref() {
            return Err(format!("Une installation est déjà en cours ({other})."));
        }
        *cur = Some((model.id.clone(), flag.clone()));
    }
    let _guard = Guard;
    let need_mb = model.files.iter().map(|f| f.size).sum::<u64>() / 1_048_576 + SPARE_DISK_MB;
    std::fs::create_dir_all(root(data)).map_err(|e| e.to_string())?;
    let free = crate::hardware::free_mb(&root(data));
    if free < need_mb {
        return Err(format!("Pas assez de place sur le disque : il faut environ {} Mo, il en reste {free}.", need_mb));
    }
    ensure_engine(data, variant_for(hw), &model.id, &flag, &*emit).await?;
    let dir = models_dir(data).join(&model.id);
    let (repo, id) = (model.repo.clone(), model.id.clone());
    for f in &model.files {
        let url = format!("https://huggingface.co/{repo}/resolve/main/{}", f.name);
        let (pid, name) = (id.clone(), f.name.clone());
        let emit2 = emit.clone();
        download(&url, &dir.join(&f.name), &f.sha256, &flag, move |stage, done, total| {
            emit2(Progress { id: pid.clone(), phase: if stage == "verify" { "vérification".into() } else { "modèle".into() }, file: name.clone(), done, total: if total == 0 { f.size } else { total } })
        })
        .await?;
    }
    std::fs::write(dir.join("manifest.json"), serde_json::to_vec_pretty(&serde_json::json!({ "id": model.id, "files": model.files.iter().map(|f| &f.name).collect::<Vec<_>>() })).unwrap_or_default()).map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Serialize, Clone, Debug)]
pub struct CtxOption {
    pub ctx: u32,
    /// Mémoire supplémentaire du cache de contexte, en Mo.
    pub mem_mb: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct Installed {
    pub id: String,
    pub name: String,
    pub size_bytes: u64,
    pub tools: bool,
    pub vision: bool,
    pub license: String,
    /// Contexte utilisé (choix de l'utilisateur, ou automatique).
    pub ctx: u32,
    /// Choix de l'utilisateur : 0 = automatique.
    pub ctx_setting: u32,
    pub auto_ctx: u32,
    pub max_ctx: u32,
    pub options: Vec<CtxOption>,
}

fn config_path(data: &Path) -> PathBuf {
    root(data).join("config.json")
}

/// Contexte choisi pour ce modèle (0 = automatique).
pub fn ctx_setting(data: &Path, id: &str) -> u32 {
    std::fs::read_to_string(config_path(data)).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).and_then(|v| v["ctx"][id].as_u64()).unwrap_or(0) as u32
}

pub fn set_ctx_setting(data: &Path, id: &str, ctx: u32) -> Result<(), String> {
    let model = localmodels::catalog().into_iter().find(|m| m.id == id).ok_or("Modèle inconnu.")?;
    if ctx != 0 && !localmodels::ctx_options(&model).contains(&ctx) {
        return Err("Taille de contexte non prise en charge par ce modèle.".into());
    }
    let mut v: serde_json::Value = std::fs::read_to_string(config_path(data)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| serde_json::json!({}));
    v["ctx"][id] = serde_json::json!(ctx);
    std::fs::create_dir_all(root(data)).map_err(|e| e.to_string())?;
    std::fs::write(config_path(data), serde_json::to_vec_pretty(&v).unwrap_or_default()).map_err(|e| e.to_string())
}

/// Le contexte à utiliser : le choix de l'utilisateur s'il est valable, sinon le calcul automatique pour cette machine.
pub fn effective_ctx(data: &Path, model: &Model, hw: &Hardware) -> u32 {
    let chosen = ctx_setting(data, &model.id);
    if chosen != 0 && localmodels::ctx_options(model).contains(&chosen) { chosen } else { localmodels::auto_ctx(model, hw) }
}

pub fn installed(data: &Path, hw: &Hardware) -> Vec<Installed> {
    let catalog = localmodels::catalog();
    let mut out = Vec::new();
    for m in catalog {
        let dir = models_dir(data).join(&m.id);
        if dir.join("manifest.json").exists() && m.files.iter().all(|f| dir.join(&f.name).exists()) {
            let options = localmodels::ctx_options(&m).into_iter().map(|c| CtxOption { ctx: c, mem_mb: localmodels::kv_mb(&m, c) }).collect();
            out.push(Installed {
                id: m.id.clone(),
                name: m.name.clone(),
                size_bytes: m.files.iter().map(|f| f.size).sum(),
                tools: m.tools,
                vision: m.vision,
                license: m.license.clone(),
                ctx: effective_ctx(data, &m, hw),
                ctx_setting: ctx_setting(data, &m.id),
                auto_ctx: localmodels::auto_ctx(&m, hw),
                max_ctx: m.max_ctx,
                options,
            });
        }
    }
    out
}

/// Espace pris par le moteur, quand il est installé.
pub fn engine_bytes(data: &Path) -> u64 {
    fn size(p: &Path) -> u64 {
        std::fs::read_dir(p).map(|d| d.flatten().map(|e| if e.path().is_dir() { size(&e.path()) } else { e.metadata().map(|m| m.len()).unwrap_or(0) }).sum()).unwrap_or(0)
    }
    size(&root(data).join("engine"))
}

pub async fn remove(data: &Path, id: &str) -> Result<(), String> {
    if id.is_empty() || id.contains(['/', '\\', '.']) {
        return Err("Identifiant de modèle invalide.".into());
    }
    if running_model().await.as_deref() == Some(id) {
        shutdown().await;
    }
    let dir = models_dir(data).join(id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("Suppression impossible : {e}"))?;
    }
    Ok(())
}

struct Running {
    child: tokio::process::Child,
    port: u16,
    key: String,
    model_id: String,
    ctx: u32,
    last_used: Instant,
}

static SERVER: tokio::sync::Mutex<Option<Running>> = tokio::sync::Mutex::const_new(None);
static WATCHER: AtomicBool = AtomicBool::new(false);

pub async fn running_model() -> Option<String> {
    SERVER.lock().await.as_ref().map(|r| r.model_id.clone())
}

fn random_key() -> String {
    let mut b = [0u8; 16];
    let _ = getrandom::getrandom(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn free_port() -> Result<u16, String> {
    let l = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    l.local_addr().map(|a| a.port()).map_err(|e| e.to_string())
}

fn tail(path: &Path) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(12)..].join("\n")
}

/// Le moteur prêt pour ce modèle : le démarre (ou change de modèle) si besoin. Renvoie le port et la clé locale.
pub async fn ensure_running(data: &Path, model_id: &str, ctx: u32) -> Result<(u16, String), String> {
    let mut guard = SERVER.lock().await;
    if let Some(r) = guard.as_mut() {
        if r.model_id == model_id && r.ctx == ctx && matches!(r.child.try_wait(), Ok(None)) {
            r.last_used = Instant::now();
            return Ok((r.port, r.key.clone()));
        }
        let _ = r.child.kill().await;
        *guard = None;
    }
    let model = localmodels::catalog().into_iter().find(|m| m.id == model_id).ok_or_else(|| format!("Modèle local inconnu : {model_id}"))?;
    let dir = models_dir(data).join(&model.id);
    let weights = model.files.iter().find(|f| f.role == "model").map(|f| dir.join(&f.name)).filter(|p| p.exists()).ok_or_else(|| format!("{} n'est pas installé.", model.name))?;
    let exe = find_server(&engine_dir(data, Variant::Vulkan)).or_else(|| find_server(&engine_dir(data, Variant::Cpu))).ok_or("Le moteur local n'est pas installé.")?;

    let (port, key) = (free_port()?, random_key());
    let log = root(data).join("server.log");
    let out = std::fs::File::create(&log).map_err(|e| e.to_string())?;
    let err = out.try_clone().map_err(|e| e.to_string())?;
    let mut cmd = tokio::process::Command::new(&exe);
    cmd.current_dir(exe.parent().unwrap_or(Path::new(".")))
        .args(["-m"])
        .arg(&weights)
        .args(["--host", "127.0.0.1", "--port", &port.to_string(), "--api-key", &key, "-c", &ctx.to_string(), "-np", "1", "--jinja", "--no-webui"]);
    if ctx > localmodels::KV_QUANT_ABOVE {
        // Cache de contexte en 8 bits (moitié moins de mémoire) : demande l'attention « flash » pour les valeurs.
        cmd.args(["-fa", "on", "-ctk", "q8_0", "-ctv", "q8_0"]);
    }
    if let Some(p) = model.files.iter().find(|f| f.role == "projector") {
        cmd.arg("--mmproj").arg(dir.join(&p.name));
    }
    let mut child = cmd.stdin(std::process::Stdio::null()).stdout(out).stderr(err).creation_flags(NO_WINDOW).kill_on_drop(true).spawn().map_err(|e| format!("Impossible de lancer le moteur : {e}"))?;
    if let Some(pid) = child.id() {
        let _ = std::fs::write(root(data).join("server.pid"), pid.to_string());
    }

    // Charger un gros modèle prend du temps : on attend que le moteur réponde, jusqu'à 3 minutes.
    let http = reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(3)).build().map_err(|e| e.to_string())?;
    let started = Instant::now();
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!("Le moteur s'est arrêté ({status}).\n{}", tail(&log)));
        }
        if let Ok(r) = http.get(format!("http://127.0.0.1:{port}/health")).header("Authorization", format!("Bearer {key}")).send().await {
            if r.status().is_success() {
                break;
            }
        }
        if started.elapsed() > Duration::from_secs(180) {
            let _ = child.kill().await;
            return Err(format!("Le moteur n'a pas démarré en 3 minutes.\n{}", tail(&log)));
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
    *guard = Some(Running { child, port, key: key.clone(), model_id: model_id.to_string(), ctx, last_used: Instant::now() });
    drop(guard);
    spawn_idle_watcher();
    Ok((port, key))
}

/// Rend la mémoire quand plus personne n'utilise le modèle depuis un moment.
fn spawn_idle_watcher() {
    if WATCHER.swap(true, Ordering::SeqCst) {
        return;
    }
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let mut g = SERVER.lock().await;
            match g.as_mut() {
                None => {
                    WATCHER.store(false, Ordering::SeqCst);
                    return;
                }
                Some(r) if r.last_used.elapsed() > IDLE => {
                    let _ = r.child.kill().await;
                    *g = None;
                    WATCHER.store(false, Ordering::SeqCst);
                    return;
                }
                _ => {}
            }
        }
    });
}

/// Marque le moteur comme utilisé (à appeler à chaque message pour que l'arrêt automatique attende la fin de la conversation).
pub async fn touch() {
    if let Some(r) = SERVER.lock().await.as_mut() {
        r.last_used = Instant::now();
    }
}

pub async fn shutdown() {
    if let Some(mut r) = SERVER.lock().await.take() {
        let _ = r.child.kill().await;
    }
}

/// À la fermeture de l'application : tue le moteur sans attendre (le runtime peut déjà s'arrêter).
pub fn kill_now(data: &Path) {
    kill_pid_file(data);
}

/// Un moteur resté en vie après un plantage de l'application : on le retrouve par son identifiant noté au lancement.
pub fn kill_pid_file(data: &Path) {
    let file = root(data).join("server.pid");
    let Ok(text) = std::fs::read_to_string(&file) else { return };
    let _ = std::fs::remove_file(&file);
    let Ok(pid) = text.trim().parse::<u32>() else { return };
    // On ne tue que si ce numéro est bien celui d'un llama-server (le numéro a pu être réattribué).
    let list = std::process::Command::new("tasklist").args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"]).creation_flags(NO_WINDOW).output();
    if list.map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains("llama-server")).unwrap_or(false) {
        let _ = std::process::Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).creation_flags(NO_WINDOW).output();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_asset_names_follow_the_pinned_build() {
        for v in [Variant::Vulkan, Variant::Cpu] {
            let (name, sha) = v.asset();
            assert!(name.contains(ENGINE_BUILD) && name.ends_with("x64.zip"), "{name}");
            assert_eq!(sha.len(), 64);
        }
    }

    #[test]
    fn unzip_refuses_paths_that_escape_the_folder() {
        use std::io::Write;
        let tmp = std::env::temp_dir().join(format!("lama-unzip-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        let zip_path = tmp.join("a.zip");
        {
            let mut w = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
            let o = zip::write::SimpleFileOptions::default();
            w.start_file("bin/llama-server.exe", o).unwrap();
            w.write_all(b"ok").unwrap();
            w.start_file("../evil.txt", o).unwrap();
            w.write_all(b"no").unwrap();
            w.finish().unwrap();
        }
        let out = tmp.join("out");
        unzip(&zip_path, &out).unwrap();
        assert!(find_server(&out).is_some());
        assert!(!tmp.join("evil.txt").exists(), "le chemin « ../ » est ignoré");
        let _ = std::fs::remove_dir_all(tmp);
    }

    #[tokio::test]
    async fn download_resumes_verifies_and_rejects_corruption() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let payload: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let good: String = Sha256::digest(&payload).iter().map(|b| format!("{b:02x}")).collect();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/f.bin", listener.local_addr().unwrap());
        let data = payload.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = listener.accept().await else { return };
                let data = data.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = s.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).to_lowercase();
                    let from = req.split("range: bytes=").nth(1).and_then(|r| r.split('-').next()).and_then(|v| v.trim().parse::<usize>().ok());
                    let (head, body) = match from {
                        Some(f) => (format!("HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", data.len() - f), &data[f..]),
                        None => (format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", data.len()), &data[..]),
                    };
                    let _ = s.write_all(head.as_bytes()).await;
                    let _ = s.write_all(body).await;
                });
            }
        });
        let tmp = std::env::temp_dir().join(format!("lama-dl-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&tmp).unwrap();
        let cancel = AtomicBool::new(false);

        // Reprise : un .part de 50 000 octets existe déjà.
        let dest = tmp.join("ok.bin");
        std::fs::write(dest.with_extension("part"), &payload[..50_000]).unwrap();
        download(&url, &dest, &good, &cancel, |_, _, _| {}).await.unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), payload);
        assert!(!dest.with_extension("part").exists());

        // Mauvaise somme : le fichier est refusé et supprimé.
        let bad = tmp.join("bad.bin");
        let err = download(&url, &bad, &"0".repeat(64), &cancel, |_, _, _| {}).await.unwrap_err();
        assert!(err.contains("corrompu"), "{err}");
        assert!(!bad.exists() && !bad.with_extension("part").exists());

        // Annulation : le .part est gardé pour reprendre.
        let stop = AtomicBool::new(true);
        let c = tmp.join("c.bin");
        assert!(download(&url, &c, &good, &stop, |_, _, _| {}).await.unwrap_err().contains("annulée"));
        let _ = std::fs::remove_dir_all(tmp);
    }

    #[tokio::test]
    async fn remove_rejects_path_tricks() {
        let tmp = std::env::temp_dir();
        assert!(remove(&tmp, "../x").await.is_err());
        assert!(remove(&tmp, "").await.is_err());
    }

    /// Vraie installation + vrai moteur (télécharge ~1,1 Go la première fois, repris ensuite) :
    /// `cargo test --lib installs_and_chats -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore]
    async fn installs_and_chats_with_a_real_model() {
        use crate::ollama::{ChatMessage, OllamaClient};
        let data = std::env::temp_dir().join("lama-local-real");
        let hw = crate::hardware::detect();
        let model = localmodels::catalog().into_iter().find(|m| m.id == "qwen3-1.7b").unwrap();
        let last = std::sync::Mutex::new((String::new(), 0u64));
        install(&data, &model, &hw, Arc::new(move |p| {
            let mut l = last.lock().unwrap();
            let pct = if p.total > 0 { p.done * 100 / p.total } else { 0 };
            if l.0 != p.phase || pct >= l.1 + 10 || pct < l.1 {
                println!("[{}] {} {}%", p.phase, p.file, pct);
                *l = (p.phase.clone(), pct);
            }
        }))
        .await
        .unwrap();
        assert!(installed(&data, &hw).iter().any(|m| m.id == "qwen3-1.7b"));

        let t = Instant::now();
        let (port, key) = ensure_running(&data, "qwen3-1.7b", 16384).await.unwrap();
        println!("moteur prêt en {:?} sur le port {port}", t.elapsed());
        let client = OllamaClient::local(port, &key).unwrap();
        let ask = |text: &str| ChatMessage { role: "user".into(), content: text.into(), thinking: None, tool_calls: None, tool_name: None, images: None };

        let mut out = String::new();
        client.chat_stream("local:qwen3-1.7b", &[ask("Réponds uniquement par le mot : bonjour")], None, Some(serde_json::json!({ "think": false, "temperature": 0, "num_predict": 32 })), |c| out.push_str(&c.content)).await.unwrap();
        println!("réponse : {out:?}");
        assert!(out.to_lowercase().contains("bonjour"), "{out}");

        let tools = vec![serde_json::json!({ "type": "function", "function": { "name": "get_weather", "description": "Météo d'une ville", "parameters": { "type": "object", "properties": { "city": { "type": "string" } }, "required": ["city"] } } })];
        let mut calls = vec![];
        let mut usage = (0, 0);
        client.chat_stream("local:qwen3-1.7b", &[ask("Quel temps fait-il à Paris ? Utilise l'outil.")], Some(tools), Some(serde_json::json!({ "think": false, "temperature": 0 })), |c| {
            calls.extend(c.tool_calls);
            if c.done {
                usage = (c.prompt_tokens, c.eval_tokens);
            }
        })
        .await
        .unwrap();
        println!("appels d'outils : {calls:?} ; jetons : {usage:?}");
        assert_eq!(calls[0]["function"]["name"], "get_weather");
        assert!(usage.0 > 0 && usage.1 > 0);

        // Un contexte bien plus long que 8 000 jetons : l'information est au début, ~11 000 jetons de remplissage suivent.
        let filler = "Le ciel est bleu et la mer est calme. ".repeat(1100);
        let long = format!("Le code secret est 7421. Retiens-le.

{filler}

Quel est le code secret ? Réponds seulement par le nombre.");
        let mut answer = String::new();
        let mut used = 0;
        let t = Instant::now();
        client.chat_stream("local:qwen3-1.7b", &[ask(&long)], None, Some(serde_json::json!({ "think": false, "temperature": 0, "num_predict": 16 })), |c| {
            answer.push_str(&c.content);
            if c.done {
                used = c.prompt_tokens;
            }
        })
        .await
        .unwrap();
        println!("long prompt : {used} jetons lus en {:?} ; réponse {answer:?}", t.elapsed());
        assert!(used > 9000, "le prompt doit dépasser les 8 000 jetons de l'ancienne limite : {used}");
        assert!(answer.contains("7421"), "{answer}");
        println!("{}", tail(&root(&data).join("server.log")));

        // Changer de contexte relance le moteur.
        let (_, key2) = ensure_running(&data, "qwen3-1.7b", 8192).await.unwrap();
        assert_ne!(key, key2, "un autre contexte redémarre le moteur");
        shutdown().await;
        assert!(running_model().await.is_none());
    }
}
