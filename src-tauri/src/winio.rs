//! Saisie au clavier et presse-papiers de Windows, pour le coffre : appels directs à l'API système.
//! Aucun texte secret ne passe par une ligne de commande, une variable d'environnement ou PowerShell.

use std::ffi::c_void;

/// Une suite de saisie : du texte (secret, effacé de la mémoire après usage), une tabulation, ou Entrée.
pub enum Piece {
    Text(zeroize::Zeroizing<String>),
    Tab,
    Enter,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct KeybdInput {
    vk: u16,
    scan: u16,
    flags: u32,
    time: u32,
    extra: usize,
}

#[repr(C)]
union InputUnion {
    ki: KeybdInput,
    // Taille de la plus grande variante (MOUSEINPUT) : INPUT fait 40 octets en 64 bits.
    pad: [u8; 32],
}

#[repr(C)]
struct Input {
    kind: u32,
    u: InputUnion,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn SendInput(count: u32, inputs: *const Input, size: i32) -> u32;
    fn GetForegroundWindow() -> isize;
    fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
    fn GetWindowTextW(hwnd: isize, buf: *mut u16, max: i32) -> i32;
    fn OpenClipboard(hwnd: isize) -> i32;
    fn CloseClipboard() -> i32;
    fn EmptyClipboard() -> i32;
    fn SetClipboardData(format: u32, handle: isize) -> isize;
    fn GetClipboardData(format: u32) -> isize;
    fn RegisterClipboardFormatW(name: *const u16) -> u32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GlobalAlloc(flags: u32, bytes: usize) -> isize;
    fn GlobalLock(h: isize) -> *mut c_void;
    fn GlobalUnlock(h: isize) -> i32;
    fn GlobalFree(h: isize) -> isize;
}

const INPUT_KEYBOARD: u32 = 1;
const KEYEVENTF_KEYUP: u32 = 0x2;
const KEYEVENTF_UNICODE: u32 = 0x4;
const VK_TAB: u16 = 0x09;
const VK_RETURN: u16 = 0x0D;
const CF_UNICODETEXT: u32 = 13;
const GMEM_MOVEABLE: u32 = 0x2;

fn key(vk: u16, scan: u16, flags: u32) -> Input {
    Input { kind: INPUT_KEYBOARD, u: InputUnion { ki: KeybdInput { vk, scan, flags, time: 0, extra: 0 } } }
}

fn send(inputs: &[Input]) -> Result<(), String> {
    let sent = unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<Input>() as i32) };
    if sent as usize == inputs.len() {
        Ok(())
    } else {
        Err("Windows a refusé la saisie (une fenêtre protégée ou en mode administrateur est peut-être active).".into())
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// La fenêtre au premier plan appartient-elle à Lama ? (Alors on ne tape rien : le secret atterrirait dans l'application elle-même.)
pub fn foreground_is_us() -> bool {
    unsafe {
        let h = GetForegroundWindow();
        if h == 0 {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, &mut pid);
        pid == std::process::id()
    }
}

/// Titre de la fenêtre au premier plan (pour dire à l'utilisateur où la saisie a eu lieu).
pub fn foreground_title() -> String {
    unsafe {
        let h = GetForegroundWindow();
        if h == 0 {
            return String::new();
        }
        let mut buf = [0u16; 256];
        let n = GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32);
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }
}

