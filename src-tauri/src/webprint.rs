//! Impression d'une page HTML en PDF avec le moteur de rendu de l'application (WebView2) : même qualité qu'un navigateur
//! (vraies polices, tableaux, couleurs, pagination, liens cliquables, en-tête et numéros de page).

use std::{path::Path, sync::mpsc, time::Duration};

use tauri::{webview::PageLoadEvent, AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use webview2_com::{
    Microsoft::Web::WebView2::Win32::{ICoreWebView2Environment6, ICoreWebView2_7, COREWEBVIEW2_PRINT_ORIENTATION_PORTRAIT},
    PrintToPdfCompletedHandler,
};
use windows_core::{Interface, PCWSTR};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Lance l'impression sur le fil de l'interface ; le résultat arrive par `tx`.
fn start(wv: &tauri::webview::PlatformWebview, out: &str, header: &str, footer: &str, tx: mpsc::Sender<Result<(), String>>) -> Result<(), String> {
    unsafe {
        let e = |x: windows_core::Error| x.to_string();
        let core = wv.controller().CoreWebView2().map_err(e)?;
        let core7: ICoreWebView2_7 = core.cast().map_err(e)?;
        let env6: ICoreWebView2Environment6 = wv.environment().cast().map_err(e)?;
        let s = env6.CreatePrintSettings().map_err(e)?;
        s.SetOrientation(COREWEBVIEW2_PRINT_ORIENTATION_PORTRAIT).map_err(e)?;
        // A4, marges de 18 à 20 mm (en pouces).
        s.SetPageWidth(8.27).map_err(e)?;
        s.SetPageHeight(11.69).map_err(e)?;
        s.SetMarginTop(0.78).map_err(e)?;
        s.SetMarginBottom(0.78).map_err(e)?;
        s.SetMarginLeft(0.7).map_err(e)?;
        s.SetMarginRight(0.7).map_err(e)?;
        s.SetShouldPrintBackgrounds(true).map_err(e)?;
        s.SetShouldPrintHeaderAndFooter(true).map_err(e)?;
        let (header_w, footer_w, out_w) = (wide(header), wide(footer), wide(out));
        s.SetHeaderTitle(PCWSTR(header_w.as_ptr())).map_err(e)?;
        s.SetFooterUri(PCWSTR(footer_w.as_ptr())).map_err(e)?;

        let out_ptr = PCWSTR(out_w.as_ptr());
        let handler = PrintToPdfCompletedHandler::create(Box::new(move |res, ok| {
            let _keep = &out_w; // le chemin doit rester valide jusqu'à la fin de l'écriture
            let r = match res {
                Ok(()) if ok => Ok(()),
                Ok(()) => Err("Windows n'a pas pu écrire le PDF.".to_string()),
                Err(err) => Err(err.to_string()),
            };
            let _ = tx.send(r);
            Ok(())
        }));
        core7.PrintToPdf(out_ptr, &s, &handler).map_err(e)
    }
}

/// Imprime `html` dans le fichier PDF `out`. `header` : titre en haut des pages ; `footer` : texte en bas.
pub fn html_to_pdf(app: &AppHandle, html: &str, out: &Path, header: &str, footer: &str) -> Result<(), String> {
    let id = uuid::Uuid::new_v4().simple().to_string();
    let page = std::env::temp_dir().join(format!("lama-print-{id}.html"));
    std::fs::write(&page, html).map_err(|e| format!("Écriture impossible : {e}"))?;
    let result = (|| {
        let url = tauri::Url::from_file_path(&page).map_err(|_| "Chemin de page invalide.".to_string())?;
        let (tx, rx) = mpsc::channel::<Result<(), String>>();
        let label = format!("pdf-{}", &id[..8]);
        let (out_s, header_s, footer_s) = (out.to_string_lossy().into_owned(), header.to_string(), footer.to_string());
        let tx_load = tx.clone();
        WebviewWindowBuilder::new(app, &label, WebviewUrl::External(url))
            .visible(false)
            .focused(false)
            .inner_size(900.0, 1200.0)
            .on_page_load(move |w, payload| {
                if payload.event() != PageLoadEvent::Finished {
                    return;
                }
                let (out_s, header_s, footer_s, tx) = (out_s.clone(), header_s.clone(), footer_s.clone(), tx_load.clone());
                // Un court délai laisse la mise en page se terminer (polices, images).
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(350));
                    let (tx_closure, tx_fail) = (tx.clone(), tx);
                    let sent = w.with_webview(move |wv| {
                        let tx_err = tx_closure.clone();
                        if let Err(e) = start(&wv, &out_s, &header_s, &footer_s, tx_closure) {
                            let _ = tx_err.send(Err(e));
                        }
                    });
                    if let Err(e) = sent {
                        let _ = tx_fail.send(Err(e.to_string()));
                    }
                });
            })
            .build()
            .map_err(|e| format!("Fenêtre d'impression impossible : {e}"))?;
        let r = rx.recv_timeout(Duration::from_secs(60)).map_err(|_| "L'impression du PDF a pris trop de temps.".to_string())?;
        if let Some(w) = app.get_webview_window(&label) {
            let _ = w.destroy();
        }
        r
    })();
    let _ = std::fs::remove_file(&page);
    result
}
