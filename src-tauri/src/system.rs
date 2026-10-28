//! Outils « ordinateur » : heure et informations système, ouverture de sites/fichiers/applications, presse-papiers,
//! réglages (volume, thème…), capture d'écran, souris et clavier, recherche de fichiers.
//! Chaque action qui change quelque chose est approuvée par l'utilisateur dans la boucle de l'agent.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

const NO_WINDOW: u32 = 0x0800_0000;

static ENABLED: AtomicBool = AtomicBool::new(false);
/// Dernière capture envoyée au modèle, pour afficher où il va cliquer avant que l'utilisateur accepte.
static LAST_SHOT: std::sync::Mutex<Option<Shot>> = std::sync::Mutex::new(None);

#[derive(Clone, serde::Serialize)]
pub struct Shot {
    /// PNG en base64.
    pub data: String,
    /// Dimensions de l'image reçue par le modèle (celles des coordonnées qu'il donne).
    pub width: f64,
    pub height: f64,
}

pub fn last_shot() -> Option<Shot> {
    LAST_SHOT.lock().ok().and_then(|s| s.clone())
}

/// Rapport (taille de la capture / taille réelle de l'écran) de la dernière capture : les coordonnées données par le
/// modèle sont dans l'image qu'il a vue.
static SCALE: AtomicU64 = AtomicU64::new(0);

/// Les outils « ordinateur » sont-ils activés pour le profil ouvert ?
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// Applications courantes que le modèle peut lancer par leur nom.
const APPS: &[(&str, &str)] = &[
    ("bloc-notes", "notepad.exe"),
    ("notepad", "notepad.exe"),
    ("calculatrice", "calc.exe"),
    ("calculator", "calc.exe"),
    ("paint", "mspaint.exe"),
    ("explorateur", "explorer.exe"),
    ("explorer", "explorer.exe"),
    ("gestionnaire des tâches", "taskmgr.exe"),
    ("task manager", "taskmgr.exe"),
    ("capture", "snippingtool.exe"),
    ("paramètres", "ms-settings:"),
    ("settings", "ms-settings:"),
];

/// Extensions qui exécuteraient du code : refusées (on ouvre des documents, pas des programmes).
const EXECUTABLE: &[&str] = &[
    "exe", "bat", "cmd", "com", "msi", "msp", "mst", "ps1", "psm1", "psd1", "ps1xml", "psc1", "vbs", "vbe", "vb", "js", "jse", "wsf", "wsh", "wsc", "ws", "sct", "scr", "lnk", "url",
    "reg", "hta", "jar", "jnlp", "dll", "ocx", "sys", "cpl", "msc", "pif", "gadget", "appref-ms", "application", "scf", "settingcontent-ms", "library-ms", "search-ms",
    "diagcab", "xll", "wll", "msix", "msixbundle", "appx", "appxbundle", "appinstaller", "chm", "inf", "py", "pyw", "pyz",
];

/// Extension telle que Windows la comprend : « virus.exe. » et « virus.exe␠ » sont ouverts comme « virus.exe ».
fn real_extension(path: &Path) -> String {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").trim_end_matches(['.', ' ']);
    name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
}

