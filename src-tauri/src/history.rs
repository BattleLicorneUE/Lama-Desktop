use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub model: String,
    pub updated_at: i64,
    pub kind: String,
    pub workspace: Option<String>,
    pub project_id: Option<String>,
    /// Conversation épinglée : affichée en tête de liste.
    pub pinned: bool,
    /// Extrait du message qui correspond à la recherche en cours (vide sans recherche).
    pub snippet: Option<String>,
}

/// Quelques mots autour de la première occurrence de `search` dans `content`.
fn make_snippet(content: &str, search: &str) -> Option<String> {
    let flat: String = content.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars: Vec<char> = flat.chars().collect();
    let needle: Vec<char> = search.to_lowercase().chars().collect();
    if needle.is_empty() {
        return None;
    }
    let lower: Vec<char> = flat.to_lowercase().chars().collect();
    // La mise en minuscules peut changer la longueur : on ne s'en sert que si les positions restent alignées.
    let at = if lower.len() == chars.len() { lower.windows(needle.len()).position(|w| w == needle.as_slice())? } else { 0 };
    let start = at.saturating_sub(40);
    let end = (at + needle.len() + 80).min(chars.len());
    let mut s: String = chars[start..end].iter().collect();
    if start > 0 {
        s.insert(0, '…');
    }
    if end < chars.len() {
        s.push('…');
    }
    Some(s)
}

#[derive(Debug, Clone, Serialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub instructions: String,
    pub pinned: bool,
    /// « chat » ou « code » : les projets de code sont liés à un dossier de travail.
    pub kind: String,
    pub workspace: Option<String>,
    /// Dernière activité : modification du projet ou de l'un de ses chats.
    pub updated_at: i64,
    /// Nombre de chats du projet.
    pub chats: i64,
}

/// Artefact gardé dans la bibliothèque : le dernier état d'une page, d'un document ou d'une image créé dans une conversation.
#[derive(Debug, Clone, Serialize)]
pub struct SavedArtifact {
    pub id: String,
    pub title: String,
    pub lang: String,
    /// document, slides, design, dashboard ou vide (déduit du langage côté interface).
    pub kind: String,
    pub code: String,
    pub conv_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    /// Nombre de versions successives enregistrées.
    pub versions: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Memory {
    pub id: i64,
    pub text: String,
    /// Mémoire d'un projet (relue seulement dans ses conversations) ; `None` : mémoire générale.
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectDoc {
    pub id: String,
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub thinking: Option<String>,
    /// JSON libre côté interface : appels d'outils, images…
    #[serde(default)]
    pub extra: Option<String>,
}

pub struct History {
    conn: Connection,
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl History {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        Self::init(Connection::open(path)?)
    }

    /// Base vide en mémoire : sert de remplaçant tant qu'aucun profil n'est choisi, et dans les tests.
    pub fn in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Self> {
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS conversations (
                 id TEXT PRIMARY KEY, title TEXT NOT NULL, model TEXT NOT NULL,
                 created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS messages (
                 conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
                 position INTEGER NOT NULL, role TEXT NOT NULL, content TEXT NOT NULL, thinking TEXT,
                 PRIMARY KEY (conversation_id, position));",
        )?;
        // Migrations légères : ignorées si la colonne existe déjà.
        let _ = conn.execute("ALTER TABLE conversations ADD COLUMN kind TEXT NOT NULL DEFAULT 'chat'", []);
        let _ = conn.execute("ALTER TABLE conversations ADD COLUMN workspace TEXT", []);
        let _ = conn.execute("ALTER TABLE conversations ADD COLUMN project_id TEXT", []);
        let _ = conn.execute("ALTER TABLE messages ADD COLUMN extra TEXT", []);
        let _ = conn.execute("ALTER TABLE conversations ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0", []);
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memories (
                 id INTEGER PRIMARY KEY AUTOINCREMENT, text TEXT NOT NULL, created_at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS project_docs (
                 id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                 name TEXT NOT NULL, content TEXT NOT NULL, created_at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS projects (
                 id TEXT PRIMARY KEY, name TEXT NOT NULL, instructions TEXT NOT NULL DEFAULT '',
                 created_at INTEGER NOT NULL);",
        )?;
        let _ = conn.execute("ALTER TABLE memories ADD COLUMN project_id TEXT", []);
        let _ = conn.execute("ALTER TABLE projects ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0", []);
        let _ = conn.execute("ALTER TABLE projects ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0", []);
        let _ = conn.execute("ALTER TABLE projects ADD COLUMN kind TEXT NOT NULL DEFAULT 'chat'", []);
        let _ = conn.execute("ALTER TABLE projects ADD COLUMN workspace TEXT", []);
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS artifacts (
                 id TEXT PRIMARY KEY, title TEXT NOT NULL, lang TEXT NOT NULL, kind TEXT NOT NULL DEFAULT '',
                 code TEXT NOT NULL, conv_id TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
                 versions INTEGER NOT NULL DEFAULT 1);",
        )?;
        Ok(Self { conn })
    }

    /// Enregistre un artefact : dans une même conversation, un nouvel état du même type devient une nouvelle version de l'entrée.
    pub fn save_artifact(&self, conv_id: Option<&str>, title: &str, lang: &str, kind: &str, code: &str) -> rusqlite::Result<SavedArtifact> {
        use rusqlite::OptionalExtension;
        let existing: Option<(String, String, String, i64)> = match conv_id {
            Some(c) => self
                .conn
                .query_row("SELECT id, code, kind, versions FROM artifacts WHERE conv_id = ?1 AND lang = ?2", params![c, lang], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                })
                .optional()?,
            None => None,
        };
        let t = now();
        let id = match existing {
            Some((id, old, old_kind, versions)) => {
                if old != code {
                    let kind = if kind.is_empty() { old_kind } else { kind.to_string() };
                    self.conn.execute(
                        "UPDATE artifacts SET title = ?2, kind = ?3, code = ?4, updated_at = ?5, versions = ?6 WHERE id = ?1",
                        params![id, title, kind, code, t, versions + 1],
                    )?;
                }
                id
            }
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                self.conn.execute(
                    "INSERT INTO artifacts (id, title, lang, kind, code, conv_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                    params![id, title, lang, kind, code, conv_id, t],
                )?;
                id
            }
        };
        self.artifact(&id)
    }

