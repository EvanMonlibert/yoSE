//! Module de persistance et moteur de recherche local (SQLite WAL + FTS5 + Cache de requêtes).
//!
//! Optimisations incluses :
//! - Utilisation systématique de `prepare_cached` (cache LRU de requêtes SQL compilées).
//! - Pragmas SQLite haute performance (`cache_size`, `mmap_size`, `journal_mode = WAL`).
//! - Gestion complète des groupes personnalisables (nom, couleur, icône, ordre, veille de groupe, suppression).

use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Statut du cycle de vie d'un onglet :
/// - `Active` : Affiché au premier plan.
/// - `Cached` : Figé en cache mémoire (réveil instantané en 0 ms sans rechargement réseau).
/// - `Hibernated` : Déchargé de la RAM (0 Mo), sauvegardé sur disque dans SQLite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TabStatus {
    Active,
    Cached,
    Hibernated,
}

impl TabStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TabStatus::Active => "active",
            TabStatus::Cached => "cached",
            TabStatus::Hibernated => "hibernated",
        }
    }

    pub fn from_db(s: &str) -> Self {
        match s {
            "active" => TabStatus::Active,
            "cached" => TabStatus::Cached,
            _ => TabStatus::Hibernated,
        }
    }
}

/// Groupe d'onglets (Espace de travail personnalisable au clic droit).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabGroup {
    pub id: i64,
    pub name: String,
    pub color: String,
    pub icon: String,
    pub collapsed: bool,
    pub position: i64,
}

/// Modèle de données persistant d'un onglet yoSE.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabRecord {
    pub id: i64,
    pub group_id: i64,
    pub url: String,
    pub title: String,
    pub scroll_pos: f64,
    pub is_active: bool,
    pub status: TabStatus,
    pub is_bookmarked: bool,
}

/// Entrée retournée par le moteur de recherche local FTS5.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub url: String,
    pub title: String,
    pub snippet: String,
    pub is_bookmark: bool,
    pub visit_count: i64,
    pub rank: f64,
}

/// Favori (Signet) géré dans la page dédiée `yose://bookmarks`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookmarkRecord {
    pub id: i64,
    pub title: String,
    pub url: String,
    pub visit_count: i64,
}

/// Statistiques de la base locale affichées sur `yose://home`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbStats {
    pub total_tabs: i64,
    pub cached_tabs: i64,
    pub hibernated_tabs: i64,
    pub active_tabs: i64,
    pub total_groups: i64,
    pub indexed_pages: i64,
    pub bookmarks_count: i64,
    pub wal_mode: bool,
}

/// État complet de la session pour synchroniser le Shell UI en une seule passe IPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub groups: Vec<TabGroup>,
    pub tabs: Vec<TabRecord>,
    pub bookmarks: Vec<BookmarkRecord>,
    pub stats: DbStats,
}