fn powershell(script: &str) -> Result<String, String> {
    let out = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(NO_WINDOW)
        .output()
        .map_err(|e| format!("PowerShell indisponible : {e}"))?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Exécute un script PowerShell écrit dans un fichier temporaire. Les paramètres passent par des variables
/// d'environnement `LAMA_*` : le texte du modèle n'est jamais interprété comme du code.
fn run_script(script: &str, envs: &[(&str, String)]) -> Result<String, String> {
    let path = std::env::temp_dir().join(format!("lama-{}.ps1", uuid::Uuid::new_v4()));
    // BOM : PowerShell 5 lit sinon le fichier en ANSI.
    std::fs::write(&path, format!("\u{feff}$ErrorActionPreference='Stop'; [Console]::OutputEncoding=[Text.Encoding]::UTF8\n{script}")).map_err(|e| e.to_string())?;
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]).arg(&path).creation_flags(NO_WINDOW);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let out = cmd.output();
    let _ = std::fs::remove_file(&path);
    let out = out.map_err(|e| format!("PowerShell indisponible : {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let first = err.lines().find(|l| !l.trim().is_empty()).unwrap_or("échec");
        return Err(format!("Action impossible : {}", first.trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

const WIN32: &str = r#"
Add-Type -AssemblyName System.Windows.Forms,System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class W {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, int d, UIntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte sc, uint f, UIntPtr e);
}
"@
[W]::SetProcessDPIAware() | Out-Null
"#;

// ---------------------------------------------------------------- informations

/// Date, heure locale, fuseau et informations sur le PC. `section` : tout, disques, processus, batterie, applications.
pub fn info(section: &str) -> Result<String, String> {
    let script = match section {
        "disques" => "Get-PSDrive -PSProvider FileSystem | Where-Object { $_.Used -ne $null } | ForEach-Object { '{0}: {1:N1} Go libres sur {2:N1} Go' -f $_.Name, ($_.Free/1GB), (($_.Used+$_.Free)/1GB) }".to_string(),
        "processus" => "Get-Process | Sort-Object WorkingSet64 -Descending | Select-Object -First 15 | ForEach-Object { '{0} ({1} Mo)' -f $_.ProcessName, [int]($_.WorkingSet64/1MB) }".to_string(),
        "batterie" => "$b=Get-CimInstance Win32_Battery; if($b){ 'Batterie : ' + $b.EstimatedChargeRemaining + ' % (' + $(if($b.BatteryStatus -eq 2){'sur secteur'}else{'sur batterie'}) + ')' } else { 'Pas de batterie (PC fixe).' }".to_string(),
        "applications" => {
            let names: Vec<String> = start_menu_apps().into_iter().map(|(n, _)| n).collect();
            return Ok(if names.is_empty() { "Aucune application trouvée.".into() } else { names.join(", ") });
        }
        _ => "[Console]::OutputEncoding=[Text.Encoding]::UTF8; \
            $d=Get-Date; $fr=[cultureinfo]'fr-FR'; \
            'Date et heure locales : ' + $d.ToString('dddd d MMMM yyyy, HH:mm:ss',$fr); \
            'Fuseau horaire : ' + [TimeZoneInfo]::Local.DisplayName; \
            'Système : ' + (Get-CimInstance Win32_OperatingSystem).Caption; \
            'Utilisateur : ' + $env:USERNAME + ' sur ' + $env:COMPUTERNAME; \
            'Dossier personnel : ' + $env:USERPROFILE"
            .to_string(),
    };
    let text = powershell(&format!("[Console]::OutputEncoding=[Text.Encoding]::UTF8; {script}"))?;
    if text.is_empty() {
        return Err("Impossible de lire les informations système.".into());
    }
    Ok(text)
}

// ---------------------------------------------------------------- ouverture

/// Une URL web que l'on accepte d'ouvrir.
fn is_url(t: &str) -> bool {
    let l = t.to_ascii_lowercase();
    (l.starts_with("http://") || l.starts_with("https://") || l.starts_with("mailto:")) && !t.contains(char::is_whitespace)
}

/// Raccourcis du menu Démarrer : (nom affiché, chemin du .lnk).
fn start_menu_apps() -> Vec<(String, PathBuf)> {
    let mut roots = Vec::new();
    if let Ok(p) = std::env::var("ProgramData") {
        roots.push(PathBuf::from(p).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    if let Ok(p) = std::env::var("APPDATA") {
        roots.push(PathBuf::from(p).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    let mut out = Vec::new();
    for root in roots {
        for e in ignore::WalkBuilder::new(&root).standard_filters(false).max_depth(Some(4)).build().flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case("lnk")) {
                if let Some(name) = p.file_stem().and_then(|s| s.to_str()) {
                    if !name.to_lowercase().contains("uninstall") && !name.to_lowercase().contains("désinstall") {
                        out.push((name.to_string(), p.to_path_buf()));
                    }
                }
            }
        }
    }
    out.sort_by_key(|(n, _)| n.to_lowercase());
    out.dedup_by(|a, b| a.0.eq_ignore_ascii_case(&b.0));
    out
}

/// Raccourci du menu Démarrer le plus proche d'un nom : exact d'abord, sinon le plus court qui le contient.
fn find_app(apps: &[(String, PathBuf)], wanted: &str) -> Option<PathBuf> {
    let w = wanted.to_lowercase();
    if let Some((_, p)) = apps.iter().find(|(n, _)| n.to_lowercase() == w) {
        return Some(p.clone());
    }
    apps.iter().filter(|(n, _)| n.to_lowercase().contains(&w)).min_by_key(|(n, _)| n.len()).map(|(_, p)| p.clone())
}

/// Ouvre un site, un fichier/dossier existant ou une application avec le programme par défaut de Windows.
pub fn open(target: &str) -> Result<String, String> {
    let target = target.trim();
    if target.is_empty() {
        return Err("Cible vide.".into());
    }
    if is_url(target) {
        // `explorer` ouvre l'URL dans le navigateur par défaut, sans passer par un interpréteur de commandes.
        Command::new("explorer").arg(target).spawn().map_err(|e| format!("Ouverture impossible : {e}"))?;
        return Ok(format!("Ouvert dans le navigateur : {target}"));
    }
    if let Some((_, exe)) = APPS.iter().find(|(n, _)| n.eq_ignore_ascii_case(target)) {
        let mut c = if exe.ends_with(':') { Command::new("explorer") } else { Command::new(exe) };
        if exe.ends_with(':') {
            c.arg(exe);
        }
        c.spawn().map_err(|e| format!("Lancement impossible : {e}"))?;
        return Ok(format!("Application lancée : {target}"));
    }
    let path = Path::new(target);
    if path.exists() {
        let ext = real_extension(path);
        if path.is_file() && EXECUTABLE.contains(&ext.as_str()) {
            return Err(format!("Refusé : un fichier .{ext} exécuterait du code. Seuls les documents et dossiers peuvent être ouverts."));
        }
        Command::new("explorer").arg(path).spawn().map_err(|e| format!("Ouverture impossible : {e}"))?;
        return Ok(format!("Ouvert : {target}"));
    }
    if !target.contains(['\\', '/', ':']) {
        if let Some(lnk) = find_app(&start_menu_apps(), target) {
            Command::new("explorer").arg(&lnk).spawn().map_err(|e| format!("Lancement impossible : {e}"))?;
            let name = lnk.file_stem().and_then(|s| s.to_str()).unwrap_or(target);
            return Ok(format!("Application lancée : {name}"));
        }
    }
    Err(format!("Cible inconnue : « {target} ». Donne une URL http(s), un chemin existant ou le nom d'une application installée (voir system_info, section applications)."))
}

// ---------------------------------------------------------------- presse-papiers et réglages

pub fn clipboard(action: &str, text: Option<&str>) -> Result<String, String> {
    match action {
        "read" => {
            let t = run_script("Get-Clipboard -Raw", &[])?;
            Ok(if t.is_empty() { "(le presse-papiers est vide ou ne contient pas de texte)".into() } else { t })
        }
        "write" => {
            let text = text.ok_or("Argument manquant : text")?;
            run_script("Set-Clipboard -Value $env:LAMA_TEXT", &[("LAMA_TEXT", text.to_string())])?;
            Ok("Texte copié dans le presse-papiers.".into())
        }
        other => Err(format!("Action inconnue : {other} (read ou write)")),
    }
}

/// Réglages du PC : volume, sourdine, thème, luminosité, verrouillage.
pub fn control(action: &str, value: Option<i64>, on: Option<bool>) -> Result<String, String> {
    match action {
        "volume" => {
            let v = value.ok_or("Argument manquant : value (0 à 100)")?.clamp(0, 100);
            // Pas d'API simple pour un volume absolu : on descend au minimum puis on monte (2 % par appui).
            let script = format!(
                "{WIN32}\n1..50 | ForEach-Object {{ [W]::keybd_event(0xAE,0,0,[UIntPtr]::Zero); [W]::keybd_event(0xAE,0,2,[UIntPtr]::Zero) }}\n\
                 1..{} | ForEach-Object {{ [W]::keybd_event(0xAF,0,0,[UIntPtr]::Zero); [W]::keybd_event(0xAF,0,2,[UIntPtr]::Zero) }}",
                v / 2
            );
            run_script(&script, &[])?;
            Ok(format!("Volume réglé à environ {v} %."))
        }
        "mute" => {
            run_script(&format!("{WIN32}\n[W]::keybd_event(0xAD,0,0,[UIntPtr]::Zero); [W]::keybd_event(0xAD,0,2,[UIntPtr]::Zero)"), &[])?;
            Ok("Son coupé ou rétabli (bascule).".into())
        }
        "dark_mode" => {
            let dark = on.ok_or("Argument manquant : on (true = thème sombre)")?;
            let n = if dark { 0 } else { 1 };
            let script = format!(
                "$k='HKCU:\\Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize'; Set-ItemProperty $k AppsUseLightTheme {n}; Set-ItemProperty $k SystemUsesLightTheme {n}"
            );
            run_script(&script, &[])?;
            Ok(format!("Thème Windows {}.", if dark { "sombre" } else { "clair" }))
        }
        "brightness" => {
            let v = value.ok_or("Argument manquant : value (0 à 100)")?.clamp(0, 100);
            let script = format!(
                "Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightnessMethods | Invoke-CimMethod -MethodName WmiSetBrightness -Arguments @{{Timeout=1;Brightness=[byte]{v}}} | Out-Null"
            );
            run_script(&script, &[]).map_err(|_| "Luminosité non réglable sur cet écran (réservé aux écrans d'ordinateur portable).".to_string())?;
            Ok(format!("Luminosité réglée à {v} %."))
        }
        "lock" => {
            Command::new("rundll32.exe").args(["user32.dll,LockWorkStation"]).creation_flags(NO_WINDOW).spawn().map_err(|e| e.to_string())?;
            Ok("Session verrouillée.".into())
        }
        other => Err(format!("Action inconnue : {other} (volume, mute, dark_mode, brightness, lock)")),
    }
}

// ---------------------------------------------------------------- écran, souris, clavier

/// Capture de l'écran principal, réduite à 1280 px de large au plus. Renvoie (PNG en base64, texte descriptif).
pub fn screenshot() -> Result<(String, String), String> {
    use base64::Engine;
    let out = std::env::temp_dir().join(format!("lama-shot-{}.png", uuid::Uuid::new_v4()));
    let script = format!(
        "{WIN32}\n$b=[System.Windows.Forms.Screen]::PrimaryScreen.Bounds\n\
         $bmp=New-Object System.Drawing.Bitmap $b.Width,$b.Height\n\
         $g=[System.Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($b.Location,[System.Drawing.Point]::Empty,$b.Size)\n\
         $s=[Math]::Min(1.0,1280.0/$b.Width); $w=[int]($b.Width*$s); $h=[int]($b.Height*$s)\n\
         $o=New-Object System.Drawing.Bitmap $bmp,$w,$h\n\
         $o.Save($env:LAMA_OUT,[System.Drawing.Imaging.ImageFormat]::Png)\n\
         \"$($b.Width) $($b.Height) $w $h\""
    );
    let res = run_script(&script, &[("LAMA_OUT", out.to_string_lossy().into_owned())]);
    let bytes = std::fs::read(&out);
    let _ = std::fs::remove_file(&out);
    let dims: Vec<f64> = res?.split_whitespace().filter_map(|x| x.parse().ok()).collect();
    let bytes = bytes.map_err(|e| format!("Capture impossible : {e}"))?;
    if dims.len() != 4 {
        return Err("Capture impossible : réponse inattendue.".into());
    }
    SCALE.store((dims[2] / dims[0]).to_bits(), Ordering::Relaxed);
    let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
    if let Ok(mut last) = LAST_SHOT.lock() {
        *last = Some(Shot { data: data.clone(), width: dims[2], height: dims[3] });
    }
    let text = format!(
        "Capture de l'écran ({}×{} px, image reçue en {}×{}). Les coordonnées des actions sur la souris se donnent dans cette image, (0,0) en haut à gauche. Nécessite un modèle avec vision pour la voir.",
        dims[0], dims[1], dims[2], dims[3]
    );
    Ok((data, text))
}

fn scale() -> f64 {
    let s = f64::from_bits(SCALE.load(Ordering::Relaxed));
    if s > 0.0 && s <= 1.0 { s } else { 1.0 }
}

/// Souris et clavier. `x`, `y` : coordonnées dans la dernière capture.
pub fn computer(action: &str, x: Option<f64>, y: Option<f64>, text: Option<&str>, keys: Option<&str>, amount: Option<i64>) -> Result<String, String> {
    let mut envs: Vec<(&str, String)> = vec![("LAMA_ACTION", action.to_string())];
    let needs_xy = matches!(action, "click" | "double_click" | "right_click" | "move");
    if needs_xy {
        let (x, y) = (x.ok_or("Argument manquant : x")?, y.ok_or("Argument manquant : y")?);
        let s = scale();
        envs.push(("LAMA_X", ((x / s).round() as i64).to_string()));
        envs.push(("LAMA_Y", ((y / s).round() as i64).to_string()));
    }
    match action {
        "click" | "double_click" | "right_click" | "move" => {}
        "scroll" => {
            if let (Some(x), Some(y)) = (x, y) {
                let s = scale();
                envs.push(("LAMA_X", ((x / s).round() as i64).to_string()));
                envs.push(("LAMA_Y", ((y / s).round() as i64).to_string()));
            }
            // Positif = vers le haut ; une « encoche » = 120.
            envs.push(("LAMA_AMOUNT", (amount.unwrap_or(-3).clamp(-30, 30) * 120).to_string()));
        }
        "type" => envs.push(("LAMA_TEXT", text.ok_or("Argument manquant : text")?.to_string())),
        "key" => envs.push(("LAMA_KEYS", keys.ok_or("Argument manquant : keys (ex. ctrl+c)")?.to_string())),
        other => return Err(format!("Action inconnue : {other} (click, double_click, right_click, move, scroll, type, key)")),
    }
    let script = format!(
        r#"{WIN32}
$a=$env:LAMA_ACTION
function Press($d,$u) {{ [W]::mouse_event($d,0,0,0,[UIntPtr]::Zero); [W]::mouse_event($u,0,0,0,[UIntPtr]::Zero) }}
if ($env:LAMA_X) {{ [W]::SetCursorPos([int]$env:LAMA_X,[int]$env:LAMA_Y) | Out-Null; Start-Sleep -Milliseconds 80 }}
switch ($a) {{
  'click' {{ Press 2 4 }}
  'right_click' {{ Press 8 16 }}
  'double_click' {{ Press 2 4; Start-Sleep -Milliseconds 60; Press 2 4 }}
  'scroll' {{ [W]::mouse_event(0x800,0,0,[int]$env:LAMA_AMOUNT,[UIntPtr]::Zero) }}
  'type' {{
    $t = $env:LAMA_TEXT -replace '([+^%~(){{}}\[\]])','{{$1}}' -replace "`r?`n",'{{ENTER}}'
    [System.Windows.Forms.SendKeys]::SendWait($t)
  }}
  'key' {{
    $map=@{{enter=0x0D;tab=9;esc=0x1B;escape=0x1B;space=0x20;backspace=8;delete=0x2E;up=0x26;down=0x28;left=0x25;right=0x27;home=0x24;end=0x23;pageup=0x21;pagedown=0x22;ctrl=0x11;alt=0x12;shift=0x10;win=0x5B}}
    1..12 | ForEach-Object {{ $map["f$_"]=0x6F+$_ }}
    $codes=@()
    foreach ($p in ($env:LAMA_KEYS.ToLower().Split('+') | ForEach-Object {{ $_.Trim() }})) {{
      if ($map.ContainsKey($p)) {{ $codes += [int]$map[$p] }}
      elseif ($p.Length -eq 1) {{ $codes += [int][char]$p.ToUpper() }}
      else {{ throw "Touche inconnue : $p" }}
    }}
    foreach ($c in $codes) {{ [W]::keybd_event([byte]$c,0,0,[UIntPtr]::Zero) }}
    [array]::Reverse($codes)
    foreach ($c in $codes) {{ [W]::keybd_event([byte]$c,0,2,[UIntPtr]::Zero) }}
  }}
}}"#
    );
    run_script(&script, &envs)?;
    Ok(match action {
        "click" => "Clic effectué.".into(),
        "double_click" => "Double-clic effectué.".into(),
        "right_click" => "Clic droit effectué.".into(),
        "move" => "Souris déplacée.".into(),
        "scroll" => "Défilement effectué.".into(),
        "type" => "Texte saisi.".into(),
        _ => "Touches envoyées.".into(),
    })
}

// ---------------------------------------------------------------- recherche de fichiers

/// Cherche des fichiers ou dossiers dont le nom contient `query`, dans les dossiers donnés.
pub fn find_files(roots: &[PathBuf], query: &str) -> Result<String, String> {
    const MAX: usize = 60;
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Err("Recherche vide.".into());
    }
    if roots.is_empty() {
        return Err("Aucun dossier à explorer : ajoute des dossiers autorisés dans Réglages → Ordinateur.".into());
    }
    let started = std::time::Instant::now();
    let mut found = Vec::new();
    'outer: for root in roots {
        for e in ignore::WalkBuilder::new(root).standard_filters(false).max_depth(Some(14)).build().flatten() {
            if started.elapsed().as_secs() > 15 {
                break 'outer;
            }
            let name = e.file_name().to_string_lossy().to_lowercase();
            if name.contains(&q) {
                found.push(e.path().display().to_string());
                if found.len() >= MAX {
                    break 'outer;
                }
            }
        }
    }
    Ok(if found.is_empty() { format!("Aucun résultat pour « {query} ».") } else { found.join("\n") })
}

#[cfg(test)]
mod tests {
    #[test]
    fn trailing_dots_and_spaces_do_not_hide_an_executable() {
        for (name, ext) in [("a/virus.exe.", "exe"), ("a/virus.exe ", "exe"), ("a/x.EXE", "exe"), ("a/note.txt", "txt"), ("a/sans-extension", ""), ("a/raccourci.url", "url")] {
            assert_eq!(super::real_extension(std::path::Path::new(name)), ext, "{name}");
        }
        for ext in ["exe", "url", "msc", "settingcontent-ms", "hta", "ps1"] {
            assert!(super::EXECUTABLE.contains(&ext), "{ext}");
        }
    }

    use super::*;

    #[test]
    fn urls_and_executables() {
        assert!(is_url("https://exemple.fr/page?q=1"));
        assert!(!is_url("https://a b"));
        assert!(!is_url("file:///C:/x"));
        assert!(!is_url("javascript:alert(1)"));
        let dir = std::env::temp_dir().join("lama-open-test");
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("x.bat");
        std::fs::write(&exe, "echo hi").unwrap();
        assert!(open(exe.to_str().unwrap()).unwrap_err().contains("Refusé"));
        assert!(open("n'importe/quoi").unwrap_err().contains("inconnue"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn info_sections() {
        assert!(info("tout").unwrap().contains("Date et heure locales"));
        assert!(info("disques").unwrap().contains("Go"));
        assert!(info("batterie").unwrap().to_lowercase().contains("batterie"));
    }

    #[test]
    fn app_matching_prefers_exact_then_shortest() {
        let apps = vec![
            ("Visual Studio Code".to_string(), PathBuf::from("a")),
            ("Code".to_string(), PathBuf::from("b")),
            ("Google Chrome".to_string(), PathBuf::from("c")),
        ];
        assert_eq!(find_app(&apps, "code"), Some(PathBuf::from("b")));
        assert_eq!(find_app(&apps, "chrome"), Some(PathBuf::from("c")));
        assert_eq!(find_app(&apps, "inexistant"), None);
    }

    #[test]
    #[ignore = "écrase le presse-papiers"]
    fn clipboard_roundtrip_with_special_chars() {
        let text = "Bonjour « é » 'a' \"b\" $x `y`";
        clipboard("write", Some(text)).unwrap();
        assert_eq!(clipboard("read", None).unwrap(), text);
    }

    #[test]
    #[ignore = "capture l'écran"]
    fn screenshot_has_png_and_scale() {
        let (b64, desc) = screenshot().unwrap();
        assert!(b64.starts_with("iVBOR"), "pas un PNG");
        assert!(desc.contains("Capture"));
        assert!(scale() > 0.0 && scale() <= 1.0);
    }

    #[test]
    fn find_files_in_roots() {
        let d = std::env::temp_dir().join(format!("lama-find-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::write(d.join("sub").join("Rapport-final.txt"), "x").unwrap();
        let r = find_files(&[d.clone()], "rapport").unwrap();
        assert!(r.contains("Rapport-final.txt"));
        assert!(find_files(&[d.clone()], "zzz").unwrap().contains("Aucun"));
        assert!(find_files(&[], "x").is_err());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn dark_mode_and_bad_actions_validate() {
        assert!(control("volume", None, None).is_err());
        assert!(control("explode", None, None).is_err());
        assert!(computer("click", None, None, None, None, None).is_err());
        assert!(computer("fly", None, None, None, None, None).is_err());
        assert!(computer("key", None, None, None, Some("ctrl+nimportequoi"), None).is_err());
    }
}
