//! The installed-package database (`tlpkg/mtx/installed.sqlite`) and
//! small persistent state (pinned mirror, freshness timestamps).

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

/// Why a package is installed; `Auto` packages are candidates for cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Bootstrap,
    Explicit,
    Auto,
    Dependency,
    /// Upgrade of an installed package: keep its recorded reason.
    Upgrade,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Reason::Bootstrap => "bootstrap",
            Reason::Explicit => "explicit",
            Reason::Auto => "auto",
            Reason::Dependency => "dependency",
            Reason::Upgrade => "auto",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Installed {
    pub name: String,
    pub revision: u64,
    pub reason: String,
    pub installed_at: u64,
}

pub struct Db {
    conn: Connection,
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO).as_secs()
}

impl Db {
    pub fn open(path: &Path) -> Result<Db> {
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        conn.busy_timeout(Duration::from_secs(60))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS packages(
                 name TEXT PRIMARY KEY,
                 revision INTEGER NOT NULL,
                 reason TEXT NOT NULL,
                 installed_at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS files(
                 path TEXT NOT NULL,
                 package TEXT NOT NULL,
                 PRIMARY KEY(path, package));
             CREATE INDEX IF NOT EXISTS files_by_package ON files(package);
             CREATE TABLE IF NOT EXISTS kv(key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )?;
        Ok(Db { conn })
    }

    pub fn installed(&self) -> Result<BTreeMap<String, Installed>> {
        let mut stmt = self.conn.prepare("SELECT name, revision, reason, installed_at FROM packages")?;
        let rows = stmt.query_map([], |r| {
            Ok(Installed {
                name: r.get(0)?,
                revision: r.get::<_, i64>(1)? as u64,
                reason: r.get(2)?,
                installed_at: r.get::<_, i64>(3)? as u64,
            })
        })?;
        let mut out = BTreeMap::new();
        for r in rows {
            let r = r?;
            out.insert(r.name.clone(), r);
        }
        Ok(out)
    }

    pub fn files_of(&self, package: &str) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT path FROM files WHERE package = ?1 ORDER BY path")?;
        let rows = stmt.query_map([package], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Other packages that also own `path` (shared files, e.g. identical .enc files).
    pub fn other_owners(&self, path: &str, package: &str) -> Result<usize> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM files WHERE path = ?1 AND package != ?2",
            params![path, package],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }

    /// Record `package` as installed with exactly `files`, replacing any
    /// previous record. An `Auto`/`Dependency` reinstall keeps a stronger
    /// existing reason.
    pub fn record(&mut self, package: &str, revision: u64, reason: Reason, files: &[String]) -> Result<()> {
        let tx = self.conn.transaction()?;
        let old_reason: Option<String> =
            tx.query_row("SELECT reason FROM packages WHERE name = ?1", [package], |r| r.get(0)).optional()?;
        let reason = match (old_reason.as_deref(), reason) {
            (Some(old @ ("bootstrap" | "explicit")), Reason::Auto | Reason::Dependency) => old.to_string(),
            (Some(old), Reason::Upgrade) => old.to_string(),
            _ => reason.as_str().to_string(),
        };
        tx.execute(
            "INSERT OR REPLACE INTO packages(name, revision, reason, installed_at) VALUES (?1, ?2, ?3, ?4)",
            params![package, revision as i64, reason, now_secs() as i64],
        )?;
        tx.execute("DELETE FROM files WHERE package = ?1", [package])?;
        {
            let mut ins = tx.prepare("INSERT OR IGNORE INTO files(path, package) VALUES (?1, ?2)")?;
            for f in files {
                ins.execute(params![f, package])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn forget(&mut self, package: &str) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM packages WHERE name = ?1", [package])?;
        tx.execute("DELETE FROM files WHERE package = ?1", [package])?;
        tx.commit()?;
        Ok(())
    }

    pub fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT value FROM kv WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute("INSERT OR REPLACE INTO kv(key, value) VALUES (?1, ?2)", params![key, value])?;
        Ok(())
    }

    pub fn get_u64(&self, key: &str) -> Result<Option<u64>> {
        Ok(self.get(key)?.and_then(|v| v.parse().ok()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_replace_and_reason_upgrade_rules() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = Db::open(&dir.path().join("i.sqlite")).unwrap();
        db.record("amsmath", 1, Reason::Explicit, &["a".into(), "b".into()]).unwrap();
        db.record("amsmath", 2, Reason::Auto, &["b".into(), "c".into()]).unwrap();
        let inst = db.installed().unwrap();
        assert_eq!(inst["amsmath"].revision, 2);
        assert_eq!(inst["amsmath"].reason, "explicit"); // not downgraded to auto
        assert_eq!(db.files_of("amsmath").unwrap(), vec!["b", "c"]);

        db.record("other", 1, Reason::Auto, &["c".into()]).unwrap();
        assert_eq!(db.other_owners("c", "amsmath").unwrap(), 1);
        db.forget("other").unwrap();
        assert_eq!(db.other_owners("c", "amsmath").unwrap(), 0);

        db.record("dep", 1, Reason::Dependency, &[]).unwrap();
        db.record("dep", 2, Reason::Upgrade, &[]).unwrap();
        assert_eq!(db.installed().unwrap()["dep"].reason, "dependency"); // upgrades keep the reason

        db.set("mirror", "https://example/").unwrap();
        assert_eq!(db.get("mirror").unwrap().as_deref(), Some("https://example/"));
        assert_eq!(db.get_u64("missing").unwrap(), None);
    }
}
