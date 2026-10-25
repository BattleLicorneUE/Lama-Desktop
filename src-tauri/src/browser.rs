//! Navigateur de Lama : une fenêtre Edge, Chrome ou Brave avec son propre profil (cookies et sessions à part de ton navigateur),
//! que l'assistant pilote par le protocole DevTools. Une connexion faite ici reste ouverte d'une fois sur l'autre.
//!
//! Sécurité des identifiants :
//! - le mot de passe n'est jamais donné au modèle : il part du coffre vers le champ de la page par `Input.insertText`
//!   (comme une frappe), sans passer par du code de la page ;
//! - la saisie n'a lieu que si l'adresse de la page correspond à celle de l'entrée du coffre (https, même site) :
//!   un faux site ne reçoit rien, même si le modèle y est conduit ;
//! - la page est revérifiée avant chaque champ rempli (elle a pu changer entre-temps) ;
//! - le port de débogage est local, tiré au hasard, et le navigateur est fermé avec Lama.

use crate::sync::MutexExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use zeroize::Zeroizing;

type Ws = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

const CALL_TIMEOUT: Duration = Duration::from_secs(20);

struct Cdp {
    ws: Ws,
    next: u64,
}

impl Cdp {
    async fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.next += 1;
        let id = self.next;
        let msg = json!({ "id": id, "method": method, "params": params }).to_string();
        self.ws.send(Message::Text(msg)).await.map_err(|e| format!("Navigateur injoignable : {e}"))?;
        let wait = async {
            while let Some(m) = self.ws.next().await {
                let m = m.map_err(|e| format!("Navigateur injoignable : {e}"))?;
                if let Message::Text(t) = m {
                    let v: Value = serde_json::from_str(&t).map_err(|e| e.to_string())?;
                    if v["id"].as_u64() == Some(id) {
                        return match v.get("error") {
                            Some(e) => Err(e["message"].as_str().unwrap_or("erreur du navigateur").to_string()),
                            None => Ok(v["result"].clone()),
                        };
                    }
                }
            }
            Err("Le navigateur s'est fermé.".to_string())
        };
        tokio::time::timeout(CALL_TIMEOUT, wait).await.map_err(|_| format!("Le navigateur n'a pas répondu ({method})."))?
    }

    /// Évalue du JavaScript dans la page et renvoie sa valeur (le script rend une chaîne JSON ou une valeur simple).
    async fn eval(&mut self, js: &str) -> Result<Value, String> {
        let r = self.call("Runtime.evaluate", json!({ "expression": js, "returnByValue": true, "awaitPromise": true })).await?;
        if let Some(ex) = r.get("exceptionDetails") {
            return Err(format!("Erreur dans la page : {}", ex["exception"]["description"].as_str().or(ex["text"].as_str()).unwrap_or("?")));
        }
        Ok(r["result"]["value"].clone())
    }

    async fn eval_json(&mut self, js: &str) -> Result<Value, String> {
        match self.eval(js).await? {
            Value::String(s) => serde_json::from_str(&s).map_err(|e| e.to_string()),
            other => Ok(other),
        }
    }

    async fn press_enter(&mut self) -> Result<(), String> {
        for t in ["keyDown", "keyUp"] {
            let mut p = json!({ "type": t, "key": "Enter", "code": "Enter", "windowsVirtualKeyCode": 13, "nativeVirtualKeyCode": 13 });
            if t == "keyDown" {
                p["text"] = json!("\r");
            }
            self.call("Input.dispatchKeyEvent", p).await?;
        }
        Ok(())
    }
}

struct Session {
    child: tokio::process::Child,
    port: u16,
    target: String,
    cdp: Cdp,
    name: String,
}

static SESSION: Mutex<Option<Session>> = Mutex::const_new(None);
static DIR: StdMutex<Option<PathBuf>> = StdMutex::new(None);
/// Identifiant du processus du navigateur, pour le fermer à la sortie de Lama même si le verrou est pris.
static PID: AtomicU32 = AtomicU32::new(0);

/// Dossier du profil du navigateur (dans le profil de l'utilisateur).
pub fn set_dir(dir: Option<&Path>) {
    *DIR.lock_safe() = dir.map(|d| d.join("browser"));
}

