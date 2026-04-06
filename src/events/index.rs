use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::Path;

pub struct LocalIndex {
    conn: Connection,
}

impl LocalIndex {
    pub fn new<P: AsRef<Path>>(nancy_dir: P) -> Result<Self> {
        let db_path = nancy_dir.as_ref().join("index.sqlite");
        let conn = Connection::open(db_path)?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS events (
                id TEXT PRIMARY KEY,
                did TEXT NOT NULL,
                log_file TEXT NOT NULL,
                line_index INTEGER NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS branch_sync_state (
                did TEXT PRIMARY KEY,
                commit_hash TEXT NOT NULL
            )",
            [],
        )?;

        Ok(LocalIndex { conn })
    }

    pub fn insert_event(
        &self,
        id: &str,
        did: &str,
        log_file: &str,
        line_index: usize,
    ) -> Result<()> {
        let line_index = line_index as i64;
        self.conn.execute(
            "INSERT OR IGNORE INTO events (id, did, log_file, line_index) 
             VALUES (?1, ?2, ?3, ?4)",
            params![id, did, log_file, line_index],
        )?;
        Ok(())
    }

    pub fn lookup_event(&self, id: &str) -> Result<Option<(String, String, usize)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT did, log_file, line_index FROM events WHERE id = ?1")?;
            
        let mut rows = stmt.query(params![id])?;

        if let Some(row) = rows.next()? {
            let did: String = row.get(0)?;
            let log_file: String = row.get(1)?;
            let line_index: usize = row.get::<_, i64>(2)? as usize;
            Ok(Some((did, log_file, line_index)))
        } else {
            Ok(None)
        }
    }

    pub fn get_branch_commit(&self, did: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT commit_hash FROM branch_sync_state WHERE did = ?1")?;
        
        let mut rows = stmt.query(params![did])?;
        if let Some(row) = rows.next()? {
            let commit_hash: String = row.get(0)?;
            Ok(Some(commit_hash))
        } else {
            Ok(None)
        }
    }

    pub fn set_branch_commit(&self, did: &str, commit_hash: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO branch_sync_state (did, commit_hash) 
             VALUES (?1, ?2) 
             ON CONFLICT(did) DO UPDATE SET commit_hash=excluded.commit_hash",
            params![did, commit_hash],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_local_index() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let index = LocalIndex::new(temp_dir.path())?;

        index.insert_event("hash123", "did:test", "00001.log", 42)?;
        index.insert_event("hash456", "did:test", "00002.log", 7)?;

        let res1 = index.lookup_event("hash123")?.unwrap();
        assert_eq!(res1.0, "did:test");
        assert_eq!(res1.1, "00001.log");
        assert_eq!(res1.2, 42);

        let res2 = index.lookup_event("hash456")?.unwrap();
        assert_eq!(res2.1, "00002.log");
        assert_eq!(res2.2, 7);

        let miss = index.lookup_event("unknown")?;
        assert!(miss.is_none());

        Ok(())
    }

    #[test]
    fn test_branch_sync_state() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let index = LocalIndex::new(temp_dir.path())?;

        let res = index.get_branch_commit("did:test")?;
        assert!(res.is_none());

        index.set_branch_commit("did:test", "commit1")?;
        let res = index.get_branch_commit("did:test")?.unwrap();
        assert_eq!(res, "commit1");

        index.set_branch_commit("did:test", "commit2")?;
        let res = index.get_branch_commit("did:test")?.unwrap();
        assert_eq!(res, "commit2");

        Ok(())
    }
}