    fn artifact(&self, id: &str) -> rusqlite::Result<SavedArtifact> {
        self.conn.query_row(
            "SELECT id, title, lang, kind, code, conv_id, created_at, updated_at, versions FROM artifacts WHERE id = ?1",
            [id],
            Self::artifact_row,
        )
    }

    fn artifact_row(r: &rusqlite::Row) -> rusqlite::Result<SavedArtifact> {
        Ok(SavedArtifact { id: r.get(0)?, title: r.get(1)?, lang: r.get(2)?, kind: r.get(3)?, code: r.get(4)?, conv_id: r.get(5)?, created_at: r.get(6)?, updated_at: r.get(7)?, versions: r.get(8)? })
    }

    pub fn list_artifacts(&self, search: &str) -> rusqlite::Result<Vec<SavedArtifact>> {
        let like = format!("%{}%", search.trim().replace('%', "").replace('_', ""));
        let mut stmt = self.conn.prepare(
            "SELECT id, title, lang, kind, code, conv_id, created_at, updated_at, versions FROM artifacts
             WHERE title LIKE ?1 COLLATE NOCASE ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([like], Self::artifact_row)?;
        rows.collect()
    }

    pub fn delete_artifact(&self, id: &str) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM artifacts WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn create(
        &self,
        model: &str,
        kind: &str,
        workspace: Option<&str>,
        project_id: Option<&str>,
    ) -> rusqlite::Result<Conversation> {
        let c = Conversation {
            id: uuid::Uuid::new_v4().to_string(),
            title: "Nouvelle conversation".into(),
            model: model.into(),
            updated_at: now(),
            kind: kind.into(),
            workspace: workspace.map(String::from),
            project_id: project_id.map(String::from),
            pinned: false,
            snippet: None,
        };
        self.conn.execute(
            "INSERT INTO conversations (id, title, model, created_at, updated_at, kind, workspace, project_id)
             VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, ?7)",
            params![c.id, c.title, c.model, c.updated_at, c.kind, c.workspace, c.project_id],
        )?;
        Ok(c)
    }