fn profile_dir() -> Result<PathBuf, String> {
    DIR.lock_safe().clone().ok_or_else(|| "Aucun profil ouvert.".to_string())
}

/// Ferme le navigateur sans attendre (sortie de l'application, effacement des données).
pub fn kill_now() {
    let pid = PID.swap(0, Ordering::SeqCst);
    if pid != 0 {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).creation_flags(0x0800_0000).output();
    }
}

pub async fn shutdown() {
    let mut g = SESSION.lock().await;
    if let Some(mut s) = g.take() {
        let _ = s.child.kill().await;
    }
    kill_now();
}

// ---------------------------------------------------------------- trouver et lancer le navigateur

/// Navigateur à base de Chromium installé : Edge, Chrome ou Brave, dans cet ordre.
pub fn find_browser() -> Result<(PathBuf, &'static str), String> {
    let bases: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"].iter().filter_map(|k| std::env::var_os(k)).map(PathBuf::from).collect();
    let known: [(&str, &str, &str); 3] = [
        ("Microsoft\\Edge\\Application\\msedge.exe", "Edge", "msedge"),
        ("Google\\Chrome\\Application\\chrome.exe", "Chrome", "chrome"),
        ("BraveSoftware\\Brave-Browser\\Application\\brave.exe", "Brave", "brave"),
    ];
    for (rel, name, _) in known {
        for b in &bases {
            let p = b.join(rel);
            if p.is_file() {
                return Ok((p, name));
            }
        }
    }
    Err("Aucun navigateur compatible (Edge, Chrome ou Brave) n'est installé sur ce PC.".into())
}

async fn http_json(url: &str) -> Result<Value, String> {
    let c = reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(5)).build().map_err(|e| e.to_string())?;
    c.get(url).send().await.map_err(|e| e.to_string())?.json::<Value>().await.map_err(|e| e.to_string())
}

/// Premier onglet de la fenêtre (le plus récemment utilisé) : identifiant et adresse du canal DevTools.
async fn first_page(port: u16) -> Result<(String, String), String> {
    let list = http_json(&format!("http://127.0.0.1:{port}/json/list")).await?;
    list.as_array()
        .and_then(|a| a.iter().find(|t| t["type"] == "page" && t["webSocketDebuggerUrl"].is_string()))
        .map(|t| (t["id"].as_str().unwrap_or("").to_string(), t["webSocketDebuggerUrl"].as_str().unwrap().to_string()))
        .ok_or_else(|| "Aucun onglet ouvert dans le navigateur de Lama.".to_string())
}

async fn connect(url: &str) -> Result<Cdp, String> {
    let (ws, _) = connect_async(url).await.map_err(|e| format!("Connexion au navigateur impossible : {e}"))?;
    Ok(Cdp { ws, next: 0 })
}

