//! Ce que le PC peut faire tourner : mémoire, carte graphique, processeur et espace disque.
//! Sert à recommander des modèles locaux adaptés ; rien n'est envoyé hors de la machine.

use serde::{Deserialize, Serialize};
use std::os::windows::process::CommandExt;
use std::process::Command;

const NO_WINDOW: u32 = 0x0800_0000;
const MB: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Vendor {
    Nvidia,
    Amd,
    Intel,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gpu {
    pub name: String,
    pub vendor: Vendor,
    /// Mémoire dédiée de la carte, en Mo (0 si inconnue).
    pub vram_mb: u64,
    /// Carte intégrée : elle se sert de la mémoire du PC, sa « VRAM » ne compte pas.
    pub integrated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hardware {
    pub ram_mb: u64,
    pub ram_free_mb: u64,
    pub cpu: String,
    pub cores: u32,
    pub avx2: bool,
    pub gpus: Vec<Gpu>,
    pub disk_free_mb: u64,
}

impl Hardware {
    /// La meilleure carte pour un modèle : celle qui a le plus de mémoire dédiée.
    pub fn best_gpu(&self) -> Option<&Gpu> {
        self.gpus.iter().filter(|g| !g.integrated).max_by_key(|g| g.vram_mb)
    }
}

#[repr(C)]
struct MemoryStatusEx {
    length: u32,
    load: u32,
    total_phys: u64,
    avail_phys: u64,
    total_page: u64,
    avail_page: u64,
    total_virtual: u64,
    avail_virtual: u64,
    avail_extended: u64,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GlobalMemoryStatusEx(status: *mut MemoryStatusEx) -> i32;
    fn GetDiskFreeSpaceExW(dir: *const u16, avail: *mut u64, total: *mut u64, free: *mut u64) -> i32;
}

fn memory() -> (u64, u64) {
    let mut s = MemoryStatusEx { length: std::mem::size_of::<MemoryStatusEx>() as u32, load: 0, total_phys: 0, avail_phys: 0, total_page: 0, avail_page: 0, total_virtual: 0, avail_virtual: 0, avail_extended: 0 };
    // SAFETY: `s` est une structure MEMORYSTATUSEX valide dont `length` est renseigné.
    let ok = unsafe { GlobalMemoryStatusEx(&mut s) } != 0;
    if ok { (s.total_phys / MB, s.avail_phys / MB) } else { (0, 0) }
}

/// Espace libre (Mo) du disque qui contient `dir`.
fn disk_free(dir: &str) -> u64 {
    let wide: Vec<u16> = dir.encode_utf16().chain(std::iter::once(0)).collect();
    let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
    // SAFETY: `wide` se termine par un zéro et les trois sorties sont des u64 valides.
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut avail, &mut total, &mut free) } != 0;
    if ok { avail / MB } else { 0 }
}

/// Espace libre (Mo) du disque qui contient `dir` ; le dossier doit exister.
pub fn free_mb(dir: &std::path::Path) -> u64 {
    disk_free(&dir.to_string_lossy())
}

pub fn vendor_of(name: &str) -> Vendor {
    let n = name.to_lowercase();
    if n.contains("nvidia") || n.contains("geforce") || n.contains("quadro") || n.contains("rtx") {
        Vendor::Nvidia
    } else if n.contains("amd") || n.contains("radeon") || n.contains("ati ") {
        Vendor::Amd
    } else if n.contains("intel") {
        Vendor::Intel
    } else {
        Vendor::Other
    }
}

/// Cartes virtuelles (bureau à distance, machine virtuelle, pilote de base) : inutiles pour un modèle.
fn is_virtual(name: &str) -> bool {
    let n = name.to_lowercase();
    ["microsoft basic", "microsoft remote", "remote display", "virtual", "vmware", "virtualbox", "parsec", "indirect display", "displaylink"].iter().any(|w| n.contains(w))
}

