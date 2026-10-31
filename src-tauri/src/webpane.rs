//! Navigateur intégré : une vraie page web (WebView2) posée sur le panneau de droite de la fenêtre.
//!
//! Sécurité :
//! - la page n'a aucun accès aux commandes de l'application : elle vient d'une origine distante, et la capacité
//!   `default` ne déclare aucune adresse distante (`remote`), donc Tauri refuse ses appels ;
//! - seules les adresses http(s) publiques sont ouvertes (pas de `file:`, ni de réseau local ou privé),
//!   à l'ouverture comme à chaque navigation ;
//! - navigation privée (rien n'est gardé sur le disque), téléchargements et autorisations (caméra, position…) refusés ;
//! - une fenêtre qui s'ouvre (`window.open`, liens `_blank`) s'affiche dans le même panneau.
//!
//! L'interface mesure le panneau et envoie sa position ; le contenu d'une page n'est jamais renvoyé à Lama.

use std::net::IpAddr;

use serde::Serialize;
use tauri::webview::{NewWindowResponse, PageLoadEvent, PermissionResponse, WebviewBuilder};
use tauri::{Emitter, LogicalPosition, LogicalSize, Manager, Webview, WebviewUrl, Window};

const BLANK: &str = "about:blank";

/// Les commandes s'exécutent l'une après l'autre : fermer puis rouvrir le panneau d'un coup ne doit pas les croiser.
static GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn label_of(window: &Window) -> String {
    format!("webpane-{}", window.label())
}

/// Adresse que le panneau a le droit d'afficher (contrôle sans réseau).
pub fn allowed(url: &str) -> bool {
    url == BLANK || crate::web::check_url(url).is_ok()
}

/// Comme `allowed`, et en plus le nom de l'hôte est résolu : un nom qui pointe vers une adresse privée est refusé.
async fn vet(url: &str) -> Result<tauri::Url, String> {
    let u = crate::web::check_url(url)?;
    if let Some(host) = u.host_str() {
        let host = host.trim_matches(['[', ']']);
        if host.parse::<IpAddr>().is_err() {
            let port = u.port_or_known_default().unwrap_or(443);
            let addrs: Vec<_> = tokio::net::lookup_host((host, port)).await.map_err(|e| format!("Résolution de {host} impossible : {e}"))?.collect();
            if addrs.iter().any(|a| crate::web::is_private_ip(a.ip())) {
                return Err(format!("Adresse privée refusée : {host} pointe vers le réseau local"));
            }
        }
    }
    u.as_str().parse::<tauri::Url>().map_err(|e| e.to_string())
}

#[derive(Clone, Serialize)]
struct Nav {
    window: String,
    url: Option<String>,
    title: Option<String>,
    loading: Option<bool>,
}

fn announce(app: &tauri::AppHandle, window: &str, url: Option<&str>, title: Option<&str>, loading: Option<bool>) {
    let _ = app.emit("webpane:nav", Nav { window: window.into(), url: url.map(Into::into), title: title.map(Into::into), loading });
}

fn pane(window: &Window) -> Option<Webview> {
    window.app_handle().get_webview(&label_of(window))
}

fn place(view: &Webview, x: f64, y: f64, w: f64, h: f64) -> Result<(), String> {
    view.set_position(LogicalPosition::new(x, y)).map_err(|e| e.to_string())?;
    view.set_size(LogicalSize::new(w.max(1.0), h.max(1.0))).map_err(|e| e.to_string())
}