async fn launch() -> Result<Session, String> {
    let dir = profile_dir()?;
    let (exe, name) = find_browser()?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(dir.join("DevToolsActivePort"));
    let child = tokio::process::Command::new(exe)
        .arg(format!("--user-data-dir={}", dir.display()))
        .args(["--remote-debugging-port=0", "--no-first-run", "--no-default-browser-check", "--disable-sync", "--new-window", "about:blank"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Lancement du navigateur impossible : {e}"))?;
    PID.store(child.id().unwrap_or(0), Ordering::SeqCst);
    let mut port = 0u16;
    for _ in 0..150 {
        if let Ok(t) = std::fs::read_to_string(dir.join("DevToolsActivePort")) {
            if let Some(p) = t.lines().next().and_then(|l| l.trim().parse::<u16>().ok()) {
                port = p;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if port == 0 {
        let mut child = child;
        let _ = child.kill().await;
        return Err(format!("{name} n'a pas démarré à temps."));
    }
    // L'onglet peut mettre un instant à exister.
    let mut page = Err("".to_string());
    for _ in 0..50 {
        page = first_page(port).await;
        if page.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let (target, ws) = page?;
    let cdp = connect(&ws).await?;
    Ok(Session { child, port, target, cdp, name: name.to_string() })
}

/// Session prête : relance le navigateur s'il a été fermé, et suit l'onglet actif si l'utilisateur en a changé.
async fn ready(g: &mut Option<Session>) -> Result<(), String> {
    let alive = match g.as_mut() {
        Some(s) => matches!(s.child.try_wait(), Ok(None)),
        None => false,
    };
    if !alive {
        *g = Some(launch().await?);
        return Ok(());
    }
    let s = g.as_mut().unwrap();
    match first_page(s.port).await {
        Ok((id, ws)) if id != s.target => {
            s.cdp = connect(&ws).await?;
            s.target = id;
        }
        Ok(_) => {}
        Err(_) => {
            // Plus aucun onglet : on en rouvre un.
            *g = Some(launch().await?);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- vérification de l'adresse

/// Sites qui hébergent les pages de milliers de personnes : un mot de passe de « github.io » ne vaut pas pour « quelqu'un.github.io ».
const SHARED: &[&str] = &[
    "github.io", "gitlab.io", "herokuapp.com", "blogspot.com", "wordpress.com", "vercel.app", "netlify.app", "pages.dev", "workers.dev", "web.app",
    "firebaseapp.com", "azurewebsites.net", "amazonaws.com", "cloudfront.net", "appspot.com", "onrender.com", "fly.dev", "glitch.me", "repl.co",
    "co.uk", "org.uk", "ac.uk", "gov.uk", "com.au", "co.jp", "co.nz", "com.br", "com.cn", "co.in", "com.mx", "com.tr", "co.za",
];

fn norm_host(url: &reqwest::Url) -> Option<String> {
    let h = url.host_str()?.to_lowercase();
    let h = h.trim_end_matches('.');
    Some(h.strip_prefix("www.").unwrap_or(h).to_string())
}

/// La page est-elle bien le site de l'entrée ? https obligatoire (http seulement pour localhost), même domaine ou sous-domaine de celui-ci.
pub fn host_allowed(entry_url: &str, page_url: &str) -> Result<(), String> {
    let entry = entry_url.trim();
    if entry.is_empty() {
        return Err("Cette entrée n'a pas d'adresse de site : ajoute-la dans le coffre pour autoriser la connexion automatique.".into());
    }
    let entry = reqwest::Url::parse(&if entry.contains("://") { entry.to_string() } else { format!("https://{entry}") }).map_err(|_| "Adresse de l'entrée invalide.".to_string())?;
    let page = reqwest::Url::parse(page_url).map_err(|_| "Aucune page web n'est ouverte.".to_string())?;
    let (Some(eh), Some(ph)) = (norm_host(&entry), norm_host(&page)) else { return Err("Adresse sans nom de site.".into()) };
    let local = matches!(ph.as_str(), "localhost" | "127.0.0.1" | "[::1]");
    if page.scheme() != "https" && !(page.scheme() == "http" && local) {
        return Err(format!("La page ({ph}) n'est pas en https : rien n'a été saisi."));
    }
    let same = ph == eh;
    let sub = ph.ends_with(&format!(".{eh}")) && eh.contains('.') && !SHARED.contains(&eh.as_str()) && eh.parse::<std::net::IpAddr>().is_err();
    if same || sub {
        Ok(())
    } else {
        Err(format!("L'adresse de la page ({ph}) ne correspond pas à l'entrée du coffre ({eh}) : rien n'a été saisi."))
    }
}

// ---------------------------------------------------------------- scripts exécutés dans la page

const VIS: &str = "const vis=e=>{const r=e.getBoundingClientRect();const s=getComputedStyle(e);return r.width>0&&r.height>0&&s.visibility!=='hidden'&&s.display!=='none'};";

fn js_snapshot() -> String {
    format!(
        "(()=>{{{VIS}document.querySelectorAll('[data-lama]').forEach(e=>e.removeAttribute('data-lama'));\
         const els=[...document.querySelectorAll('a[href],button,input,select,textarea,[role=button],[role=link],[contenteditable=true],[onclick]')].filter(vis).slice(0,80);\
         const label=e=>(e.getAttribute('aria-label')||(e.labels&&e.labels[0]&&e.labels[0].innerText)||e.placeholder||e.name||e.id||e.innerText||e.value||e.title||'').trim().replace(/\\s+/g,' ').slice(0,80);\
         const items=els.map((e,i)=>{{e.setAttribute('data-lama',String(i+1));const t=e.tagName.toLowerCase();return{{i:i+1,tag:t,type:e.type||'',label:t==='input'&&e.type==='password'?(e.getAttribute('aria-label')||e.placeholder||e.name||'mot de passe'):label(e),href:t==='a'?e.href.slice(0,140):''}}}});\
         return JSON.stringify({{url:location.href,title:document.title,text:document.body?document.body.innerText.replace(/\\n{{3,}}/g,'\\n\\n').slice(0,5000):'',items}})}})()"
    )
}

fn js_info() -> &'static str {
    "JSON.stringify({url:location.href,title:document.title,ready:document.readyState})"
}

/// Repère le champ identifiant et le champ mot de passe visibles (attributs data-lama-user / data-lama-pass).
fn js_find_login() -> String {
    format!(
        "(()=>{{{VIS}document.querySelectorAll('[data-lama-user],[data-lama-pass]').forEach(e=>{{e.removeAttribute('data-lama-user');e.removeAttribute('data-lama-pass')}});\
         const pw=[...document.querySelectorAll('input[type=password]')].filter(e=>vis(e)&&!e.disabled)[0]||null;\
         const textual=e=>['text','email','tel',''].includes((e.getAttribute('type')||'text').toLowerCase())&&!e.disabled&&!e.readOnly;\
         const cands=[...document.querySelectorAll('input')].filter(e=>vis(e)&&textual(e));\
         let user=null;\
         if(pw){{const before=cands.filter(e=>e.compareDocumentPosition(pw)&Node.DOCUMENT_POSITION_FOLLOWING);user=before.length?before[before.length-1]:null}}\
         else{{user=cands.find(e=>/user|login|email|mail|ident|account|tel/i.test([e.name,e.id,e.autocomplete,e.getAttribute('aria-label')||'',e.placeholder].join(' ')))||cands[0]||null}}\
         if(user)user.setAttribute('data-lama-user','1');if(pw)pw.setAttribute('data-lama-pass','1');\
         return JSON.stringify({{user:!!user,pass:!!pw}})}})()"
    )
}

fn js_focus(selector: &str) -> String {
    format!("(()=>{{const e=document.querySelector('{selector}');if(!e)return false;e.scrollIntoView({{block:'center'}});e.focus();if(e.select)e.select();return true}})()")
}

fn js_center(selector: &str) -> String {
    format!("(()=>{{const e=document.querySelector('{selector}');if(!e)return null;e.scrollIntoView({{block:'center'}});const r=e.getBoundingClientRect();return JSON.stringify({{x:r.x+r.width/2,y:r.y+r.height/2}})}})()")
}

// ---------------------------------------------------------------- opérations

pub struct PageInfo {
    pub url: String,
    pub title: String,
}

impl PageInfo {
    fn from(v: &Value) -> Self {
        Self { url: v["url"].as_str().unwrap_or("").to_string(), title: v["title"].as_str().unwrap_or("").to_string() }
    }
    pub fn to_json(&self) -> Value {
        json!({ "url": self.url, "title": self.title })
    }
}

async fn wait_loaded(cdp: &mut Cdp) -> Result<PageInfo, String> {
    for _ in 0..80 {
        let v = cdp.eval_json(js_info()).await?;
        if v["ready"] == "complete" {
            return Ok(PageInfo::from(&v));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    cdp.eval_json(js_info()).await.map(|v| PageInfo::from(&v))
}

/// Page actuellement affichée, sans rien lancer si le navigateur est fermé.
pub async fn page_info() -> Option<PageInfo> {
    let mut g = SESSION.lock().await;
    if g.as_mut().is_none_or(|s| !matches!(s.child.try_wait(), Ok(None))) {
        return None;
    }
    ready(&mut g).await.ok()?;
    let v = g.as_mut()?.cdp.eval_json(js_info()).await.ok()?;
    Some(PageInfo::from(&v))
}

pub async fn is_running() -> Option<String> {
    let mut g = SESSION.lock().await;
    g.as_mut().and_then(|s| matches!(s.child.try_wait(), Ok(None)).then(|| s.name.clone()))
}

/// Ouvre une adresse dans le navigateur de Lama (http ou https seulement).
pub async fn open(url: &str) -> Result<String, String> {
    let u = reqwest::Url::parse(url.trim()).map_err(|_| format!("Adresse invalide : {url}"))?;
    if !matches!(u.scheme(), "http" | "https") {
        return Err("Seules les adresses http(s) sont ouvertes dans le navigateur.".into());
    }
    let mut g = SESSION.lock().await;
    ready(&mut g).await?;
    let s = g.as_mut().unwrap();
    s.cdp.call("Page.navigate", json!({ "url": u.as_str() })).await?;
    tokio::time::sleep(Duration::from_millis(400)).await;
    let info = wait_loaded(&mut s.cdp).await?;
    Ok(format!("Page ouverte : « {} » ({})", info.title, info.url))
}

pub async fn close() -> Result<String, String> {
    shutdown().await;
    Ok("Navigateur fermé.".into())
}

/// Texte de la page et éléments interactifs numérotés (pour browser_click et browser_type). Les valeurs des champs ne sont jamais lues.
pub async fn read() -> Result<String, String> {
    let mut g = SESSION.lock().await;
    if g.is_none() {
        return Err("Le navigateur n'est pas ouvert : utilise browser_open d'abord.".into());
    }
    ready(&mut g).await?;
    let v = g.as_mut().unwrap().cdp.eval_json(&js_snapshot()).await?;
    let mut out = format!("Adresse : {}\nTitre : {}\n\nTexte de la page :\n{}\n\nÉléments :\n", v["url"].as_str().unwrap_or(""), v["title"].as_str().unwrap_or(""), v["text"].as_str().unwrap_or(""));
    for it in v["items"].as_array().cloned().unwrap_or_default() {
        let kind = match (it["tag"].as_str().unwrap_or(""), it["type"].as_str().unwrap_or("")) {
            ("a", _) => "lien",
            ("button", _) | (_, "submit") | (_, "button") => "bouton",
            (_, "password") => "champ mot de passe",
            (_, "checkbox") => "case à cocher",
            ("select", _) => "liste",
            ("textarea", _) => "zone de texte",
            _ => "champ",
        };
        let href = it["href"].as_str().filter(|h| !h.is_empty()).map(|h| format!(" → {h}")).unwrap_or_default();
        out.push_str(&format!("[{}] {} « {} »{}\n", it["i"], kind, it["label"].as_str().unwrap_or(""), href));
    }
    Ok(out)
}

pub async fn click(index: u32) -> Result<String, String> {
    let mut g = SESSION.lock().await;
    ready(&mut g).await?;
    let cdp = &mut g.as_mut().unwrap().cdp;
    let pos = cdp.eval_json(&js_center(&format!("[data-lama=\"{index}\"]"))).await?;
    if pos.is_null() {
        return Err(format!("Élément [{index}] introuvable : relis la page avec browser_read."));
    }
    let (x, y) = (pos["x"].as_f64().unwrap_or(0.0), pos["y"].as_f64().unwrap_or(0.0));
    for (t, extra) in [("mouseMoved", 0), ("mousePressed", 1), ("mouseReleased", 1)] {
        cdp.call("Input.dispatchMouseEvent", json!({ "type": t, "x": x, "y": y, "button": if extra == 1 { "left" } else { "none" }, "clickCount": extra })).await?;
    }
    tokio::time::sleep(Duration::from_millis(600)).await;
    let info = cdp.eval_json(js_info()).await.map(|v| PageInfo::from(&v)).unwrap_or(PageInfo { url: String::new(), title: String::new() });
    Ok(format!("Clic sur [{index}]. Page : « {} » ({})", info.title, info.url))
}

/// Saisit du texte ordinaire dans un champ. Jamais un mot de passe : ils passent par `login`.
pub async fn type_text(index: u32, text: &str, submit: bool) -> Result<String, String> {
    let mut g = SESSION.lock().await;
    ready(&mut g).await?;
    let cdp = &mut g.as_mut().unwrap().cdp;
    let kind = cdp.eval(&format!("(()=>{{const e=document.querySelector('[data-lama=\"{index}\"]');return e?(e.type||e.tagName).toLowerCase():null}})()")).await?;
    match kind.as_str() {
        None => return Err(format!("Élément [{index}] introuvable : relis la page avec browser_read.")),
        Some("password") => return Err("Les mots de passe ne se saisissent pas avec cet outil : utilise vault_login.".into()),
        _ => {}
    }
    cdp.eval(&js_focus(&format!("[data-lama=\"{index}\"]"))).await?;
    cdp.call("Input.insertText", json!({ "text": text })).await?;
    if submit {
        cdp.press_enter().await?;
        tokio::time::sleep(Duration::from_millis(600)).await;
    }
    Ok(format!("Texte saisi dans [{index}]{}.", if submit { " puis Entrée pressée" } else { "" }))
}

/// Remplit identifiant et/ou mot de passe sur la page, après avoir vérifié qu'elle correspond au site de l'entrée.
/// Gère aussi les connexions en deux temps (identifiant, puis mot de passe sur la page suivante). Aucun secret n'est renvoyé.
pub async fn login(entry_url: &str, username: Option<Zeroizing<String>>, password: Option<Zeroizing<String>>, submit: bool) -> Result<String, String> {
    let mut g = SESSION.lock().await;
    if g.is_none() {
        return Err("Le navigateur n'est pas ouvert : ouvre d'abord la page de connexion avec browser_open.".into());
    }
    ready(&mut g).await?;
    let cdp = &mut g.as_mut().unwrap().cdp;

    // Contrôle de l'adresse juste avant CHAQUE champ : la page a pu changer, ou être une redirection vers un faux site.
    async fn check(cdp: &mut Cdp, entry: &str) -> Result<PageInfo, String> {
        let info = PageInfo::from(&cdp.eval_json(js_info()).await?);
        host_allowed(entry, &info.url)?;
        Ok(info)
    }
    async fn fill(cdp: &mut Cdp, selector: &str, text: &str) -> Result<(), String> {
        if cdp.eval(&js_focus(selector)).await? != Value::Bool(true) {
            return Err("Champ introuvable sur la page.".into());
        }
        cdp.call("Input.insertText", json!({ "text": text })).await.map(|_| ())
    }

    let mut did: Vec<&str> = Vec::new();
    let mut info = check(cdp, entry_url).await?;
    let mut found = cdp.eval_json(&js_find_login()).await?;

    if let Some(u) = &username {
        if found["user"] != true {
            return Err("Aucun champ identifiant trouvé sur cette page.".into());
        }
        check(cdp, entry_url).await?;
        fill(cdp, "[data-lama-user=\"1\"]", u).await?;
        did.push("identifiant");
    }
    if let Some(p) = &password {
        if found["pass"] != true {
            if username.is_none() {
                return Err("Aucun champ mot de passe sur cette page.".into());
            }
            // Connexion en deux temps : on valide l'identifiant, puis on attend le champ mot de passe.
            cdp.press_enter().await?;
            let mut appeared = false;
            for _ in 0..40 {
                tokio::time::sleep(Duration::from_millis(250)).await;
                found = cdp.eval_json(&js_find_login()).await.unwrap_or(Value::Null);
                if found["pass"] == true {
                    appeared = true;
                    break;
                }
            }
            if !appeared {
                return Err("L'identifiant est saisi, mais le champ mot de passe n'est pas apparu (code de vérification, CAPTCHA ou autre étape ?). Termine dans la fenêtre du navigateur.".into());
            }
        }
        info = check(cdp, entry_url).await?;
        fill(cdp, "[data-lama-pass=\"1\"]", p).await?;
        did.push("mot de passe");
    }
    if submit && (password.is_some() || username.is_some()) {
        check(cdp, entry_url).await?;
        cdp.press_enter().await?;
        tokio::time::sleep(Duration::from_millis(1500)).await;
        info = PageInfo::from(&cdp.eval_json(js_info()).await?);
        return Ok(format!("{} saisi(s), Entrée pressée. Page maintenant : « {} » ({}).", did.join(" et "), info.title, info.url));
    }
    Ok(format!("{} saisi(s) sur « {} » ({}). Rien n'a été validé.", did.join(" et "), info.title, info.url))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_entrys_own_https_site_is_accepted() {
        let ok = |e: &str, p: &str| host_allowed(e, p).is_ok();
        assert!(ok("https://github.com/login", "https://github.com/session"));
        assert!(ok("github.com", "https://www.github.com/login"));
        assert!(ok("https://google.com", "https://accounts.google.com/signin"), "sous-domaine du site");
        assert!(ok("https://www.example.com", "https://login.example.com/"));
        assert!(ok("https://GITHUB.com.", "https://github.com/"), "casse et point final");
        // Faux sites et ruses classiques.
        for fake in [
            "https://github.com.evil.example/login",
            "https://evil-github.com/",
            "https://githubb.com/",
            "https://notgithub.com/",
            "https://github.com@evil.example/",
            "https://evil.example/github.com",
            "https://xn--gthub-qta.com/",
            "http://github.com/login",
            "about:blank",
            "file:///C:/x.html",
        ] {
            assert!(host_allowed("https://github.com", fake).is_err(), "{fake} doit être refusé");
        }
        // Hébergeurs partagés : le domaine nu n'autorise pas les pages des autres.
        assert!(host_allowed("https://github.io", "https://pirate.github.io/").is_err());
        assert!(host_allowed("https://monsite.github.io", "https://monsite.github.io/login").is_ok());
        assert!(host_allowed("https://example.co.uk", "https://autre.co.uk/").is_err());
        // Adresse IP : exacte seulement ; localhost en http permis pour les tests de développement.
        assert!(host_allowed("https://10.0.0.5", "https://10.0.0.5/login").is_ok());
        assert!(host_allowed("https://10.0.0.5", "https://x.10.0.0.5/").is_err());
        assert!(host_allowed("http://localhost:8080", "http://localhost:3000/login").is_ok());
        assert!(host_allowed("http://github.com", "http://github.com/").is_err(), "pas de mot de passe en clair sur Internet");
        // Entrée sans site : jamais de saisie automatique.
        assert!(host_allowed("", "https://github.com/").unwrap_err().contains("pas d'adresse"));
        assert!(host_allowed("   ", "https://github.com/").is_err());
    }

    #[test]
    fn a_browser_is_looked_for_in_the_usual_places() {
        // Sur un PC sans Edge, Chrome ni Brave, l'erreur est claire ; sinon on obtient un fichier existant.
        match find_browser() {
            Ok((p, name)) => assert!(p.is_file() && ["Edge", "Chrome", "Brave"].contains(&name)),
            Err(e) => assert!(e.contains("Aucun navigateur")),
        }
    }

    /// Serveur de pages de connexion : /login (un temps), /step1 (identifiant puis mot de passe, comme Google ou Microsoft).
    /// Les formulaires reçus sont rangés dans `posts`, décodés.
    async fn login_server(posts: std::sync::Arc<std::sync::Mutex<Vec<Vec<(String, String)>>>>) -> u16 {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = listener.accept().await else { return };
                let posts = posts.clone();
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 4096];
                    let (head, body) = loop {
                        let n = s.read(&mut chunk).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        buf.extend_from_slice(&chunk[..n]);
                        let text = String::from_utf8_lossy(&buf).to_string();
                        if let Some(i) = text.find("\r\n\r\n") {
                            let len = text[..i].to_lowercase().split("content-length:").nth(1).and_then(|r| r.lines().next()).and_then(|l| l.trim().parse::<usize>().ok()).unwrap_or(0);
                            if buf.len() >= i + 4 + len {
                                break (text[..i].to_string(), text[i + 4..i + 4 + len].to_string());
                            }
                        }
                    };
                    let first = head.lines().next().unwrap_or("").to_string();
                    let page = if first.starts_with("POST") {
                        let pairs: Vec<(String, String)> = reqwest::Url::parse(&format!("http://x/?{body}")).unwrap().query_pairs().map(|(k, v)| (k.to_string(), v.to_string())).collect();
                        let who = pairs.iter().find(|(k, _)| k == "email").map(|(_, v)| v.clone()).unwrap_or_default();
                        posts.lock_safe().push(pairs);
                        format!("<title>Connecté</title><h1>Bienvenue {who}</h1>")
                    } else if first.contains("/step1") {
                        // Étape 1 : l'identifiant ; l'étape 2 (mot de passe) arrive après un délai, sans changer d'adresse.
                        "<title>Étape 1</title><form id=f method=post action=/session><input name=email type=email placeholder='Adresse e-mail' autocomplete=username><button type=button id=n>Suivant</button></form>\
                         <script>const f=document.getElementById('f');const next=()=>setTimeout(()=>{f.querySelector('#n').remove();const p=document.createElement('input');p.type='password';p.name='password';f.appendChild(p);const b=document.createElement('button');b.textContent='Connexion';f.appendChild(b);p.focus()},600);\
                         document.getElementById('n').onclick=next;f.addEventListener('keydown',e=>{if(e.key==='Enter'&&!f.querySelector('[type=password]')){e.preventDefault();next()}})</script>"
                            .to_string()
                    } else {
                        "<title>Connexion</title><input name=q placeholder='Rechercher'><form method=post action=/session><input name=email type=email autocomplete=username><input name=password type=password><button>Connexion</button></form>".to_string()
                    };
                    let head = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", page.len());
                    let _ = s.write_all(head.as_bytes()).await;
                    let _ = s.write_all(page.as_bytes()).await;
                });
            }
        });
        port
    }

    /// Avec un vrai navigateur (une fenêtre s'ouvre ~20 s) : `cargo test logs_in -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore]
    async fn logs_in_on_a_real_browser_and_refuses_the_wrong_site() {
        let dir = std::env::temp_dir().join(format!("lama-browser-test-{}", std::process::id()));
        set_dir(Some(&dir));
        let posts = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let port = login_server(posts.clone()).await;
        let entry = format!("http://localhost:{port}");
        let secret = "P@ss é€ #1 {x} [y]^%~ +;";
        let creds = || (Some(Zeroizing::new("jean.dupont@example.org".to_string())), Some(Zeroizing::new(secret.to_string())));

        // 1. Formulaire classique, validé par Entrée.
        open(&format!("{entry}/login")).await.unwrap();
        let (u, p) = creds();
        let msg = login(&entry, u, p, true).await.unwrap();
        println!("{msg}");
        assert!(!msg.contains("P@ss") && !msg.contains(secret), "le résultat ne doit contenir aucun secret");
        {
            let got = posts.lock_safe();
            assert_eq!(got.len(), 1, "un seul envoi");
            assert_eq!(got[0], vec![("email".to_string(), "jean.dupont@example.org".to_string()), ("password".to_string(), secret.to_string())], "valeurs exactes, caractères spéciaux compris, dans le bon champ");
        }

        // 2. Connexion en deux temps.
        open(&format!("{entry}/step1")).await.unwrap();
        let (u, p) = creds();
        let msg = login(&entry, u, p, true).await.unwrap();
        println!("{msg}");
        {
            let got = posts.lock_safe();
            assert_eq!(got.len(), 2);
            assert_eq!(got[1][0], ("email".to_string(), "jean.dupont@example.org".to_string()));
            assert_eq!(got[1][1], ("password".to_string(), secret.to_string()));
        }

        // 3. Sans valider : les champs sont remplis, rien n'est envoyé.
        open(&format!("{entry}/login")).await.unwrap();
        let (u, p) = creds();
        let msg = login(&entry, u, p, false).await.unwrap();
        assert!(msg.contains("Rien n'a été validé"), "{msg}");
        assert_eq!(posts.lock_safe().len(), 2);

        // 4. Mauvais site : même serveur mais autre nom d'hôte (127.0.0.1 au lieu de localhost) → refus, aucun envoi.
        open(&format!("http://127.0.0.1:{port}/login")).await.unwrap();
        let (u, p) = creds();
        let err = login(&entry, u, p, true).await.unwrap_err();
        println!("{err}");
        assert!(err.contains("ne correspond pas"), "{err}");
        assert_eq!(posts.lock_safe().len(), 2, "rien n'a été saisi ni envoyé sur le faux site");

        // 5. Lecture de la page et clic par numéro.
        open(&format!("{entry}/login")).await.unwrap();
        let page = read().await.unwrap();
        println!("{page}");
        assert!(page.contains("Connexion") && page.contains("champ mot de passe") && page.contains("bouton"), "{page}");
        assert!(type_text(3, "x", false).await.is_ok() || type_text(2, "x", false).await.is_ok());
        let pw_index = page.lines().find(|l| l.contains("champ mot de passe")).and_then(|l| l.trim_start_matches('[').split(']').next()).and_then(|n| n.parse::<u32>().ok()).unwrap();
        assert!(type_text(pw_index, "interdit", false).await.unwrap_err().contains("vault_login"), "un mot de passe ne se saisit pas avec browser_type");

        close().await.unwrap();
        assert!(is_running().await.is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
