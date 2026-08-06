use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;

use crate::error::{JarvisError, Result};
use crate::paths::VaultPaths;
use crate::playbook::{hash_content, load_playbook, Playbook};
use crate::types::{ObservedEvent, PatternRecord, RunRecord, TrustGrant, TrustLevel};

pub struct Vault {
    conn: Arc<Mutex<Connection>>,
    pub paths: VaultPaths,
}

impl Vault {
    pub fn open(paths: VaultPaths) -> Result<Self> {
        paths.ensure_layout()?;
        let conn = Connection::open(paths.database())?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA foreign_keys=ON;",
        )?;
        let vault = Self {
            conn: Arc::new(Mutex::new(conn)),
            paths,
        };
        vault.migrate()?;
        Ok(vault)
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY,
                ts TEXT NOT NULL,
                source TEXT NOT NULL,
                kind TEXT NOT NULL,
                payload_json TEXT NOT NULL,
                session_id TEXT,
                redacted INTEGER DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS playbooks (
                id TEXT NOT NULL,
                version INTEGER NOT NULL,
                content_hash TEXT NOT NULL,
                yaml_path TEXT NOT NULL,
                created_at TEXT NOT NULL,
                source TEXT DEFAULT 'user',
                PRIMARY KEY (id, version)
            );

            CREATE TABLE IF NOT EXISTS runs (
                id TEXT PRIMARY KEY,
                playbook_id TEXT NOT NULL,
                playbook_ver INTEGER NOT NULL,
                status TEXT NOT NULL,
                trigger TEXT NOT NULL,
                phrase TEXT,
                started_at TEXT,
                finished_at TEXT,
                error TEXT
            );

            CREATE TABLE IF NOT EXISTS run_steps (
                id INTEGER PRIMARY KEY,
                run_id TEXT NOT NULL,
                step_id TEXT NOT NULL,
                trust_level TEXT NOT NULL,
                status TEXT NOT NULL,
                input_json TEXT,
                output_json TEXT,
                started_at TEXT,
                finished_at TEXT,
                FOREIGN KEY(run_id) REFERENCES runs(id)
            );

            CREATE TABLE IF NOT EXISTS trust_grants (
                playbook_id TEXT NOT NULL,
                step_id TEXT NOT NULL,
                level TEXT NOT NULL,
                granted_at TEXT NOT NULL,
                PRIMARY KEY (playbook_id, step_id)
            );

