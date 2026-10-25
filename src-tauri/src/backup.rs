//! Sauvegarde et restauration d'un profil (archive .zip), export de toutes les conversations, effacement des données.
//! Les secrets (clé API, jetons des connecteurs) restent dans le coffre Windows : ils ne sont jamais écrits dans une sauvegarde.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::history::StoredMessage;

const MANIFEST: &str = "lama-sauvegarde.json";
const DB: &str = "history.db";

fn walk(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, base, out);
        } else if let Ok(rel) = p.strip_prefix(base) {
            out.push(rel.to_path_buf());
        }
    }
}

fn options() -> zip::write::SimpleFileOptions {
    zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)
}

/// Archive le dossier du profil. `db_copy` : copie cohérente de la base (la base ouverte n'est pas copiée telle quelle).
pub fn backup(dir: &Path, db_copy: &Path, out: &Path, profile: &str) -> Result<usize, String> {
    let mut files = Vec::new();
    walk(dir, dir, &mut files);
    files.retain(|f| {
        let name = f.to_string_lossy();
        // Le profil du navigateur de Lama (cookies, sessions, cache) reste sur ce PC : jamais dans une archive.
        !(name.starts_with(DB) || name.ends_with(".bak") || name.starts_with("history-avant") || name.starts_with("browser/") || name.starts_with("browser\\"))
    });
    let file = std::fs::File::create(out).map_err(|e| format!("Création impossible : {e}"))?;
    let mut zip = zip::ZipWriter::new(file);
    let manifest = serde_json::json!({ "app": "Lama Desktop", "version": env!("CARGO_PKG_VERSION"), "profile": profile });
    zip.start_file(MANIFEST, options()).map_err(|e| e.to_string())?;
    zip.write_all(manifest.to_string().as_bytes()).map_err(|e| e.to_string())?;
    let mut count = 0;
    let add = |zip: &mut zip::ZipWriter<std::fs::File>, name: String, path: &Path| -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|e| format!("Lecture de {name} impossible : {e}"))?;
        zip.start_file(name, options()).map_err(|e| e.to_string())?;
        zip.write_all(&bytes).map_err(|e| e.to_string())
    };
    add(&mut zip, DB.into(), db_copy)?;
    count += 1;
    for f in &files {
        add(&mut zip, f.to_string_lossy().replace('\\', "/"), &dir.join(f))?;
        count += 1;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(count)
}