/// Une carte AMD « Radeon(TM) Graphics » sans nom de série, ou toute Intel, est intégrée ; les cartes AMD/NVIDIA avec peu de
/// mémoire dédiée aussi (elles empruntent la RAM).
fn is_integrated(name: &str, vendor: Vendor, vram_mb: u64) -> bool {
    let n = name.to_lowercase();
    match vendor {
        Vendor::Intel => !n.contains("arc"),
        Vendor::Nvidia => false,
        Vendor::Amd => vram_mb < 2048 || n.trim_end().ends_with("radeon(tm) graphics") || n.trim_end().ends_with("radeon graphics"),
        Vendor::Other => vram_mb < 2048,
    }
}

/// Sortie du script PowerShell : des lignes `CPU|nom` et `GPU|nom|octets`.
pub fn parse_probe(out: &str) -> (String, Vec<Gpu>) {
    let mut cpu = String::new();
    let mut gpus: Vec<Gpu> = Vec::new();
    for line in out.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix("CPU|") {
            cpu = name.trim().to_string();
        } else if let Some(rest) = line.strip_prefix("GPU|") {
            let (name, bytes) = rest.rsplit_once('|').unwrap_or((rest, "0"));
            let name = name.trim().to_string();
            if name.is_empty() || is_virtual(&name) {
                continue;
            }
            let vram_mb = bytes.trim().parse::<u64>().unwrap_or(0) / MB;
            let vendor = vendor_of(&name);
            let g = Gpu { integrated: is_integrated(&name, vendor, vram_mb), name, vendor, vram_mb };
            if !gpus.iter().any(|x| x.name == g.name && x.vram_mb == g.vram_mb) {
                gpus.push(g);
            }
        }
    }
    (cpu, gpus)
}

/// `nvidia-smi --query-gpu=name,memory.total --format=csv,noheader,nounits` : la mémoire exacte des cartes NVIDIA
/// (le registre peut plafonner à 4 Go selon les pilotes).
pub fn parse_nvidia(out: &str) -> Vec<(String, u64)> {
    out.lines()
        .filter_map(|l| {
            let (name, mb) = l.rsplit_once(',')?;
            Some((name.trim().to_string(), mb.trim().parse::<u64>().ok()?))
        })
        .collect()
}

fn run(cmd: &mut Command) -> Option<String> {
    let out = cmd.creation_flags(NO_WINDOW).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
}

/// Le processeur et les cartes graphiques, lus dans le registre des pilotes d'affichage (qwMemorySize : mémoire dédiée sur 64 bits).
const PROBE: &str = "$ErrorActionPreference='SilentlyContinue'; \
    'CPU|' + (Get-CimInstance Win32_Processor | Select-Object -First 1 -ExpandProperty Name); \
    Get-ItemProperty 'HKLM:\\SYSTEM\\CurrentControlSet\\Control\\Class\\{4d36e968-e325-11ce-bfc1-08002be10318}\\0*' | ForEach-Object { \
      $m = $_.'HardwareInformation.qwMemorySize'; if ($m -is [byte[]]) { $m = [BitConverter]::ToUInt64($m, 0) }; \
      if ($_.DriverDesc) { 'GPU|' + $_.DriverDesc + '|' + [uint64]$m } }";