    /// `project = None` : toutes les conversations du type ; `Some(id)` : seulement celles du projet.
    pub fn list(&self, search: &str, kind: &str, project: Option<&str>) -> rusqlite::Result<Vec<Conversation>> {
        let like = format!("%{}%", search.replace('%', "\\%").replace('_', "\\_"));
        let mut stmt = self.conn.prepare(
            "SELECT id, title, model, updated_at, kind, workspace, project_id, pinned FROM conversations
             WHERE kind = ?2 AND (?3 IS NULL OR project_id = ?3)
               AND (title LIKE ?1 ESCAPE '\\'
                OR id IN (SELECT conversation_id FROM messages WHERE content LIKE ?1 ESCAPE '\\'))
             ORDER BY pinned DESC, updated_at DESC",
        )?;
        let rows = stmt.query_map(params![like, kind, project], |r| {
            Ok(Conversation {
                id: r.get(0)?,
                title: r.get(1)?,
                model: r.get(2)?,
                updated_at: r.get(3)?,
                kind: r.get(4)?,
                workspace: r.get(5)?,
                project_id: r.get(6)?,
                pinned: r.get::<_, i64>(7)? != 0,
                snippet: None,
            })
        })?;
        let mut list: Vec<Conversation> = rows.collect::<rusqlite::Result<_>>()?;
        // Recherche : on montre l'extrait du message trouvé quand le titre ne suffit pas à expliquer le résultat.
        if !search.trim().is_empty() {
            let mut q = self.conn.prepare(
                "SELECT content FROM messages WHERE conversation_id = ?1 AND content LIKE ?2 ESCAPE '\\' ORDER BY position LIMIT 1",
            )?;
            for c in &mut list {
                if c.title.to_lowercase().contains(&search.trim().to_lowercase()) {
                    continue;
                }
                if let Ok(content) = q.query_row(params![c.id, like], |r| r.get::<_, String>(0)) {
                    c.snippet = make_snippet(&content, search.trim());
                }
            }
        }
        Ok(list)
    }

