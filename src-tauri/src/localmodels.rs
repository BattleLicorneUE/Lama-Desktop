//! Catalogue des modèles locaux et verdict pour un PC donné : lequel tient dans la carte graphique, lequel passe par la
//! mémoire du PC, lequel est trop lourd. Les tailles sont celles des fichiers quantifiés (Q4) ; ce sont des ordres de
//! grandeur, pas des garanties de vitesse.

use crate::hardware::Hardware;
use serde::{Deserialize, Serialize};

/// Le contexte (~8 000 mots), les tampons et le moteur en plus des poids du modèle.
const OVERHEAD_MB: u64 = 1500;
/// Ce que Windows et les autres applications gardent pour eux.
const OS_RESERVE_MB: u64 = 4096;
/// Marge sur le disque pour le téléchargement.
const DISK_MARGIN_MB: u64 = 2048;
/// Sous cette taille de mémoire graphique, la carte n'aide pas à grand-chose.
const MIN_USEFUL_VRAM_MB: u64 = 1500;
/// Un modèle qui travaille sur le processeur reste agréable jusqu'à cette quantité de poids lus par mot.
const CPU_COMFORT_MB: u64 = 2600;
const CPU_LIMIT_MB: u64 = 6000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: String,
    /// Milliards de paramètres.
    pub params_b: f64,
    /// Pour les modèles « à experts » : paramètres actifs à chaque mot (ils fixent la vitesse).
    #[serde(default)]
    pub active_b: Option<f64>,
    pub size_mb: u64,
    pub uses: Vec<String>,
    /// Sait appeler les outils de l'agent.
    pub tools: bool,
    pub vision: bool,
    pub blurb: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Recommended,
    Possible,
    Heavy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Tout dans la carte graphique.
    Gpu,
    /// Une partie dans la carte graphique, le reste dans la mémoire du PC.
    Mixed,
    /// Sur le processeur.
    Cpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Speed {
    Fast,
    Medium,
    Slow,
}

