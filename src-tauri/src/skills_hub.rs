//! Installation de skills depuis skills.sh / GitHub.
//!
//! L'API de skills.sh exige un jeton Vercel : on lit donc seulement la page publique du classement,
//! et les skills sont téléchargés depuis leur dépôt GitHub (archive publique), comme le fait `npx skills add`.

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use futures_util::StreamExt;
use regex::Regex;
use serde::Serialize;

const UA: &str = concat!("LamaDesktop/", env!("CARGO_PKG_VERSION"), " (+skills)");
const MAX_ARCHIVE: usize = 80_000_000;
const MAX_UNPACKED: u64 = 250_000_000;
const MAX_FILES: usize = 20_000;

macro_rules! re {
    ($pat:expr) => {{
        static R: OnceLock<Regex> = OnceLock::new();
        R.get_or_init(|| Regex::new($pat).unwrap())
    }};
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Entry {
    pub rank: u32,
    pub name: String,
    /// `owner/repo`
    pub source: String,
    /// Identifiant complet `owner/repo/skill`.
    pub id: String,
    pub installs: String,
}

/// Lit les lignes du classement dans le HTML de skills.sh.
pub fn parse_leaderboard(html: &str) -> Vec<Entry> {
    let row = re!(r#"(?s)<a class="group grid[^>]*href="/([^"/]+/[^"/]+/[^"/]+)"[^>]*>(.*?)</a>"#);
    let mut seen = std::collections::HashSet::new();
    row.captures_iter(html)
        .filter_map(|c| {
            let id = c[1].to_string();
            if !seen.insert(id.clone()) {
                return None;
            }
            let cells: Vec<String> = re!(r"<[^>]+>")
                .replace_all(&c[2], "|")
                .split('|')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            // Cellules : rang, nom, dépôt, installations.
            let rank = cells.first()?.parse().ok()?;
            let (owner_repo, skill) = id.rsplit_once('/')?;
            Some(Entry {
                rank,
                name: cells.get(1).cloned().unwrap_or_else(|| skill.to_string()),
                source: owner_repo.to_string(),
                installs: cells.get(3).cloned().unwrap_or_default(),
                id,
            })
        })
        .collect()
}

pub async fn browse(view: &str) -> Result<Vec<Entry>, String> {
    let path = match view {
        "trending" => "trending",
        "hot" => "hot",
        _ => "",
    };
    let resp = client()?
        .get(format!("https://www.skills.sh/{path}"))
        .send()
        .await
        .map_err(|e| format!("skills.sh injoignable : {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("skills.sh a répondu {}", resp.status()));
    }
    let entries = parse_leaderboard(&resp.text().await.map_err(|e| e.to_string())?);
    if entries.is_empty() {
        return Err("Le classement n'a pas pu être lu (la page de skills.sh a peut-être changé). Colle plutôt une commande ou un dépôt.".into());
    }
    Ok(entries)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Spec {
    pub owner: String,
    pub repo: String,
    pub skill: Option<String>,
}

fn valid_part(s: &str) -> bool {
    !s.is_empty() && s.len() <= 100 && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')) && s != "." && s != ".."
}

/// Comprend une commande `npx skills add …`, une URL skills.sh / GitHub, ou `owner/repo[/skill]`.
pub fn parse_spec(input: &str) -> Result<Spec, String> {
    let words: Vec<&str> = input.split_whitespace().collect();
    // Dans une commande, on prend le premier mot qui n'est ni une option ni un mot de la commande.
    let token = words
        .iter()
        .find(|w| !w.starts_with('-') && !matches!(w.to_lowercase().trim_start_matches("skills@"), "npx" | "skills" | "add" | "latest" | "bunx" | "pnpm" | "dlx" | "npm" | "exec" | "i" | "install"))
        .ok_or("Entrée vide")?;

    let mut t = token.trim().trim_matches(['"', '\'']).to_string();
    for p in ["https://", "http://"] {
        if let Some(r) = t.strip_prefix(p) {
            t = r.to_string();
        }
    }
    let t = t.trim_start_matches("www.");
    let mut parts: Vec<&str> = if let Some(r) = t.strip_prefix("skills.sh/") {
        r.split('/').collect()
    } else if let Some(r) = t.strip_prefix("github.com/") {
        r.trim_end_matches(".git").split('/').collect()
    } else {
        t.trim_end_matches(".git").split('/').collect()
    };
    parts.retain(|p| !p.is_empty());
    // github.com/o/r/tree/<branche>/skills/<nom> : on garde le dernier segment comme skill.
    let skill = match parts.as_slice() {
        [_, _, "tree" | "blob", .., last] => Some((*last).to_string()),
        [_, _, s] => Some((*s).to_string()),
        _ => None,
    };
    let (Some(owner), Some(repo)) = (parts.first(), parts.get(1)) else {
        return Err("Format attendu : owner/repo, owner/repo/skill, une URL skills.sh/GitHub ou une commande npx skills add".into());
    };
    if !valid_part(owner) || !valid_part(repo) {
        return Err(format!("Dépôt invalide : {owner}/{repo}"));
    }
    let skill = skill.filter(|s| valid_part(s) && !matches!(s.as_str(), "tree" | "blob"));
    Ok(Spec { owner: (*owner).into(), repo: (*repo).into(), skill })
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(UA)
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())
}

/// Télécharge l'archive publique du dépôt et l'extrait dans `dest` (remplacé s'il existe).
pub async fn fetch_repo(spec: &Spec, dest: &Path) -> Result<(), String> {
    let url = format!("https://codeload.github.com/{}/{}/zip/HEAD", spec.owner, spec.repo);
    let resp = client()?.get(&url).send().await.map_err(|e| format!("GitHub injoignable : {e}"))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(format!("Dépôt introuvable ou privé : {}/{}", spec.owner, spec.repo));
    }
    if !resp.status().is_success() {
        return Err(format!("GitHub a répondu {}", resp.status()));
    }
    let mut data: Vec<u8> = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        data.extend_from_slice(&chunk.map_err(|e| e.to_string())?);
        if data.len() > MAX_ARCHIVE {
            return Err("Dépôt trop volumineux (80 Mo max)".into());
        }
    }
    if dest.exists() {
        std::fs::remove_dir_all(dest).map_err(|e| e.to_string())?;
    }
    extract_zip(&data, dest)
}

/// Extraction sûre : pas de sortie du dossier (zip-slip), pas de liens symboliques, tailles plafonnées.
pub fn extract_zip(data: &[u8], dest: &Path) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(Cursor::new(data)).map_err(|e| format!("Archive invalide : {e}"))?;
    if zip.len() > MAX_FILES {
        return Err("Archive trop volumineuse (trop de fichiers)".into());
    }
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let mut total: u64 = 0;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = f.enclosed_name() else { continue };
        if f.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            continue; // lien symbolique
        }
        // Le premier niveau est le dossier `repo-<commit>` : on le retire.
        let rel: PathBuf = name.components().skip(1).collect();
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = dest.join(&rel);
        if f.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        total += f.size();
        if total > MAX_UNPACKED {
            return Err("Archive trop volumineuse une fois décompressée".into());
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut buf = Vec::with_capacity(f.size() as usize);
        f.by_ref().take(MAX_UNPACKED).read_to_end(&mut buf).map_err(|e| e.to_string())?;
        std::fs::write(&out, buf).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const HTML: &str = r#"<div><a class="group grid grid-cols-[auto_1fr_auto] items-start" href="/vercel-labs/skills/find-skills"><div><span>1</span></div><div><h3 class="x">find-skills</h3><p class="y">vercel-labs/skills</p></div><div class="z">3.9M</div></a>
        <a class="group grid g" href="/mattpocock/skills/tdd"><div><span>2</span></div><div><h3>tdd</h3><p>mattpocock/skills</p></div><div>1.1M</div></a>
        <a class="group grid g" href="/vercel-labs/skills/find-skills"><div><span>9</span></div><div><h3>find-skills</h3></div></a>
        <a href="/docs/cli">doc</a></div>"#;

    #[test]
    fn parses_leaderboard_rows_and_ignores_duplicates_and_other_links() {
        let e = parse_leaderboard(HTML);
        assert_eq!(e.len(), 2);
        assert_eq!(
            e[0],
            Entry { rank: 1, name: "find-skills".into(), source: "vercel-labs/skills".into(), id: "vercel-labs/skills/find-skills".into(), installs: "3.9M".into() }
        );
        assert_eq!(e[1].installs, "1.1M");
    }

    #[test]
    fn parses_every_supported_input_form() {
        let s = |o: &str, r: &str, k: Option<&str>| Spec { owner: o.into(), repo: r.into(), skill: k.map(String::from) };
        assert_eq!(parse_spec("npx skills add vercel-labs/skills/find-skills").unwrap(), s("vercel-labs", "skills", Some("find-skills")));
        assert_eq!(parse_spec("npx skills@latest add vercel-labs/agent-skills -g -y").unwrap(), s("vercel-labs", "agent-skills", None));
        assert_eq!(parse_spec("https://skills.sh/anthropics/skills/frontend-design").unwrap(), s("anthropics", "skills", Some("frontend-design")));
        assert_eq!(parse_spec("https://www.skills.sh/mattpocock/skills").unwrap(), s("mattpocock", "skills", None));
        assert_eq!(parse_spec("https://github.com/expo/skills").unwrap(), s("expo", "skills", None));
        assert_eq!(parse_spec("https://github.com/expo/skills.git").unwrap(), s("expo", "skills", None));
        assert_eq!(parse_spec("https://github.com/expo/skills/tree/main/skills/react-native").unwrap(), s("expo", "skills", Some("react-native")));
        assert_eq!(parse_spec("  owner/repo  ").unwrap(), s("owner", "repo", None));
    }

    /// Test réseau : `cargo test live_ -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn live_browse_and_install_from_github() {
        let top = browse("all").await.unwrap();
        println!("{} skills dans le classement ; n°1 = {:?}", top.len(), top[0]);
        assert!(top.len() > 20);

        let spec = parse_spec("npx skills add vercel-labs/skills/find-skills").unwrap();
        let dest = std::env::temp_dir().join(format!("hub-live-{}", uuid::Uuid::new_v4()));
        fetch_repo(&spec, &dest).await.unwrap();
        let found = crate::skills::discover(&[dest.clone()], &[]);
        println!("skills du dépôt : {:?}", found.iter().map(|f| f.name.as_str()).collect::<Vec<_>>());
        let wanted = found.iter().find(|f| f.name == "find-skills" || f.dir.ends_with("find-skills")).expect("find-skills");
        let target = std::env::temp_dir().join(format!("hub-installed-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&target).unwrap();
        assert!(crate::skills::import(&wanted.dir, &target).unwrap());
        assert!(target.join("find-skills").join("SKILL.md").is_file());
    }

    #[test]
    fn rejects_garbage_and_path_tricks() {
        assert!(parse_spec("").is_err());
        assert!(parse_spec("juste-un-mot").is_err());
        assert!(parse_spec("../../etc/passwd").is_err());
        // Seul le premier mot utile est lu : le reste de la ligne est ignoré.
        assert_eq!(parse_spec("owner/rep o").unwrap().repo, "rep");
        assert!(parse_spec("a b/c").is_err(), "« a » seul n'est pas un dépôt");
        assert!(parse_spec("ow ner/repo/../x").is_err());
    }

    fn zip_with(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, body) in entries {
            w.start_file(*name, opts).unwrap();
            w.write_all(body.as_bytes()).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn extracts_without_prefix_and_blocks_zip_slip() {
        let data = zip_with(&[
            ("repo-abc/skills/a/SKILL.md", "---\nname: a\n---\nx"),
            ("repo-abc/skills/a/notes.txt", "n"),
            ("repo-abc/../../evil.txt", "pwned"),
            ("/abs/evil2.txt", "pwned"),
        ]);
        let dest = std::env::temp_dir().join(format!("hub-{}", uuid::Uuid::new_v4()));
        extract_zip(&data, &dest).unwrap();
        assert!(dest.join("skills/a/SKILL.md").is_file());
        assert!(dest.join("skills/a/notes.txt").is_file());
        assert!(!dest.parent().unwrap().join("evil.txt").exists());
        assert!(!dest.join("abs").exists() && !dest.join("evil2.txt").exists());
    }
}