/// Restaure une sauvegarde dans le dossier du profil. L'ancienne base est gardée sous `history-avant-restauration.db`.
pub fn restore(archive: &Path, dir: &Path) -> Result<usize, String> {
    let file = std::fs::File::open(archive).map_err(|e| format!("Ouverture impossible : {e}"))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|_| "Ce fichier n'est pas une sauvegarde de Lama Desktop.".to_string())?;
    let mut manifest = String::new();
    zip.by_name(MANIFEST)
        .map_err(|_| "Ce fichier n'est pas une sauvegarde de Lama Desktop.".to_string())?
        .read_to_string(&mut manifest)
        .map_err(|e| e.to_string())?;
    if !manifest.contains("Lama Desktop") {
        return Err("Ce fichier n'est pas une sauvegarde de Lama Desktop.".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let current = dir.join(DB);
    if current.exists() {
        std::fs::rename(&current, dir.join("history-avant-restauration.db")).map_err(|e| e.to_string())?;
    }
    let _ = std::fs::remove_file(dir.join("history.db-wal"));
    let _ = std::fs::remove_file(dir.join("history.db-shm"));
    let mut count = 0;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() || entry.name() == MANIFEST {
            continue;
        }
        // `enclosed_name` refuse les chemins qui sortiraient du dossier (« ../ », chemins absolus).
        let Some(rel) = entry.enclosed_name() else { continue };
        let target = dir.join(rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        std::fs::write(&target, bytes).map_err(|e| format!("Écriture de {} impossible : {e}", target.display()))?;
        count += 1;
    }
    Ok(count)
}

/// Conversation en Markdown lisible.
pub fn conversation_markdown(title: &str, model: &str, msgs: &[StoredMessage]) -> String {
    let mut out = format!("# {title}\n\n*Modèle : {model}*\n");
    for m in msgs.iter().filter(|m| matches!(m.role.as_str(), "user" | "assistant") && !m.content.trim().is_empty()) {
        let who = if m.role == "user" { "Moi" } else { "Lama" };
        out.push_str(&format!("\n## {who}\n\n{}\n", m.content.trim()));
    }
    out
}

fn file_stem(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') { c } else { ' ' }).collect();
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.is_empty() { "conversation".into() } else { s.chars().take(60).collect() }
}

/// Une archive de fichiers Markdown : (dossier, titre, contenu). Les titres identiques reçoivent un numéro.
pub fn write_chats(out: &Path, entries: &[(String, String, String)]) -> Result<usize, String> {
    let file = std::fs::File::create(out).map_err(|e| format!("Création impossible : {e}"))?;
    let mut zip = zip::ZipWriter::new(file);
    let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (folder, title, md) in entries {
        let dir = file_stem(folder);
        let stem = file_stem(title);
        let mut name = format!("{dir}/{stem}.md");
        let mut n = 2;
        while !used.insert(name.to_lowercase()) {
            name = format!("{dir}/{stem} ({n}).md");
            n += 1;
        }
        zip.start_file(name, options()).map_err(|e| e.to_string())?;
        zip.write_all(md.as_bytes()).map_err(|e| e.to_string())?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(entries.len())
}

/// Vide un dossier sans le supprimer.
pub fn erase_contents(root: &Path) -> Result<usize, String> {
    let mut n = 0;
    let mut failed = Vec::new();
    let Ok(rd) = std::fs::read_dir(root) else { return Ok(0) };
    for e in rd.flatten() {
        let p = e.path();
        let r = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
        match r {
            Ok(()) => n += 1,
            Err(err) => failed.push(format!("{} ({err})", p.display())),
        }
    }
    if failed.is_empty() { Ok(n) } else { Err(format!("Impossible de supprimer : {}", failed.join(" ; "))) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("bak-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn backup_then_restore_brings_back_files_and_keeps_the_old_database() {
        let (src, dst, work) = (tmp(), tmp(), tmp());
        std::fs::write(src.join("history.db"), "base ouverte").unwrap();
        std::fs::write(src.join("history.db-wal"), "journal").unwrap();
        std::fs::write(src.join("mcp.json"), "{\"mcpServers\":{}}").unwrap();
        std::fs::create_dir_all(src.join("skills/a")).unwrap();
        std::fs::write(src.join("skills/a/SKILL.md"), "# a").unwrap();
        std::fs::create_dir_all(src.join("browser/Default")).unwrap();
        std::fs::write(src.join("browser/Default/Cookies"), "sessions").unwrap();
        let copy = work.join("copie.db");
        std::fs::write(&copy, "base cohérente").unwrap();
        let zip = work.join("s.zip");
        assert_eq!(backup(&src, &copy, &zip, "Samuel").unwrap(), 3);

        std::fs::write(dst.join("history.db"), "base actuelle").unwrap();
        assert_eq!(restore(&zip, &dst).unwrap(), 3);
        assert_eq!(std::fs::read_to_string(dst.join("history.db")).unwrap(), "base cohérente");
        assert_eq!(std::fs::read_to_string(dst.join("history-avant-restauration.db")).unwrap(), "base actuelle");
        assert_eq!(std::fs::read_to_string(dst.join("skills/a/SKILL.md")).unwrap(), "# a");
        assert!(!dst.join("history.db-wal").exists());
        assert!(!dst.join("browser").exists(), "pas de cookies ni de sessions dans une sauvegarde");

        let notes = work.join("autre.zip");
        let mut w = zip::ZipWriter::new(std::fs::File::create(&notes).unwrap());
        w.start_file("notes.txt", options()).unwrap();
        w.write_all(b"x").unwrap();
        w.finish().unwrap();
        assert!(restore(&notes, &dst).unwrap_err().contains("pas une sauvegarde"));
        assert!(restore(&work.join("absent.zip"), &dst).is_err());
        for d in [src, dst, work] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn restore_ignores_paths_that_escape_the_folder() {
        let (dst, work) = (tmp(), tmp());
        let zip = work.join("piege.zip");
        let mut w = zip::ZipWriter::new(std::fs::File::create(&zip).unwrap());
        w.start_file(MANIFEST, options()).unwrap();
        w.write_all(b"{\"app\":\"Lama Desktop\"}").unwrap();
        w.start_file("../evil.txt", options()).unwrap();
        w.write_all(b"x").unwrap();
        w.start_file("ok.txt", options()).unwrap();
        w.write_all(b"y").unwrap();
        w.finish().unwrap();
        assert_eq!(restore(&zip, &dst).unwrap(), 1);
        assert!(dst.join("ok.txt").exists());
        assert!(!dst.parent().unwrap().join("evil.txt").exists());
        for d in [dst, work] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn chats_export_is_readable_and_names_never_collide() {
        let msg = |r: &str, c: &str| StoredMessage { role: r.into(), content: c.into(), thinking: None, extra: None };
        let md = conversation_markdown("Voyage", "gpt-oss", &[msg("user", "Salut"), msg("assistant", "  Bonjour !  "), msg("tool", "x"), msg("assistant", "")]);
        assert_eq!(md, "# Voyage\n\n*Modèle : gpt-oss*\n\n## Moi\n\nSalut\n\n## Lama\n\nBonjour !\n");
        let work = tmp();
        let out = work.join("c.zip");
        let entries = vec![
            ("Sans projet".to_string(), "Idée / plan ?".to_string(), md.clone()),
            ("Sans projet".to_string(), "Idée / plan ?".to_string(), md.clone()),
            ("Voyage".to_string(), "Billets".to_string(), md),
        ];
        assert_eq!(write_chats(&out, &entries).unwrap(), 3);
        let mut z = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
        let names: Vec<String> = (0..z.len()).map(|i| z.by_index(i).unwrap().name().to_string()).collect();
        assert_eq!(names, vec!["Sans projet/Idée plan.md", "Sans projet/Idée plan (2).md", "Voyage/Billets.md"]);
        let _ = std::fs::remove_dir_all(work);
    }

    #[test]
    fn erase_empties_a_folder_but_keeps_it() {
        let d = tmp();
        std::fs::create_dir_all(d.join("a/b")).unwrap();
        std::fs::write(d.join("a/b/f.txt"), "x").unwrap();
        std::fs::write(d.join("g.json"), "x").unwrap();
        assert_eq!(erase_contents(&d).unwrap(), 2);
        assert!(d.exists() && std::fs::read_dir(&d).unwrap().count() == 0);
        assert_eq!(erase_contents(&d.join("absent")).unwrap(), 0);
        let _ = std::fs::remove_dir_all(d);
    }
}