#[derive(Debug, Clone, Serialize)]
pub struct Verdict {
    pub level: Level,
    pub mode: Option<Mode>,
    pub speed: Option<Speed>,
    pub reason: String,
    /// Assez de place sur le disque pour le télécharger.
    pub disk_ok: bool,
    /// Mémoire nécessaire en Mo (poids + contexte).
    pub need_mb: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Assessed {
    #[serde(flatten)]
    pub model: Model,
    #[serde(flatten)]
    pub verdict: Verdict,
    /// Le choix que je conseille pour ce PC.
    pub best: bool,
}

pub fn catalog() -> Vec<Model> {
    serde_json::from_str(include_str!("../local-models.json")).expect("local-models.json est valide")
}

fn gb(mb: u64) -> String {
    format!("{:.1}", mb as f64 / 1024.0).replace('.', ",")
}

pub fn assess(model: &Model, hw: &Hardware) -> Verdict {
    let need = model.size_mb + OVERHEAD_MB;
    let vram = hw.best_gpu().map(|g| g.vram_mb).unwrap_or(0);
    let ram = hw.ram_mb.saturating_sub(OS_RESERVE_MB);
    let disk_ok = hw.disk_free_mb >= model.size_mb + DISK_MARGIN_MB;
    // Poids réellement lus à chaque mot : tout le modèle, ou seulement les experts actifs.
    let per_token_mb = match model.active_b {
        Some(a) if model.params_b > 0.0 => (model.size_mb as f64 * a / model.params_b) as u64,
        _ => model.size_mb,
    };
    let v = |level, mode, speed, reason: String| Verdict { level, mode: Some(mode), speed: Some(speed), reason, disk_ok, need_mb: need };

    if vram >= need {
        return v(Level::Recommended, Mode::Gpu, Speed::Fast, format!("Tient entièrement dans ta carte graphique ({} Go) : rapide.", gb(vram)));
    }
    if vram >= MIN_USEFUL_VRAM_MB && need <= vram + ram {
        let speed = if model.active_b.is_some() || vram * 2 >= need { Speed::Medium } else { Speed::Slow };
        return v(Level::Possible, Mode::Mixed, speed, format!("Il faut {} Go : une partie dans la carte graphique, le reste dans la mémoire du PC. {}", gb(need), if speed == Speed::Medium { "Vitesse correcte." } else { "Plutôt lent." }));
    }
    if need <= ram {
        if !hw.avx2 {
            return Verdict { level: Level::Heavy, mode: None, speed: None, reason: "Ton processeur n'a pas AVX2 : les modèles seraient beaucoup trop lents.".into(), disk_ok, need_mb: need };
        }
        if per_token_mb <= CPU_COMFORT_MB {
            return v(Level::Recommended, Mode::Cpu, Speed::Medium, "Tourne sur le processeur à une vitesse correcte.".into());
        }
        if per_token_mb <= CPU_LIMIT_MB {
            return v(Level::Possible, Mode::Cpu, Speed::Slow, "Tourne sur le processeur, mais lentement.".into());
        }
        return Verdict { level: Level::Heavy, mode: None, speed: None, reason: "Il tiendrait en mémoire mais serait beaucoup trop lent sans carte graphique assez puissante.".into(), disk_ok, need_mb: need };
    }
    let offer = ram + if vram >= MIN_USEFUL_VRAM_MB { vram } else { 0 };
    Verdict { level: Level::Heavy, mode: None, speed: None, reason: format!("Il faut environ {} Go de mémoire ; ton PC peut en offrir environ {}.", gb(need), gb(offer)), disk_ok, need_mb: need }
}

/// Tout le catalogue, jugé pour ce PC : les recommandés d'abord (le plus capable en tête), puis les possibles, puis les trop lourds.
pub fn assess_all(hw: &Hardware) -> Vec<Assessed> {
    let mut all: Vec<Assessed> = catalog().into_iter().map(|m| Assessed { verdict: assess(&m, hw), model: m, best: false }).collect();
    let rank = |l: Level| match l {
        Level::Recommended => 0,
        Level::Possible => 1,
        Level::Heavy => 2,
    };
    all.sort_by(|a, b| rank(a.verdict.level).cmp(&rank(b.verdict.level)).then(b.model.size_mb.cmp(&a.model.size_mb)));
    // Le conseillé : le plus capable des recommandés qui sait aussi utiliser les outils de l'agent, sinon le plus capable tout court.
    let pick = all.iter().position(|a| a.verdict.level == Level::Recommended && a.model.tools && a.verdict.disk_ok).or_else(|| all.iter().position(|a| a.verdict.level == Level::Recommended && a.verdict.disk_ok));
    if let Some(i) = pick {
        all[i].best = true;
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::{Gpu, Vendor};

    fn pc(ram_mb: u64, vram_mb: u64, disk_free_mb: u64) -> Hardware {
        Hardware {
            ram_mb,
            ram_free_mb: ram_mb / 2,
            cpu: "test".into(),
            cores: 8,
            avx2: true,
            gpus: if vram_mb > 0 { vec![Gpu { name: "GPU".into(), vendor: Vendor::Nvidia, vram_mb, integrated: false }] } else { vec![] },
            disk_free_mb,
        }
    }

    fn level_of(hw: &Hardware, id: &str) -> Level {
        assess_all(hw).into_iter().find(|a| a.model.id == id).unwrap().verdict.level
    }

    #[test]
    fn the_catalogue_is_sane() {
        let c = catalog();
        assert!(c.len() >= 6);
        let mut ids: Vec<&str> = c.iter().map(|m| m.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), c.len(), "identifiants uniques");
        for m in &c {
            assert!(m.size_mb > 300 && m.params_b > 0.0 && !m.uses.is_empty() && !m.blurb.is_empty(), "{}", m.id);
            // Q4 : environ 0,5 à 0,75 Go par milliard de paramètres.
            let per_b = m.size_mb as f64 / 1024.0 / m.params_b;
            assert!((0.45..0.85).contains(&per_b), "{} : taille incohérente ({per_b:.2} Go/Md)", m.id);
        }
    }

    /// Le PC de Samuel : 24 Go de RAM, carte de 8 Go.
    #[test]
    fn an_eight_gb_card_runs_an_8b_model_fast_and_bigger_ones_mixed() {
        let hw = pc(24260, 8188, 69000);
        let all = assess_all(&hw);
        let find = |id: &str| all.iter().find(|a| a.model.id == id).unwrap();
        assert_eq!((find("qwen3-8b").verdict.level, find("qwen3-8b").verdict.mode), (Level::Recommended, Some(Mode::Gpu)));
        assert_eq!(find("qwen3-14b").verdict.mode, Some(Mode::Mixed));
        assert_eq!(find("qwen3-14b").verdict.level, Level::Possible);
        assert_eq!(find("qwen3-30b-a3b").verdict.speed, Some(Speed::Medium), "les modèles à experts restent vifs en mode mixte");
        assert!(find("qwen3-8b").best || find("qwen3-4b").best || all.iter().any(|a| a.best), "un choix conseillé existe");
        let best = all.iter().find(|a| a.best).unwrap();
        assert!(best.model.tools && best.verdict.level == Level::Recommended);
        assert_eq!(best.model.id, "qwen3-8b", "le plus capable des recommandés avec outils");
    }

    #[test]
    fn without_a_graphics_card_only_small_models_are_recommended() {
        let hw = pc(16384, 0, 50000);
        assert_eq!(level_of(&hw, "qwen3-1.7b"), Level::Recommended);
        assert_eq!(level_of(&hw, "qwen3-4b"), Level::Recommended, "2,5 Go de poids : correct sur processeur");
        assert_eq!(level_of(&hw, "gemma3-4b"), Level::Possible, "3,3 Go de poids : lent sur processeur");
        assert_eq!(level_of(&hw, "qwen3-8b"), Level::Possible);
        assert_eq!(level_of(&hw, "qwen3-14b"), Level::Heavy, "tiendrait en mémoire mais trop lent");
        assert_eq!(level_of(&hw, "qwen3-30b-a3b"), Level::Heavy, "20 Go > 12 Go disponibles");
    }

    #[test]
    fn a_moe_model_is_comfortable_on_a_big_cpu_only_machine() {
        let hw = pc(65536, 0, 100000);
        let v = assess_all(&hw).into_iter().find(|a| a.model.id == "qwen3-30b-a3b").unwrap().verdict;
        assert_eq!((v.level, v.mode, v.speed), (Level::Recommended, Some(Mode::Cpu), Some(Speed::Medium)));
    }

    #[test]
    fn a_small_laptop_gets_a_clear_explanation() {
        let hw = pc(4096, 0, 20000);
        let all = assess_all(&hw);
        assert!(all.iter().all(|a| a.verdict.level == Level::Heavy));
        assert!(all.iter().all(|a| !a.best));
        assert!(all[0].verdict.reason.contains("Il faut environ"), "{}", all[0].verdict.reason);
    }

    #[test]
    fn a_big_card_runs_everything_fast() {
        let hw = pc(32768, 24576, 500000);
        for a in assess_all(&hw).iter().filter(|a| a.model.size_mb < 18000) {
            assert_eq!(a.verdict.mode, Some(Mode::Gpu), "{}", a.model.id);
        }
    }

    #[test]
    fn integrated_graphics_do_not_count_as_a_card() {
        let mut hw = pc(16384, 0, 50000);
        hw.gpus = vec![Gpu { name: "Intel UHD".into(), vendor: Vendor::Intel, vram_mb: 8192, integrated: true }];
        let v = assess_all(&hw).into_iter().find(|a| a.model.id == "qwen3-8b").unwrap().verdict;
        assert_eq!(v.mode, Some(Mode::Cpu), "la carte intégrée est ignorée : le modèle passe par le processeur");
    }

    #[test]
    fn missing_avx2_rules_out_cpu_models_and_low_disk_is_flagged() {
        let mut hw = pc(16384, 0, 1000);
        hw.avx2 = false;
        let all = assess_all(&hw);
        assert!(all.iter().all(|a| a.verdict.level == Level::Heavy));
        let hw = pc(24260, 8188, 3000);
        let a = assess_all(&hw).into_iter().find(|a| a.model.id == "qwen3-8b").unwrap();
        assert!(!a.verdict.disk_ok, "5 Go + marge > 3 Go libres");
        assert!(!a.best, "on ne conseille pas un modèle qu'on ne peut pas télécharger");
    }

    /// Sur la vraie machine : `cargo test --lib assesses_this_pc -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn assesses_this_pc() {
        let hw = crate::hardware::detect();
        for a in assess_all(&hw) {
            println!("{:<9} {:<6} {:<7} {}{} | {}", format!("{:?}", a.verdict.level), format!("{:?}", a.verdict.mode), format!("{:?}", a.verdict.speed), a.model.name, if a.best { "  <== conseillé" } else { "" }, a.verdict.reason);
        }
    }
}