            CREATE TABLE IF NOT EXISTS patterns (
                id TEXT PRIMARY KEY,
                fingerprint TEXT NOT NULL,
                event_ids TEXT NOT NULL,
                suggested_name TEXT,
                confidence REAL,
                status TEXT DEFAULT 'pending',
                command_sequence TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_events_ts ON events(ts);
            CREATE INDEX IF NOT EXISTS idx_runs_started ON runs(started_at);
            ",
        )?;
        Ok(())
    }

    pub fn sync_playbooks(&self) -> Result<Vec<Playbook>> {
        let dir = self.paths.playbooks();
        let playbooks = crate::playbook::load_playbooks_from_dir(&dir)?;
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        for pb in &playbooks {
            conn.execute(
                "INSERT INTO playbooks (id, version, content_hash, yaml_path, created_at, source)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'user')
                 ON CONFLICT(id, version) DO UPDATE SET
                   content_hash=excluded.content_hash,
                   yaml_path=excluded.yaml_path",
                params![
                    pb.id(),
                    pb.version,
                    pb.content_hash,
                    pb.path.display().to_string(),
                    now,
                ],
            )?;
        }
        Ok(playbooks)
    }

    pub fn get_playbook(&self, id: &str) -> Result<Playbook> {
        let conn = self.conn.lock().unwrap();
        let path: String = conn
            .query_row(
                "SELECT yaml_path FROM playbooks WHERE id = ?1 ORDER BY version DESC LIMIT 1",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| JarvisError::Playbook(format!("playbook not found: {id}")))?;
        load_playbook(Path::new(&path))
    }

    pub fn list_playbook_ids(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT DISTINCT id FROM playbooks ORDER BY id",
        )?;
        let rows = stmt
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<Vec<String>, _>>()?;
        Ok(rows)
    }

    pub fn find_playbook_by_phrase(&self, phrase: &str) -> Result<Option<Playbook>> {
        for id in self.list_playbook_ids()? {
            let pb = self.get_playbook(&id)?;
            if pb.matches_phrase(phrase) {
                return Ok(Some(pb));
            }
        }
        Ok(None)
    }

    pub fn insert_event(&self, event: &ObservedEvent) -> Result<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO events (ts, source, kind, payload_json, session_id, redacted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                event.ts,
                event.source,
                event.kind,
                serde_json::to_string(&event.payload)?,
                event.session_id,
                event.redacted as i64,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn list_events_since(&self, since: &str, limit: usize) -> Result<Vec<(i64, ObservedEvent)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, ts, source, kind, payload_json, session_id, redacted
             FROM events WHERE ts >= ?1 ORDER BY ts ASC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![since, limit as i64], |row| {
                let payload: String = row.get(4)?;
                Ok((
                    row.get(0)?,
                    ObservedEvent {
                        ts: row.get(1)?,
                        source: row.get(2)?,
                        kind: row.get(3)?,
                        payload: serde_json::from_str(&payload).unwrap_or(json!({})),
                        session_id: row.get(5)?,
                        redacted: row.get::<_, i64>(6)? != 0,
                    },
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn prune_events(&self, retention_days: u32) -> Result<usize> {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);
        let conn = self.conn.lock().unwrap();
        let deleted = conn.execute(
            "DELETE FROM events WHERE ts < ?1",
            params![cutoff.to_rfc3339()],
        )?;
        Ok(deleted)
    }

    pub fn create_run(
        &self,
        id: &str,
        playbook_id: &str,
        version: i64,
        trigger: &str,
        phrase: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO runs (id, playbook_id, playbook_ver, status, trigger, phrase, started_at)
             VALUES (?1, ?2, ?3, 'pending', ?4, ?5, ?6)",
            params![
                id,
                playbook_id,
                version,
                trigger,
                phrase,
                chrono::Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn update_run_status(
        &self,
        id: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE runs SET status = ?1, finished_at = ?2, error = ?3 WHERE id = ?4",
            params![status, chrono::Utc::now().to_rfc3339(), error, id],
        )?;
        Ok(())
    }

    pub fn insert_run_step(
        &self,
        run_id: &str,
        step_id: &str,
        trust_level: &str,
        status: &str,
        input: Option<&serde_json::Value>,
        output: Option<&serde_json::Value>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO run_steps (run_id, step_id, trust_level, status, input_json, output_json, started_at, finished_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                run_id,
                step_id,
                trust_level,
                status,
                input.map(serde_json::to_string).transpose()?,
                output.map(serde_json::to_string).transpose()?,
                chrono::Utc::now().to_rfc3339(),
                chrono::Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn list_runs(&self, limit: usize) -> Result<Vec<RunRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, playbook_id, playbook_ver, status, trigger, phrase, started_at, finished_at, error
             FROM runs ORDER BY started_at DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit as i64], |row| {
                Ok(RunRecord {
                    id: row.get(0)?,
                    playbook_id: row.get(1)?,
                    playbook_ver: row.get(2)?,
                    status: row.get(3)?,
                    trigger: row.get(4)?,
                    phrase: row.get(5)?,
                    started_at: row.get(6)?,
                    finished_at: row.get(7)?,
                    error: row.get(8)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn get_run(&self, id: &str) -> Result<RunRecord> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, playbook_id, playbook_ver, status, trigger, phrase, started_at, finished_at, error
             FROM runs WHERE id = ?1",
            params![id],
            |row| {
                Ok(RunRecord {
                    id: row.get(0)?,
                    playbook_id: row.get(1)?,
                    playbook_ver: row.get(2)?,
                    status: row.get(3)?,
                    trigger: row.get(4)?,
                    phrase: row.get(5)?,
                    started_at: row.get(6)?,
                    finished_at: row.get(7)?,
                    error: row.get(8)?,
                })
            },
        )
        .map_err(|e| JarvisError::Vault(format!("run {id}: {e}")))
    }

    pub fn set_trust_grant(&self, grant: &TrustGrant) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO trust_grants (playbook_id, step_id, level, granted_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(playbook_id, step_id) DO UPDATE SET
               level = excluded.level,
               granted_at = excluded.granted_at",
            params![
                grant.playbook_id,
                grant.step_id,
                grant.level.to_string(),
                grant.granted_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_trust_grant(&self, playbook_id: &str, step_id: &str) -> Result<TrustGrant> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT playbook_id, step_id, level, granted_at FROM trust_grants
             WHERE playbook_id = ?1 AND step_id = ?2",
            params![playbook_id, step_id],
            |row| {
                let level: String = row.get(2)?;
                Ok(TrustGrant {
                    playbook_id: row.get(0)?,
                    step_id: row.get(1)?,
                    level: level.parse().map_err(|e: String| {
                        rusqlite::Error::InvalidParameterName(e)
                    })?,
                    granted_at: row.get(3)?,
                })
            },
        )
        .map_err(|e| JarvisError::Vault(format!("trust grant: {e}")))
    }

    pub fn upsert_pattern(&self, pattern: &PatternRecord) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO patterns (id, fingerprint, event_ids, suggested_name, confidence, status, command_sequence)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(id) DO UPDATE SET
               suggested_name = excluded.suggested_name,
               confidence = excluded.confidence,
               status = excluded.status,
               command_sequence = excluded.command_sequence",
            params![
                pattern.id,
                pattern.fingerprint,
                serde_json::to_string(&pattern.event_ids)?,
                pattern.suggested_name,
                pattern.confidence,
                pattern.status,
                serde_json::to_string(&pattern.command_sequence)?,
            ],
        )?;
        Ok(())
    }

    pub fn list_patterns(&self, status: Option<&str>) -> Result<Vec<PatternRecord>> {
        let conn = self.conn.lock().unwrap();
        let query = if status.is_some() {
            "SELECT id, fingerprint, event_ids, suggested_name, confidence, status, command_sequence
             FROM patterns WHERE status = ?1 ORDER BY confidence DESC"
        } else {
            "SELECT id, fingerprint, event_ids, suggested_name, confidence, status, command_sequence
             FROM patterns ORDER BY confidence DESC"
        };
        let mut stmt = conn.prepare(query)?;
        let map_row = |row: &rusqlite::Row<'_>| {
            let event_ids: String = row.get(2)?;
            let command_sequence: String = row.get(6)?;
            Ok(PatternRecord {
                id: row.get(0)?,
                fingerprint: row.get(1)?,
                event_ids: serde_json::from_str(&event_ids).unwrap_or_default(),
                suggested_name: row.get(3)?,
                confidence: row.get(4)?,
                status: row.get(5)?,
                command_sequence: serde_json::from_str(&command_sequence).unwrap_or_default(),
            })
        };
        let rows = if let Some(status) = status {
            stmt.query_map(params![status], map_row)?
                .collect::<std::result::Result<Vec<_>, _>>()?
        } else {
            stmt.query_map([], map_row)?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        Ok(rows)
    }

    pub fn update_pattern_status(&self, id: &str, status: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE patterns SET status = ?1 WHERE id = ?2",
            params![status, id],
        )?;
        Ok(())
    }

    pub fn save_playbook_file(&self, playbook: &Playbook) -> Result<PathBuf> {
        let path = self.paths.playbooks().join(format!("{}.yaml", playbook.id()));
        let yaml = crate::playbook::playbook_to_yaml(&playbook.doc)?;
        std::fs::write(&path, &yaml)?;
        let content_hash = hash_content(&yaml);
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO playbooks (id, version, content_hash, yaml_path, created_at, source)
             VALUES (?1, ?2, ?3, ?4, ?5, 'suggested')",
            params![
                playbook.id(),
                playbook.version + 1,
                content_hash,
                path.display().to_string(),
                chrono::Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(path)
    }
}