    pub fn set_pinned(&self, id: &str, pinned: bool) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE conversations SET pinned = ?2 WHERE id = ?1", params![id, pinned as i64])?;
        Ok(())
    }

    /// Branche : copie la conversation jusqu'au message `upto` (exclu) dans une nouvelle conversation.
    pub fn fork(&mut self, id: &str, upto: usize) -> rusqlite::Result<Conversation> {
        let (title, model, kind, workspace, project): (String, String, String, Option<String>, Option<String>) = self.conn.query_row(
            "SELECT title, model, kind, workspace, project_id FROM conversations WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )?;
        let msgs: Vec<StoredMessage> = self.messages(id)?.into_iter().take(upto).collect();
        let mut c = self.create(&model, &kind, workspace.as_deref(), project.as_deref())?;
        c.title = format!("{title} (branche)");
        self.rename(&c.id, &c.title)?;
        self.save_messages(&c.id, &model, &msgs)?;
        Ok(c)
    }

    pub fn create_project(&self, name: &str, instructions: &str, kind: &str, workspace: Option<&str>) -> rusqlite::Result<Project> {
        let p = Project {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            instructions: instructions.into(),
            pinned: false,
            kind: kind.into(),
            workspace: workspace.map(String::from),
            updated_at: now(),
            chats: 0,
        };
        self.conn.execute(
            "INSERT INTO projects (id, name, instructions, created_at, updated_at, kind, workspace) VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6)",
            params![p.id, p.name, p.instructions, p.updated_at, p.kind, p.workspace],
        )?;
        Ok(p)
    }

    pub fn list_projects(&self) -> rusqlite::Result<Vec<Project>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.id, p.name, p.instructions, p.pinned, p.kind, p.workspace,
                    MAX(p.updated_at, p.created_at, COALESCE((SELECT MAX(c.updated_at) FROM conversations c WHERE c.project_id = p.id), 0)),
                    (SELECT COUNT(*) FROM conversations c WHERE c.project_id = p.id)
             FROM projects p ORDER BY p.pinned DESC, 7 DESC, p.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Project {
                id: r.get(0)?,
                name: r.get(1)?,
                instructions: r.get(2)?,
                pinned: r.get::<_, i64>(3)? != 0,
                kind: r.get(4)?,
                workspace: r.get(5)?,
                updated_at: r.get(6)?,
                chats: r.get(7)?,
            })
        })?;
        rows.collect()
    }

    pub fn set_project_pinned(&self, id: &str, pinned: bool) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE projects SET pinned = ?2 WHERE id = ?1", params![id, pinned as i64])?;
        Ok(())
    }

    pub fn update_project(&self, id: &str, name: &str, instructions: &str, workspace: Option<&str>) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE projects SET name = ?2, instructions = ?3, updated_at = ?4, workspace = COALESCE(?5, workspace) WHERE id = ?1",
            params![id, name, instructions, now(), workspace],
        )?;
        Ok(())
    }

    /// Copie cohérente de la base dans un nouveau fichier (la base ouverte ne se copie pas telle quelle).
    pub fn backup_to(&self, path: &Path) -> rusqlite::Result<()> {
        let _ = std::fs::remove_file(path);
        self.conn.execute("VACUUM INTO ?1", [path.to_string_lossy().as_ref()])?;
        Ok(())
    }

    pub fn add_memory(&self, text: &str) -> rusqlite::Result<Memory> {
        self.add_memory_in(text, None)
    }

    pub fn add_memory_in(&self, text: &str, project: Option<&str>) -> rusqlite::Result<Memory> {
        self.conn.execute("INSERT INTO memories (text, created_at, project_id) VALUES (?1, ?2, ?3)", params![text, now(), project])?;
        Ok(Memory { id: self.conn.last_insert_rowid(), text: text.into(), project_id: project.map(String::from) })
    }

    /// Mémoire générale (hors projets).
    pub fn list_memories(&self) -> rusqlite::Result<Vec<Memory>> {
        self.list_memories_of(None)
    }

    pub fn list_memories_of(&self, project: Option<&str>) -> rusqlite::Result<Vec<Memory>> {
        let mut stmt = self.conn.prepare("SELECT id, text, project_id FROM memories WHERE project_id IS ?1 ORDER BY id")?;
        let rows = stmt.query_map([project], |r| Ok(Memory { id: r.get(0)?, text: r.get(1)?, project_id: r.get(2)? }))?;
        rows.collect()
    }

    /// Conversations passées qui parlent de ces mots (toutes sortes, tous projets), les plus pertinentes d'abord :
    /// (titre, date de dernière activité, extrait).
    pub fn search_chats(&self, query: &str, limit: usize) -> rusqlite::Result<Vec<(String, i64, String)>> {
        let words: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.chars().count() >= 3)
            .map(|w| w.to_lowercase())
            .take(5)
            .collect();
        let mut found: std::collections::HashMap<String, (usize, String, i64, String)> = std::collections::HashMap::new();
        for w in &words {
            let like = format!("%{}%", w.replace('%', "\\%").replace('_', "\\_"));
            let mut stmt = self.conn.prepare(
                "SELECT c.id, c.title, c.updated_at, m.content FROM messages m JOIN conversations c ON c.id = m.conversation_id
                 WHERE m.content LIKE ?1 ESCAPE '\\' OR c.title LIKE ?1 ESCAPE '\\' GROUP BY c.id ORDER BY c.updated_at DESC LIMIT 100",
            )?;
            let rows = stmt.query_map([like], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?)))?;
            for row in rows {
                let (id, title, at, content) = row?;
                let snippet = make_snippet(&content, w).unwrap_or_default();
                let e = found.entry(id).or_insert((0, title, at, snippet.clone()));
                e.0 += 1;
                if e.3.is_empty() {
                    e.3 = snippet;
                }
            }
        }
        let mut list: Vec<_> = found.into_values().collect();
        list.sort_by(|a, b| b.0.cmp(&a.0).then(b.2.cmp(&a.2)));
        Ok(list.into_iter().take(limit).map(|(_, t, at, s)| (t, at, s)).collect())
    }

    pub fn delete_memory(&self, id: i64) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM memories WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn add_doc(&self, project_id: &str, name: &str, content: &str) -> rusqlite::Result<ProjectDoc> {
        let d = ProjectDoc { id: uuid::Uuid::new_v4().to_string(), name: name.into(), content: content.into() };
        self.conn.execute(
            "INSERT INTO project_docs (id, project_id, name, content, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![d.id, project_id, d.name, d.content, now()],
        )?;
        Ok(d)
    }

    pub fn list_docs(&self, project_id: &str) -> rusqlite::Result<Vec<ProjectDoc>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, content FROM project_docs WHERE project_id = ?1 ORDER BY created_at, name")?;
        let rows = stmt.query_map([project_id], |r| {
            Ok(ProjectDoc { id: r.get(0)?, name: r.get(1)?, content: r.get(2)? })
        })?;
        rows.collect()
    }

    pub fn delete_doc(&self, id: &str) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM project_docs WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Supprime le projet ; ses conversations sont conservées, sans projet.
    pub fn delete_project(&self, id: &str) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE conversations SET project_id = NULL WHERE project_id = ?1", [id])?;
        self.conn.execute("DELETE FROM project_docs WHERE project_id = ?1", [id])?;
        self.conn.execute("DELETE FROM memories WHERE project_id = ?1", [id])?;
        self.conn.execute("DELETE FROM projects WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn set_conversation_project(&self, id: &str, project: Option<&str>) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE conversations SET project_id = ?2 WHERE id = ?1", params![id, project])?;
        Ok(())
    }

    pub fn messages(&self, id: &str) -> rusqlite::Result<Vec<StoredMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT role, content, thinking, extra FROM messages WHERE conversation_id = ?1 ORDER BY position",
        )?;
        let rows = stmt.query_map([id], |r| {
            Ok(StoredMessage { role: r.get(0)?, content: r.get(1)?, thinking: r.get(2)?, extra: r.get(3)? })
        })?;
        rows.collect()
    }

    /// Remplace tous les messages de la conversation (dans une transaction).
    pub fn save_messages(&mut self, id: &str, model: &str, msgs: &[StoredMessage]) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM messages WHERE conversation_id = ?1", [id])?;
        for (i, m) in msgs.iter().enumerate() {
            tx.execute(
                "INSERT INTO messages (conversation_id, position, role, content, thinking, extra) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, i as i64, m.role, m.content, m.thinking, m.extra],
            )?;
        }
        tx.execute("UPDATE conversations SET updated_at = ?2, model = ?3 WHERE id = ?1", params![id, now(), model])?;
        tx.commit()
    }

    pub fn rename(&self, id: &str, title: &str) -> rusqlite::Result<()> {
        self.conn.execute("UPDATE conversations SET title = ?2 WHERE id = ?1", params![id, title])?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> rusqlite::Result<()> {
        self.conn.execute("DELETE FROM conversations WHERE id = ?1", [id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_snippet_pinning_and_forking() {
        let mut db = History::in_memory().unwrap();
        let a = db.create("m", "chat", None, None).unwrap();
        let b = db.create("m", "chat", None, None).unwrap();
        db.rename(&a.id, "Recette").unwrap();
        db.rename(&b.id, "Voyage").unwrap();
        let long = format!("{} le mot-clé quinoa est ici {}", "avant ".repeat(30), "après ".repeat(30));
        db.save_messages(&a.id, "m", &[msg("user", "Question"), msg("assistant", &long), msg("user", "Suite")]).unwrap();
        db.save_messages(&b.id, "m", &[msg("user", "Billets d'avion")]).unwrap();

        let found = db.list("QUINOA", "chat", None).unwrap();
        assert_eq!(found.len(), 1);
        let snip = found[0].snippet.as_deref().unwrap();
        assert!(snip.to_lowercase().contains("quinoa") && snip.starts_with('…') && snip.chars().count() < 160, "{snip}");
        assert!(db.list("voyage", "chat", None).unwrap()[0].snippet.is_none(), "titre trouvé : pas d'extrait");

        db.set_pinned(&b.id, true).unwrap();
        let all = db.list("", "chat", None).unwrap();
        assert_eq!(all[0].id, b.id, "l'épinglée passe devant, même plus ancienne");
        assert!(all[0].pinned && !all[1].pinned);

        let f = db.fork(&a.id, 1).unwrap();
        assert_eq!(f.title, "Recette (branche)");
        let copy = db.messages(&f.id).unwrap();
        assert_eq!(copy.len(), 1);
        assert_eq!(copy[0].content, "Question");
        assert_eq!(db.messages(&a.id).unwrap().len(), 3, "l'original est intact");
    }

    fn msg(role: &str, content: &str) -> StoredMessage {
        StoredMessage { role: role.into(), content: content.into(), thinking: None, extra: None }
    }

    #[test]
    fn save_and_reload_messages_in_order() {
        let mut h = History::in_memory().unwrap();
        let c = h.create("llama3", "chat", None, None).unwrap();
        h.save_messages(&c.id, "llama3", &[msg("user", "a"), msg("assistant", "b")]).unwrap();
        h.save_messages(&c.id, "llama3", &[msg("user", "a"), msg("assistant", "b"), msg("user", "c")]).unwrap();
        let m = h.messages(&c.id).unwrap();
        assert_eq!(m.iter().map(|m| m.content.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
    }

    #[test]
    fn list_is_filtered_by_kind() {
        let h = History::in_memory().unwrap();
        h.create("m", "chat", None, None).unwrap();
        h.create("m", "code", Some("C:/proj"), None).unwrap();
        let code = h.list("", "code", None).unwrap();
        assert_eq!(code.len(), 1);
        assert_eq!(code[0].workspace.as_deref(), Some("C:/proj"));
        assert_eq!(h.list("", "chat", None).unwrap().len(), 1);
    }

    #[test]
    fn projects_group_conversations_and_survive_deletion() {
        let h = History::in_memory().unwrap();
        let p = h.create_project("Rust", "Réponds en français", "chat", None).unwrap();
        let a = h.create("m", "chat", None, Some(&p.id)).unwrap();
        h.create("m", "chat", None, None).unwrap();
        assert_eq!(h.list("", "chat", Some(&p.id)).unwrap().len(), 1);
        assert_eq!(h.list("", "chat", None).unwrap().len(), 2);
        h.update_project(&p.id, "Rust 2", "x", None).unwrap();
        assert_eq!(h.list_projects().unwrap()[0].name, "Rust 2");
        h.delete_project(&p.id).unwrap();
        assert!(h.list_projects().unwrap().is_empty());
        assert_eq!(h.list("", "chat", None).unwrap().iter().find(|c| c.id == a.id).unwrap().project_id, None);
        let b = h.create("m", "chat", None, None).unwrap();
        let p2 = h.create_project("P", "", "chat", None).unwrap();
        h.set_conversation_project(&b.id, Some(&p2.id)).unwrap();
        assert_eq!(h.list("", "chat", Some(&p2.id)).unwrap().len(), 1);
    }

    #[test]
    fn artifacts_keep_one_entry_per_conversation_and_count_versions() {
        let h = History::in_memory().unwrap();
        let a = h.save_artifact(Some("c1"), "Rapport", "markdown", "document", "# V1").unwrap();
        let b = h.save_artifact(Some("c1"), "Rapport v2", "markdown", "", "# V2").unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!((b.versions, b.kind.as_str(), b.title.as_str()), (2, "document", "Rapport v2"));
        // Même contenu : pas de nouvelle version.
        assert_eq!(h.save_artifact(Some("c1"), "Rapport v2", "markdown", "", "# V2").unwrap().versions, 2);
        h.save_artifact(Some("c1"), "Page", "html", "slides", "<html></html>").unwrap();
        h.save_artifact(None, "Libre", "svg", "", "<svg/>").unwrap();
        assert_eq!(h.list_artifacts("").unwrap().len(), 3);
        assert_eq!(h.list_artifacts("rapp").unwrap().len(), 1);
        h.delete_artifact(&a.id).unwrap();
        assert_eq!(h.list_artifacts("").unwrap().len(), 2);
    }

    #[test]
    fn code_projects_keep_their_folder() {
        let h = History::in_memory().unwrap();
        let p = h.create_project("App", "Utilise pnpm", "code", Some("C:/dev/app")).unwrap();
        h.create_project("Chat", "", "chat", None).unwrap();
        let code: Vec<_> = h.list_projects().unwrap().into_iter().filter(|x| x.kind == "code").collect();
        assert_eq!((code.len(), code[0].workspace.as_deref()), (1, Some("C:/dev/app")));
        // Sans nouveau dossier, l'ancien est conservé.
        h.update_project(&p.id, "App 2", "x", None).unwrap();
        h.update_project(&p.id, "App 2", "x", Some("C:/dev/autre")).unwrap();
        let again = h.list_projects().unwrap().into_iter().find(|x| x.id == p.id).unwrap();
        assert_eq!((again.name.as_str(), again.workspace.as_deref()), ("App 2", Some("C:/dev/autre")));
    }

    #[test]
    fn pinned_projects_come_first_and_report_their_chats() {
        let h = History::in_memory().unwrap();
        let a = h.create_project("Alpha", "", "chat", None).unwrap();
        let b = h.create_project("Beta", "Le deuxième", "chat", None).unwrap();
        h.create("m", "chat", None, Some(&b.id)).unwrap();
        h.set_project_pinned(&a.id, true).unwrap();
        let list = h.list_projects().unwrap();
        assert_eq!(list[0].name, "Alpha");
        assert!(list[0].pinned && !list[1].pinned);
        assert_eq!((list[0].chats, list[1].chats), (0, 1));
        assert!(list[1].updated_at > 0);
    }

    #[test]
    fn memories_and_project_docs_roundtrip() {
        let h = History::in_memory().unwrap();
        let m = h.add_memory("Préfère le français").unwrap();
        assert_eq!(h.list_memories().unwrap().len(), 1);
        h.delete_memory(m.id).unwrap();
        assert!(h.list_memories().unwrap().is_empty());

        let p = h.create_project("P", "", "chat", None).unwrap();
        let d = h.add_doc(&p.id, "notes.md", "contenu").unwrap();
        assert_eq!(h.list_docs(&p.id).unwrap()[0].content, "contenu");
        h.delete_doc(&d.id).unwrap();
        h.add_doc(&p.id, "b.md", "x").unwrap();
        h.delete_project(&p.id).unwrap();
        assert!(h.list_docs(&p.id).unwrap().is_empty());
    }

    #[test]
    fn search_matches_title_and_content_and_delete_cascades() {
        let mut h = History::in_memory().unwrap();
        let c = h.create("m", "chat", None, None).unwrap();
        h.rename(&c.id, "Recette").unwrap();
        h.save_messages(&c.id, "m", &[msg("user", "parle de rust")]).unwrap();
        assert_eq!(h.list("rust", "chat", None).unwrap().len(), 1);
        assert_eq!(h.list("recet", "chat", None).unwrap().len(), 1);
        assert_eq!(h.list("zzz", "chat", None).unwrap().len(), 0);
        h.delete(&c.id).unwrap();
        assert!(h.messages(&c.id).unwrap().is_empty());
    }

    #[test]
    fn project_memories_stay_in_their_project_and_chats_can_be_searched() {
        let mut h = History::in_memory().unwrap();
        let p = h.create_project("Voyage", "", "chat", None).unwrap();
        h.add_memory("Aime le thé").unwrap();
        h.add_memory_in("Budget : 2000 €", Some(&p.id)).unwrap();
        assert_eq!(h.list_memories().unwrap().len(), 1);
        let mine = h.list_memories_of(Some(&p.id)).unwrap();
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].project_id.as_deref(), Some(p.id.as_str()));
        let c = h.create("m", "chat", None, None).unwrap();
        h.rename(&c.id, "Billets de train").unwrap();
        let msg = |t: &str| StoredMessage { role: "user".into(), content: t.into(), thinking: None, extra: None };
        h.save_messages(&c.id, "m", &[msg("Je cherche un billet pour Lisbonne en mars")]).unwrap();
        let other = h.create("m", "chat", None, None).unwrap();
        h.save_messages(&other.id, "m", &[msg("Recette de crêpes")]).unwrap();
        let r = h.search_chats("billet Lisbonne", 5).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].0, "Billets de train");
        assert!(r[0].2.contains("Lisbonne"));
        assert!(h.search_chats("xyz", 5).unwrap().is_empty());
        let copy = std::env::temp_dir().join(format!("copie-{}.db", uuid::Uuid::new_v4()));
        h.backup_to(&copy).unwrap();
        let reopened = History::open(&copy).unwrap();
        assert_eq!(reopened.list_memories().unwrap().len(), 1);
        assert_eq!(reopened.search_chats("Lisbonne", 5).unwrap().len(), 1);
        drop(reopened);
        let _ = std::fs::remove_file(&copy);
        h.delete_project(&p.id).unwrap();
        assert!(h.list_memories_of(Some(&p.id)).unwrap().is_empty());
        assert_eq!(h.list_memories().unwrap().len(), 1);
    }
}