/// Ouvre le panneau (ou le déplace et le fait naviguer s'il existe déjà).
#[tauri::command]
pub async fn webpane_open(window: Window, url: String, x: f64, y: f64, w: f64, h: f64) -> Result<(), String> {
    let target = vet(&url).await?;
    let _gate = GATE.lock().await;
    if let Some(view) = pane(&window) {
        place(&view, x, y, w, h)?;
        view.show().map_err(|e| e.to_string())?;
        if view.url().map(|u| u != target).unwrap_or(true) {
            view.navigate(target).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    let (label, owner, app) = (label_of(&window), window.label().to_string(), window.app_handle().clone());
    let builder = WebviewBuilder::new(&label, WebviewUrl::External(target))
        .incognito(true)
        .on_navigation({
            let (app, owner) = (app.clone(), owner.clone());
            move |u| {
                let ok = allowed(u.as_str());
                if ok {
                    announce(&app, &owner, Some(u.as_str()), None, Some(true));
                }
                ok
            }
        })
        .on_page_load({
            let owner = owner.clone();
            move |view, p| announce(view.app_handle(), &owner, Some(p.url().as_str()), None, Some(matches!(p.event(), PageLoadEvent::Started)))
        })
        .on_document_title_changed({
            let owner = owner.clone();
            move |view, title| announce(view.app_handle(), &owner, None, Some(&title), None)
        })
        .on_new_window({
            let (app, label) = (app.clone(), label.clone());
            move |url, _| {
                if allowed(url.as_str()) {
                    // Hors du gestionnaire : naviguer depuis la boucle de la fenêtre pourrait se bloquer.
                    let (app, label) = (app.clone(), label.clone());
                    tauri::async_runtime::spawn(async move {
                        if let Some(v) = app.get_webview(&label) {
                            let _ = v.navigate(url);
                        }
                    });
                }
                NewWindowResponse::Deny
            }
        })
        .on_download(|_, _| false)
        .on_permission_request(|_, _| PermissionResponse::Deny);
    window
        .add_child(builder, LogicalPosition::new(x, y), LogicalSize::new(w.max(1.0), h.max(1.0)))
        .map_err(|e| format!("Navigateur intégré impossible : {e}"))?;
    Ok(())
}

/// Suit le rectangle du panneau (déplacement, redimensionnement de la fenêtre).
#[tauri::command]
pub async fn webpane_bounds(window: Window, x: f64, y: f64, w: f64, h: f64) -> Result<(), String> {
    let _gate = GATE.lock().await;
    match pane(&window) {
        Some(view) => place(&view, x, y, w, h),
        None => Ok(()),
    }
}

#[tauri::command]
pub async fn webpane_navigate(window: Window, url: String) -> Result<(), String> {
    let target = vet(&url).await?;
    let _gate = GATE.lock().await;
    match pane(&window) {
        Some(view) => view.navigate(target).map_err(|e| e.to_string()),
        None => Err("Le navigateur n'est pas ouvert.".into()),
    }
}

/// `back`, `forward`, `reload` ou `stop`.
#[tauri::command]
pub async fn webpane_action(window: Window, action: String) -> Result<(), String> {
    let _gate = GATE.lock().await;
    let Some(view) = pane(&window) else { return Ok(()) };
    match action.as_str() {
        "back" => view.eval("history.back()"),
        "forward" => view.eval("history.forward()"),
        "stop" => view.eval("window.stop()"),
        "reload" => view.reload(),
        other => return Err(format!("Action inconnue : {other}")),
    }
    .map_err(|e| e.to_string())
}

/// Cache le panneau sans le fermer (une boîte de dialogue de l'application doit passer devant : la page native est toujours au-dessus).
#[tauri::command]
pub async fn webpane_visible(window: Window, visible: bool) -> Result<(), String> {
    let _gate = GATE.lock().await;
    let Some(view) = pane(&window) else { return Ok(()) };
    if visible { view.show() } else { view.hide() }.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn webpane_close(window: Window) -> Result<(), String> {
    let _gate = GATE.lock().await;
    match pane(&window) {
        Some(view) => view.close().map_err(|e| e.to_string()),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_public_http_addresses_are_shown() {
        for ok in ["https://duckduckgo.com/?q=lama", "http://example.org/a?b=1", "about:blank"] {
            assert!(allowed(ok), "{ok}");
        }
        for bad in ["file:///C:/Windows/win.ini", "javascript:alert(1)", "http://localhost:1420/", "http://127.0.0.1/", "http://192.168.1.1/", "http://[::1]/", "ftp://example.org/", "data:text/html,x", "tauri://localhost/", "http://tauri.localhost/", "https://ipc.localhost/x", "http://asset.localhost/x"] {
            assert!(!allowed(bad), "{bad}");
        }
    }
}