/// Lit la machine. Peut durer une ou deux secondes : à appeler hors du fil de l'interface.
pub fn detect() -> Hardware {
    let (ram_mb, ram_free_mb) = memory();
    let probe = run(Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", PROBE])).unwrap_or_default();
    let (cpu, mut gpus) = parse_probe(&probe);
    if gpus.iter().any(|g| g.vendor == Vendor::Nvidia) {
        let smi = run(Command::new("nvidia-smi").args(["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"])).unwrap_or_default();
        let exact = parse_nvidia(&smi);
        for g in gpus.iter_mut().filter(|g| g.vendor == Vendor::Nvidia) {
            if let Some((_, mb)) = exact.iter().find(|(n, _)| g.name.contains(n.as_str()) || n.contains(g.name.as_str())) {
                g.vram_mb = *mb;
            }
        }
    }
    gpus.sort_by(|a, b| a.integrated.cmp(&b.integrated).then(b.vram_mb.cmp(&a.vram_mb)));
    let dir = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| "C:\\".into());
    Hardware {
        ram_mb,
        ram_free_mb,
        cpu: if cpu.is_empty() { "Processeur inconnu".into() } else { cpu },
        cores: std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1),
        avx2: std::is_x86_feature_detected!("avx2"),
        gpus,
        disk_free_mb: disk_free(&dir),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_desktop_with_a_dedicated_card_and_an_integrated_one() {
        let out = "CPU|AMD Ryzen 7 5800X 8-Core Processor\nGPU|NVIDIA GeForce RTX 3070|4293918720\nGPU|AMD Radeon(TM) Graphics|536870912\nGPU|Microsoft Basic Display Adapter|0\n";
        let (cpu, gpus) = parse_probe(out);
        assert_eq!(cpu, "AMD Ryzen 7 5800X 8-Core Processor");
        assert_eq!(gpus.len(), 2, "la carte virtuelle est ignorée");
        let nv = gpus.iter().find(|g| g.vendor == Vendor::Nvidia).unwrap();
        assert!(!nv.integrated);
        assert_eq!(nv.vram_mb, 4095, "le registre plafonne à 4 Go : nvidia-smi corrige ensuite");
        assert!(gpus.iter().find(|g| g.vendor == Vendor::Amd).unwrap().integrated);
    }

    #[test]
    fn nvidia_smi_gives_the_real_memory() {
        let smi = "NVIDIA GeForce RTX 3070, 8192\n";
        assert_eq!(parse_nvidia(smi), vec![("NVIDIA GeForce RTX 3070".to_string(), 8192)]);
        assert!(parse_nvidia("").is_empty());
        assert!(parse_nvidia("No devices were found").is_empty());
    }

    #[test]
    fn classifies_cards() {
        assert_eq!(vendor_of("Intel(R) UHD Graphics 630"), Vendor::Intel);
        assert!(is_integrated("Intel(R) UHD Graphics 630", Vendor::Intel, 128));
        assert!(!is_integrated("Intel(R) Arc(TM) A770 Graphics", Vendor::Intel, 16384));
        assert!(is_integrated("AMD Radeon(TM) Graphics", Vendor::Amd, 4096), "APU : même avec 4 Go réservés");
        assert!(!is_integrated("AMD Radeon RX 6800 XT", Vendor::Amd, 16384));
        assert!(!is_integrated("NVIDIA GeForce GTX 1050", Vendor::Nvidia, 2048));
    }

    #[test]
    fn ignores_garbage_lines() {
        let (cpu, gpus) = parse_probe("bruit\nGPU||0\nGPU|Carte inconnue|pas-un-nombre\n");
        assert_eq!(cpu, "");
        assert_eq!(gpus.len(), 1);
        assert_eq!(gpus[0].vram_mb, 0);
    }

    #[test]
    fn best_gpu_ignores_integrated_cards() {
        let hw = Hardware {
            ram_mb: 16000,
            ram_free_mb: 8000,
            cpu: "x".into(),
            cores: 8,
            avx2: true,
            gpus: vec![
                Gpu { name: "Intel UHD".into(), vendor: Vendor::Intel, vram_mb: 128, integrated: true },
                Gpu { name: "RTX".into(), vendor: Vendor::Nvidia, vram_mb: 8192, integrated: false },
            ],
            disk_free_mb: 1000,
        };
        assert_eq!(hw.best_gpu().unwrap().name, "RTX");
    }

    /// Sur la vraie machine : `cargo test --lib reads_this_pc -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn reads_this_pc() {
        let hw = detect();
        println!("{hw:#?}");
        assert!(hw.ram_mb > 0 && hw.cores > 0 && hw.disk_free_mb > 0);
    }
}