/// Gestionnaire de base de données SQLite optimisé avec cache d'instructions préparées.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Initialise la base SQLite en mode WAL et configure le cache mémoire interne.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;

        // Capacité du cache d'instructions SQL préparées (évite toute recompilation SQL)
        conn.set_prepared_statement_cache_capacity(64);

        // Configuration matérielle pour écritures/lectures asynchrones ultra-rapides
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "temp_store", "MEMORY")?;
        conn.pragma_update(None, "cache_size", -16_384i64)?; // 16 Mo page cache SQLite
        conn.pragma_update(None, "mmap_size", 67_108_864i64)?; // 64 Mo mmap I/O

        let db = Self { conn };
        db.init_schema()?;
        db.migrate_columns_if_needed()?;
        db.bootstrap_cold_start()?;
        Ok(db)
    }

    /// Crée le schéma relationnel et la table virtuelle FTS5.
    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS groups (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                name        TEXT NOT NULL,
                color       TEXT NOT NULL DEFAULT '#2E7D47',
                icon        TEXT NOT NULL DEFAULT '◆',
                collapsed   INTEGER NOT NULL DEFAULT 0,
                position    INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS tabs (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                group_id    INTEGER NOT NULL,
                url         TEXT NOT NULL,
                title       TEXT NOT NULL,
                scroll_pos  REAL NOT NULL DEFAULT 0.0,
                is_active   INTEGER NOT NULL DEFAULT 0,
                status      TEXT NOT NULL DEFAULT 'hibernated',
                updated_at  INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                FOREIGN KEY (group_id) REFERENCES groups(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_tabs_group ON tabs(group_id);
            CREATE INDEX IF NOT EXISTS idx_tabs_active ON tabs(is_active);

            CREATE TABLE IF NOT EXISTS history (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                url         TEXT NOT NULL UNIQUE,
                title       TEXT NOT NULL,
                meta        TEXT NOT NULL DEFAULT '',
                is_bookmark INTEGER NOT NULL DEFAULT 0,
                visit_count INTEGER NOT NULL DEFAULT 1,
                last_visit  INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS history_fts USING fts5(
                url,
                title,
                meta,
                content='history',
                content_rowid='id',
                tokenize='unicode61 remove_diacritics 2'
            );

            CREATE TRIGGER IF NOT EXISTS history_ai AFTER INSERT ON history BEGIN
                INSERT INTO history_fts(rowid, url, title, meta)
                VALUES (new.id, new.url, new.title, new.meta);
            END;

            CREATE TRIGGER IF NOT EXISTS history_ad AFTER DELETE ON history BEGIN
                INSERT INTO history_fts(history_fts, rowid, url, title, meta)
                VALUES ('delete', old.id, old.url, old.title, old.meta);
            END;

            CREATE TRIGGER IF NOT EXISTS history_au AFTER UPDATE ON history BEGIN
                INSERT INTO history_fts(history_fts, rowid, url, title, meta)
                VALUES ('delete', old.id, old.url, old.title, old.meta);
                INSERT INTO history_fts(rowid, url, title, meta)
                VALUES (new.id, new.url, new.title, new.meta);
            END;
            "#,
        )?;
        Ok(())
    }

    /// Migration douce si une base existante n'avait pas encore les colonnes `color` et `icon`.
    fn migrate_columns_if_needed(&self) -> Result<()> {
        let _ = self.conn.execute(
            "ALTER TABLE groups ADD COLUMN color TEXT NOT NULL DEFAULT '#2E7D47'",
            [],
        );
        let _ = self.conn.execute(
            "ALTER TABLE groups ADD COLUMN icon TEXT NOT NULL DEFAULT '◆'",
            [],
        );
        Ok(())
    }

    /// Au démarrage à froid, prépare les groupes par défaut et marque les onglets non chargés en `hibernated`.
    fn bootstrap_cold_start(&self) -> Result<()> {
        let group_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM groups", [], |r| r.get(0))?;

        if group_count == 0 {
            self.conn.execute(
                "INSERT INTO groups (name, color, icon, collapsed, position) VALUES (?1, ?2, ?3, 0, 0)",
                params!["Core / Système", "#2E7D47", "◆"],
            )?;
            let core_group_id = self.conn.last_insert_rowid();

            self.conn.execute(
                "INSERT INTO groups (name, color, icon, collapsed, position) VALUES (?1, ?2, ?3, 0, 1)",
                params!["Recherche & Docs", "#3B82F6", "◈"],
            )?;
            let research_group_id = self.conn.last_insert_rowid();

            self.conn.execute(
                "INSERT INTO tabs (group_id, url, title, scroll_pos, is_active, status)
                 VALUES (?1, 'yose://home', 'yoSE - Recherche', 0.0, 1, 'active')",
                params![core_group_id],
            )?;

            self.conn.execute(
                "INSERT INTO tabs (group_id, url, title, scroll_pos, is_active, status)
                 VALUES (?1, 'https://doc.rust-lang.org/book/', 'The Rust Programming Language', 420.0, 0, 'hibernated')",
                params![research_group_id],
            )?;
            self.conn.execute(
                "INSERT INTO tabs (group_id, url, title, scroll_pos, is_active, status)
                 VALUES (?1, 'https://sqlite.org/fts5.html', 'SQLite FTS5 Extension', 180.0, 0, 'hibernated')",
                params![research_group_id],
            )?;

            self.index_page(
                "https://doc.rust-lang.org/book/",
                "The Rust Programming Language",
                "rust book systems programming memory safety cargo",
                true,
            )?;
            self.index_page(
                "https://sqlite.org/fts5.html",
                "SQLite FTS5 Extension — Full-Text Search",
                "sqlite fts5 full text search local engine wal",
                true,
            )?;
            self.index_page(
                "https://github.com/tauri-apps/wry",
                "WRY — Cross-platform WebView rendering library in Rust",
                "wry tao webview webkitgtk wkwebview webview2 rust",
                true,
            )?;
        } else {
            // Au lancement à froid, le cache RAM des WebViews est vide : les inactifs sont 'hibernated'
            self.conn.execute(
                "UPDATE tabs SET status = 'hibernated' WHERE is_active = 0",
                [],
            )?;
            self.conn.execute(
                "UPDATE tabs SET title = 'yoSE - Recherche' WHERE url = 'yose://home'",
                [],
            )?;
        }

        Ok(())
    }

    // =========================================================================
    // GESTION & PERSONNALISATION DES GROUPES (CLIC DROIT)
    // =========================================================================

    /// Crée un nouveau groupe d'onglets avec nom, couleur et icône.
    pub fn create_group(&self, name: &str, color: &str, icon: &str) -> Result<TabGroup> {
        let next_pos: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM groups",
            [],
            |r| r.get(0),
        )?;
        self.conn.execute(
            "INSERT INTO groups (name, color, icon, collapsed, position) VALUES (?1, ?2, ?3, 0, ?4)",
            params![name, color, icon, next_pos],
        )?;
        let id = self.conn.last_insert_rowid();
        Ok(TabGroup {
            id,
            name: name.to_string(),
            color: color.to_string(),
            icon: icon.to_string(),
            collapsed: false,
            position: next_pos,
        })
    }

    /// Modifie librement un groupe (nom, couleur, icône).
    pub fn update_group(&self, group_id: i64, name: &str, color: &str, icon: &str) -> Result<()> {
        let clean_name = if name.trim().is_empty() {
            "Groupe"
        } else {
            name.trim()
        };
        let mut stmt = self.conn.prepare_cached(
            "UPDATE groups SET name = ?1, color = ?2, icon = ?3 WHERE id = ?4",
        )?;
        stmt.execute(params![clean_name, color, icon, group_id])?;
        Ok(())
    }

    /// Réordonne un groupe vers le haut (`direction = -1`) ou vers le bas (`direction = 1`).
    pub fn move_group(&self, group_id: i64, direction: i64) -> Result<()> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT id, position FROM groups ORDER BY position ASC, id ASC")?;
        let mut list: Vec<(i64, i64)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>>>()?;

        if let Some(idx) = list.iter().position(|(id, _)| *id == group_id) {
            let target_idx = idx as isize + direction as isize;
            if target_idx >= 0 && (target_idx as usize) < list.len() {
                list.swap(idx, target_idx as usize);
                let tx = self.conn.unchecked_transaction()?;
                for (new_pos, (gid, _)) in list.iter().enumerate() {
                    tx.execute(
                        "UPDATE groups SET position = ?1 WHERE id = ?2",
                        params![new_pos as i64, gid],
                    )?;
                }
                tx.commit()?;
            }
        }
        Ok(())
    }

    /// Supprime un groupe. S'il reste d'autres groupes, y transfère ou ferme les onglets selon `keep_tabs`.
    /// Retourne les IDs des onglets supprimés pour que Rust puisse purger leur cache WebView.
    pub fn delete_group(&self, group_id: i64, keep_tabs: bool) -> Result<Vec<i64>> {
        let total_groups: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM groups", [], |r| r.get(0))?;

        // On garde toujours au moins 1 groupe dans l'application
        if total_groups <= 1 {
            return Ok(Vec::new());
        }

        let fallback_group_id: i64 = self.conn.query_row(
            "SELECT id FROM groups WHERE id != ?1 ORDER BY position ASC, id ASC LIMIT 1",
            params![group_id],
            |r| r.get(0),
        )?;

        let mut removed_tab_ids = Vec::new();

        if keep_tabs {
            self.conn.execute(
                "UPDATE tabs SET group_id = ?1 WHERE group_id = ?2",
                params![fallback_group_id, group_id],
            )?;
        } else {
            let mut stmt = self
                .conn
                .prepare_cached("SELECT id FROM tabs WHERE group_id = ?1")?;
            removed_tab_ids = stmt
                .query_map(params![group_id], |r| r.get(0))?
                .collect::<Result<Vec<i64>>>()?;
        }

        self.conn
            .execute("DELETE FROM groups WHERE id = ?1", params![group_id])?;

        // Garantit qu'un onglet reste actif après la suppression
        if self.get_active_tab()?.is_none() {
            let next_tab: Option<i64> = self
                .conn
                .query_row(
                    "SELECT id FROM tabs ORDER BY updated_at DESC, id DESC LIMIT 1",
                    [],
                    |r| r.get(0),
                )
                .optional()?;

            if let Some(tid) = next_tab {
                let _ = self.activate_tab(tid, None, false)?;
            } else {
                let _ = self.create_tab(
                    fallback_group_id,
                    "yose://home",
                    "yoSE - Recherche",
                    None,
                    true,
                )?;
            }
        }

        Ok(removed_tab_ids)
    }

    /// Met tous les onglets inactifs d'un groupe en veille profonde (`hibernated`) et retourne leurs IDs.
    pub fn hibernate_group(&self, group_id: i64) -> Result<Vec<i64>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT id FROM tabs WHERE group_id = ?1 AND is_active = 0")?;
        let ids: Vec<i64> = stmt
            .query_map(params![group_id], |r| r.get(0))?
            .collect::<Result<Vec<i64>>>()?;

        self.conn.execute(
            "UPDATE tabs SET status = 'hibernated' WHERE group_id = ?1 AND is_active = 0",
            params![group_id],
        )?;
        Ok(ids)
    }

    /// Bascule l'état replié/déplié d'un groupe d'onglets.
    pub fn toggle_group_collapse(&self, group_id: i64) -> Result<()> {
        let mut stmt = self.conn.prepare_cached(
            "UPDATE groups SET collapsed = CASE WHEN collapsed = 0 THEN 1 ELSE 0 END WHERE id = ?1",
        )?;
        stmt.execute(params![group_id])?;
        Ok(())
    }

    // =========================================================================
    // GESTION DES ONGLETS (CACHE MÉMOIRE & HIBERNATION)
    // =========================================================================

    /// Crée un nouvel onglet. L'onglet précédemment actif passe en `cached` (s'il est gardé en cache)
    /// ou `hibernated`.
    pub fn create_tab(
        &self,
        group_id: i64,
        url: &str,
        title: &str,
        current_scroll: Option<f64>,
        keep_prev_cached: bool,
    ) -> Result<TabRecord> {
        let tx = self.conn.unchecked_transaction()?;

        if let Some(scroll) = current_scroll {
            tx.execute(
                "UPDATE tabs SET scroll_pos = ?1 WHERE is_active = 1",
                params![scroll],
            )?;
        }

        let prev_status = if keep_prev_cached {
            "cached"
        } else {
            "hibernated"
        };
        tx.execute(
            "UPDATE tabs SET is_active = 0, status = ?1 WHERE is_active = 1",
            params![prev_status],
        )?;

        tx.execute(
            "INSERT INTO tabs (group_id, url, title, scroll_pos, is_active, status)
             VALUES (?1, ?2, ?3, 0.0, 1, 'active')",
            params![group_id, url, title],
        )?;

        let id = tx.last_insert_rowid();
        let is_bookmarked: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM history WHERE url = ?1 AND is_bookmark = 1)",
                params![url],
                |r| r.get(0),
            )
            .unwrap_or(false);
        tx.commit()?;

        Ok(TabRecord {
            id,
            group_id,
            url: url.to_string(),
            title: title.to_string(),
            scroll_pos: 0.0,
            is_active: true,
            status: TabStatus::Active,
            is_bookmarked,
        })
    }

    /// Active un onglet cible. L'onglet précédemment actif passe en `cached` (si mis en cache RAM)
    /// sans être détruit, évitant tout rechargement réseau au prochain clic.
    pub fn activate_tab(
        &self,
        target_tab_id: i64,
        previous_scroll_pos: Option<f64>,
        keep_previous_cached: bool,
    ) -> Result<Option<TabRecord>> {
        let tx = self.conn.unchecked_transaction()?;

        if let Some(scroll) = previous_scroll_pos {
            tx.execute(
                "UPDATE tabs SET scroll_pos = ?1 WHERE is_active = 1",
                params![scroll],
            )?;
        }

        let prev_status = if keep_previous_cached {
            "cached"
        } else {
            "hibernated"
        };

        tx.execute(
            "UPDATE tabs SET is_active = 0, status = ?1 WHERE is_active = 1 AND id != ?2",
            params![prev_status, target_tab_id],
        )?;

        tx.execute(
            "UPDATE tabs SET is_active = 1, status = 'active', updated_at = strftime('%s', 'now') WHERE id = ?1",
            params![target_tab_id],
        )?;

        tx.commit()?;
        self.get_tab(target_tab_id)
    }

    /// Marque explicitement un onglet comme déchargé (`hibernated`) ou en cache (`cached`).
    pub fn set_tab_status(&self, tab_id: i64, status: TabStatus, scroll_pos: Option<f64>) -> Result<()> {
        if let Some(scroll) = scroll_pos {
            let mut stmt = self.conn.prepare_cached(
                "UPDATE tabs SET status = ?1, scroll_pos = ?2 WHERE id = ?3",
            )?;
            stmt.execute(params![status.as_str(), scroll, tab_id])?;
        } else {
            let mut stmt = self
                .conn
                .prepare_cached("UPDATE tabs SET status = ?1 WHERE id = ?2")?;
            stmt.execute(params![status.as_str(), tab_id])?;
        }
        Ok(())
    }

    /// Met à jour l'URL, le titre et la position de scroll de l'onglet actif.
    pub fn update_active_tab(&self, url: &str, title: &str, scroll_pos: f64) -> Result<()> {
        let mut stmt = self.conn.prepare_cached(
            "UPDATE tabs
             SET url = ?1, title = ?2, scroll_pos = ?3, updated_at = strftime('%s', 'now')
             WHERE is_active = 1",
        )?;
        stmt.execute(params![url, title, scroll_pos])?;
        Ok(())
    }

    /// Sauvegarde uniquement la position de défilement de l'onglet actif.
    pub fn save_active_scroll(&self, scroll_pos: f64) -> Result<()> {
        let mut stmt = self
            .conn
            .prepare_cached("UPDATE tabs SET scroll_pos = ?1 WHERE is_active = 1")?;
        stmt.execute(params![scroll_pos])?;
        Ok(())
    }

    /// Ferme un onglet. S'il était actif, réveille le dernier onglet récent ou crée `yose://home`.
    pub fn close_tab(&self, tab_id: i64) -> Result<TabRecord> {
        let mut del_stmt = self
            .conn
            .prepare_cached("DELETE FROM tabs WHERE id = ?1")?;
        del_stmt.execute(params![tab_id])?;

        if let Some(active) = self.get_active_tab()? {
            return Ok(active);
        }

        let fallback_id: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM tabs ORDER BY updated_at DESC, id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()?;

        if let Some(next_id) = fallback_id {
            if let Some(tab) = self.activate_tab(next_id, None, true)? {
                return Ok(tab);
            }
        }

        let first_group_id: i64 = self
            .conn
            .query_row("SELECT id FROM groups ORDER BY position ASC LIMIT 1", [], |r| {
                r.get(0)
            })?;
        self.create_tab(
            first_group_id,
            "yose://home",
            "yoSE - Recherche",
            None,
            true,
        )
    }

    /// Récupère un onglet par son identifiant.
    pub fn get_tab(&self, tab_id: i64) -> Result<Option<TabRecord>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT
                t.id,
                t.group_id,
                t.url,
                t.title,
                t.scroll_pos,
                t.is_active,
                t.status,
                EXISTS(SELECT 1 FROM history h WHERE h.url = t.url AND h.is_bookmark = 1)
             FROM tabs t WHERE t.id = ?1",
        )?;
        stmt.query_row(params![tab_id], |row| {
            let status_str: String = row.get(6)?;
            Ok(TabRecord {
                id: row.get(0)?,
                group_id: row.get(1)?,
                url: row.get(2)?,
                title: row.get(3)?,
                scroll_pos: row.get(4)?,
                is_active: row.get::<_, i64>(5)? != 0,
                status: TabStatus::from_db(&status_str),
                is_bookmarked: row.get(7)?,
            })
        })
        .optional()
    }

    /// Récupère l'unique onglet actif au premier plan.
    pub fn get_active_tab(&self) -> Result<Option<TabRecord>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT
                t.id,
                t.group_id,
                t.url,
                t.title,
                t.scroll_pos,
                t.is_active,
                t.status,
                EXISTS(SELECT 1 FROM history h WHERE h.url = t.url AND h.is_bookmark = 1)
             FROM tabs t WHERE t.is_active = 1 LIMIT 1",
        )?;
        stmt.query_row([], |row| {
            let status_str: String = row.get(6)?;
            Ok(TabRecord {
                id: row.get(0)?,
                group_id: row.get(1)?,
                url: row.get(2)?,
                title: row.get(3)?,
                scroll_pos: row.get(4)?,
                is_active: row.get::<_, i64>(5)? != 0,
                status: TabStatus::from_db(&status_str),
                is_bookmarked: row.get(7)?,
            })
        })
        .optional()
    }

    // =========================================================================
    // MOTEUR DE RECHERCHE LOCAL (FTS5)
    // =========================================================================

    /// Indexe ou met à jour une page visitée dans `history` + `history_fts`.
    pub fn index_page(&self, url: &str, title: &str, meta: &str, is_bookmark: bool) -> Result<()> {
        if url.starts_with("yose://") || url.is_empty() {
            return Ok(());
        }

        let mut stmt = self.conn.prepare_cached(
            r#"
            INSERT INTO history (url, title, meta, is_bookmark, visit_count, last_visit)
            VALUES (?1, ?2, ?3, ?4, 1, strftime('%s', 'now'))
            ON CONFLICT(url) DO UPDATE SET
                title = excluded.title,
                meta = CASE WHEN excluded.meta != '' THEN excluded.meta ELSE history.meta END,
                is_bookmark = MAX(history.is_bookmark, excluded.is_bookmark),
                visit_count = history.visit_count + 1,
                last_visit = strftime('%s', 'now')
            "#,
        )?;
        stmt.execute(params![url, title, meta, if is_bookmark { 1 } else { 0 }])?;
        Ok(())
    }

    /// Bascule l'état signet (bookmark) d'une URL dans l'index local.
    pub fn toggle_bookmark(&self, url: &str, title: &str) -> Result<bool> {
        let existing: Option<i64> = self
            .conn
            .query_row(
                "SELECT is_bookmark FROM history WHERE url = ?1",
                params![url],
                |r| r.get(0),
            )
            .optional()?;

        match existing {
            Some(val) => {
                let next = if val == 0 { 1 } else { 0 };
                self.conn.execute(
                    "UPDATE history SET is_bookmark = ?1 WHERE url = ?2",
                    params![next, url],
                )?;
                Ok(next == 1)
            }
            None => {
                self.index_page(url, title, "bookmark", true)?;
                Ok(true)
            }
        }
    }

    /// Ajoute explicitement un favori depuis la page `yose://bookmarks`.
    pub fn add_bookmark(&self, title: &str, raw_url: &str) -> Result<()> {
        let trimmed_url = raw_url.trim();
        if trimmed_url.is_empty() {
            return Ok(());
        }
        let normalized_url = if trimmed_url.starts_with("http://")
            || trimmed_url.starts_with("https://")
            || trimmed_url.starts_with("yose://")
        {
            trimmed_url.to_string()
        } else {
            format!("https://{}", trimmed_url)
        };
        let clean_title = if title.trim().is_empty() {
            normalized_url.as_str()
        } else {
            title.trim()
        };
        self.index_page(&normalized_url, clean_title, "bookmark", true)
    }

    /// Modifie le titre et l'URL d'un favori existant.
    pub fn update_bookmark(&self, id: i64, title: &str, raw_url: &str) -> Result<()> {
        let trimmed_url = raw_url.trim();
        if trimmed_url.is_empty() {
            return Ok(());
        }
        let normalized_url = if trimmed_url.starts_with("http://")
            || trimmed_url.starts_with("https://")
            || trimmed_url.starts_with("yose://")
        {
            trimmed_url.to_string()
        } else {
            format!("https://{}", trimmed_url)
        };
        let clean_title = if title.trim().is_empty() {
            normalized_url.as_str()
        } else {
            title.trim()
        };
        self.conn.execute(
            "UPDATE history SET title = ?1, url = ?2, is_bookmark = 1 WHERE id = ?3",
            params![clean_title, normalized_url, id],
        )?;
        Ok(())
    }

    /// Supprime un favori (retire son statut de signet).
    pub fn delete_bookmark(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE history SET is_bookmark = 0 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Liste tous les favoris enregistrés.
    pub fn list_bookmarks(&self) -> Result<Vec<BookmarkRecord>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT id, title, url, visit_count FROM history WHERE is_bookmark = 1 ORDER BY id DESC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(BookmarkRecord {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    url: row.get(2)?,
                    visit_count: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Recherche plein-texte FTS5 ultra-rapide via requête préparée en cache.
    pub fn search_fts(&self, raw_query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let cleaned = sanitize_fts_query(raw_query);
        if cleaned.is_empty() {
            return self.recent_history(limit);
        }

        let mut stmt = self.conn.prepare_cached(
            r#"
            SELECT
                h.url,
                h.title,
                snippet(history_fts, 1, '[', ']', '…', 12) AS snip,
                h.is_bookmark,
                h.visit_count,
                bm25(history_fts, 2.0, 5.0, 1.0) AS rank
            FROM history_fts
            JOIN history h ON h.id = history_fts.rowid
            WHERE history_fts MATCH ?1
            ORDER BY h.is_bookmark DESC, rank ASC, h.visit_count DESC
            LIMIT ?2
            "#,
        )?;

        let rows = stmt.query_map(params![cleaned, limit as i64], |row| {
            Ok(SearchHit {
                url: row.get(0)?,
                title: row.get(1)?,
                snippet: row.get(2)?,
                is_bookmark: row.get::<_, i64>(3)? != 0,
                visit_count: row.get(4)?,
                rank: row.get(5)?,
            })
        })?;

        let mut hits = Vec::new();
        for hit in rows {
            hits.push(hit?);
        }
        Ok(hits)
    }

    /// Vérifie si une requête correspond exactement à un signet ou une entrée locale connue.
    pub fn find_exact_local_match(&self, query: &str) -> Result<Option<String>> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(None);
        }

        let mut stmt = self.conn.prepare_cached(
            r#"
            SELECT url FROM history
            WHERE lower(title) = lower(?1)
               OR lower(url) = lower(?1)
            ORDER BY is_bookmark DESC, visit_count DESC
            LIMIT 1
            "#,
        )?;
        let exact: Option<String> = stmt.query_row(params![q], |r| r.get(0)).optional()?;

        if exact.is_some() {
            return Ok(exact);
        }

        let hits = self.search_fts(q, 1)?;
        if let Some(top) = hits.first() {
            if top.is_bookmark && top.title.to_lowercase().contains(&q.to_lowercase()) {
                return Ok(Some(top.url.clone()));
            }
        }

        Ok(None)
    }

    /// Retourne les entrées les plus pertinentes lorsque la barre de recherche est vide.
    pub fn recent_history(&self, limit: usize) -> Result<Vec<SearchHit>> {
        let mut stmt = self.conn.prepare_cached(
            r#"
            SELECT url, title, meta, is_bookmark, visit_count
            FROM history
            ORDER BY is_bookmark DESC, last_visit DESC
            LIMIT ?1
            "#,
        )?;

        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(SearchHit {
                url: row.get(0)?,
                title: row.get(1)?,
                snippet: row.get(2)?,
                is_bookmark: row.get::<_, i64>(3)? != 0,
                visit_count: row.get(4)?,
                rank: 0.0,
            })
        })?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Récupère l'état complet (groupes, onglets, statistiques) avec requêtes préparées en cache.
    pub fn snapshot(&self) -> Result<SessionSnapshot> {
        let mut group_stmt = self.conn.prepare_cached(
            "SELECT id, name, color, icon, collapsed, position FROM groups ORDER BY position ASC, id ASC",
        )?;
        let groups = group_stmt
            .query_map([], |row| {
                Ok(TabGroup {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    color: row.get(2)?,
                    icon: row.get(3)?,
                    collapsed: row.get::<_, i64>(4)? != 0,
                    position: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>>>()?;

        let mut tab_stmt = self.conn.prepare_cached(
            "SELECT
                t.id,
                t.group_id,
                t.url,
                t.title,
                t.scroll_pos,
                t.is_active,
                t.status,
                EXISTS(SELECT 1 FROM history h WHERE h.url = t.url AND h.is_bookmark = 1)
             FROM tabs t ORDER BY t.id ASC",
        )?;
        let tabs = tab_stmt
            .query_map([], |row| {
                let status_str: String = row.get(6)?;
                Ok(TabRecord {
                    id: row.get(0)?,
                    group_id: row.get(1)?,
                    url: row.get(2)?,
                    title: row.get(3)?,
                    scroll_pos: row.get(4)?,
                    is_active: row.get::<_, i64>(5)? != 0,
                    status: TabStatus::from_db(&status_str),
                    is_bookmarked: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>>>()?;

        let bookmarks = self.list_bookmarks()?;
        let stats = self.stats()?;

        Ok(SessionSnapshot {
            groups,
            tabs,
            bookmarks,
            stats,
        })
    }

    /// Calcule les métriques de la base en une seule requête SQL agrégée (ultra-rapide).
    pub fn stats(&self) -> Result<DbStats> {
        let mut stmt = self.conn.prepare_cached(
            r#"
            SELECT
                (SELECT COUNT(*) FROM tabs),
                (SELECT COUNT(*) FROM tabs WHERE status = 'cached'),
                (SELECT COUNT(*) FROM tabs WHERE status = 'hibernated'),
                (SELECT COUNT(*) FROM tabs WHERE status = 'active'),
                (SELECT COUNT(*) FROM groups),
                (SELECT COUNT(*) FROM history),
                (SELECT COUNT(*) FROM history WHERE is_bookmark = 1)
            "#,
        )?;

        let (
            total_tabs,
            cached_tabs,
            hibernated_tabs,
            active_tabs,
            total_groups,
            indexed_pages,
            bookmarks_count,
        ) = stmt.query_row([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        })?;

        Ok(DbStats {
            total_tabs,
            cached_tabs,
            hibernated_tabs,
            active_tabs,
            total_groups,
            indexed_pages,
            bookmarks_count,
            wal_mode: true,
        })
    }
}

/// Nettoie une entrée utilisateur pour construire une requête FTS5 par préfixe sûre.
fn sanitize_fts_query(input: &str) -> String {
    let tokens: Vec<String> = input
        .split_whitespace()
        .map(|tok| {
            tok.chars()
                .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                .collect::<String>()
        })
        .filter(|tok| !tok.is_empty())
        .map(|tok| format!("\"{}\"*", tok))
        .collect();

    tokens.join(" AND ")
}