/// Tape les pièces dans la fenêtre active, caractère par caractère (Unicode : tous les symboles passent).
pub fn type_pieces(pieces: &[Piece]) -> Result<(), String> {
    for piece in pieces {
        match piece {
            Piece::Text(t) => {
                for unit in t.encode_utf16() {
                    send(&[key(0, unit, KEYEVENTF_UNICODE), key(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP)])?;
                    std::thread::sleep(std::time::Duration::from_millis(4));
                }
            }
            Piece::Tab => send(&[key(VK_TAB, 0, 0), key(VK_TAB, 0, KEYEVENTF_KEYUP)])?,
            Piece::Enter => send(&[key(VK_RETURN, 0, 0), key(VK_RETURN, 0, KEYEVENTF_KEYUP)])?,
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    Ok(())
}

fn open_clipboard() -> Result<(), String> {
    for _ in 0..10 {
        if unsafe { OpenClipboard(0) } != 0 {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    Err("Le presse-papiers est occupé par une autre application.".into())
}

unsafe fn global_bytes(bytes: &[u8]) -> Option<isize> {
    unsafe {
        let h = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1));
        if h == 0 {
            return None;
        }
        let p = GlobalLock(h) as *mut u8;
        if p.is_null() {
            GlobalFree(h);
            return None;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
        GlobalUnlock(h);
        Some(h)
    }
}

/// Copie un secret dans le presse-papiers en demandant à Windows de ne pas le garder dans l'historique (Win + V),
/// de ne pas le synchroniser avec le cloud et de ne pas le montrer aux outils de surveillance du presse-papiers.
pub fn copy_secret(text: &str) -> Result<(), String> {
    open_clipboard()?;
    let result = unsafe {
        EmptyClipboard();
        let utf16: Vec<u8> = wide(text).iter().flat_map(|u| u.to_le_bytes()).collect();
        let ok = match global_bytes(&utf16) {
            Some(h) => {
                if SetClipboardData(CF_UNICODETEXT, h) == 0 {
                    GlobalFree(h);
                    false
                } else {
                    true
                }
            }
            None => false,
        };
        for (name, value) in [("ExcludeClipboardContentFromMonitorProcessing", 1u32), ("CanIncludeInClipboardHistory", 0), ("CanUploadToCloudClipboard", 0)] {
            let fmt = RegisterClipboardFormatW(wide(name).as_ptr());
            if fmt != 0 {
                if let Some(h) = global_bytes(&value.to_le_bytes()) {
                    if SetClipboardData(fmt, h) == 0 {
                        GlobalFree(h);
                    }
                }
            }
        }
        ok
    };
    unsafe { CloseClipboard() };
    if result { Ok(()) } else { Err("Copie impossible dans le presse-papiers.".into()) }
}

/// Texte actuellement dans le presse-papiers.
fn read_clipboard() -> Option<String> {
    open_clipboard().ok()?;
    let text = unsafe {
        let h = GetClipboardData(CF_UNICODETEXT);
        if h == 0 {
            None
        } else {
            let p = GlobalLock(h) as *const u16;
            if p.is_null() {
                None
            } else {
                let mut len = 0;
                while *p.add(len) != 0 {
                    len += 1;
                }
                let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
                GlobalUnlock(h);
                Some(s)
            }
        }
    };
    unsafe { CloseClipboard() };
    text
}

/// Vide le presse-papiers, mais seulement s'il contient encore ce secret (l'utilisateur a pu copier autre chose entre-temps).
pub fn clear_clipboard_if(secret: &str) -> bool {
    if read_clipboard().as_deref() != Some(secret) {
        return false;
    }
    if open_clipboard().is_err() {
        return false;
    }
    unsafe {
        EmptyClipboard();
        CloseClipboard();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_struct_has_the_size_windows_expects() {
        // 40 octets en 64 bits : SendInput refuse toute autre taille.
        assert_eq!(std::mem::size_of::<Input>(), if cfg!(target_pointer_width = "64") { 40 } else { 28 });
    }

    /// Ouvre une vraie fenêtre de saisie et y tape du texte avec accents, euro et symboles : `cargo test types_unicode -- --ignored`.
    #[test]
    #[ignore]
    fn types_unicode_symbols_and_tab_into_a_real_window() {
        let out = std::env::temp_dir().join(format!("typed-{}.txt", std::process::id()));
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms; \
             $f = New-Object Windows.Forms.Form; $f.Text = 'LamaTypeTest'; $f.TopMost = $true; $f.Width = 500; $f.Height = 200; \
             $a = New-Object Windows.Forms.TextBox; $a.Width = 400; $a.Top = 10; $f.Controls.Add($a); \
             $b = New-Object Windows.Forms.TextBox; $b.Width = 400; $b.Top = 50; $f.Controls.Add($b); \
             $f.Add_Shown({{ $f.Activate(); $a.Focus() }}); \
             $t = New-Object Windows.Forms.Timer; $t.Interval = 6000; \
             $t.Add_Tick({{ Set-Content -Path '{}' -Value ($a.Text + '|' + $b.Text) -Encoding UTF8 -NoNewline; $f.Close() }}); $t.Start(); \
             [void]$f.ShowDialog()",
            out.display()
        );
        let mut child = std::process::Command::new("powershell").args(["-NoProfile", "-Command", &script]).spawn().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2500));
        assert_eq!(foreground_title(), "LamaTypeTest", "la fenêtre de test n'a pas pris le focus");
        type_pieces(&[
            Piece::Text(zeroize::Zeroizing::new("Lama-é€#+{}[]^%~ ü".into())),
            Piece::Tab,
            Piece::Text(zeroize::Zeroizing::new("Zéro-€-2".into())),
        ])
        .unwrap();
        let _ = child.wait();
        let typed = std::fs::read_to_string(&out).unwrap();
        let _ = std::fs::remove_file(&out);
        assert_eq!(typed.trim_start_matches('\u{feff}'), "Lama-é€#+{}[]^%~ ü|Zéro-€-2");
    }

    /// Touche au vrai presse-papiers de la session : `cargo test clipboard_roundtrip -- --ignored`.
    #[test]
    #[ignore]
    fn clipboard_roundtrip_and_conditional_clear() {
        copy_secret("Mot-de-passe-é€-1").unwrap();
        assert_eq!(read_clipboard().as_deref(), Some("Mot-de-passe-é€-1"));
        assert!(!clear_clipboard_if("autre chose"));
        assert_eq!(read_clipboard().as_deref(), Some("Mot-de-passe-é€-1"));
        assert!(clear_clipboard_if("Mot-de-passe-é€-1"));
        assert_eq!(read_clipboard(), None);
    }
}
