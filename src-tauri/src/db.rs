//! SQLite persistence for LocalKit (rusqlite, forward-only migrations).

use rusqlite::{params, Connection, Row};
use std::path::Path;

use crate::serverkit::ServerKitConnection;
use crate::site::Site;
use crate::sync::SyncRecord;

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create data directory: {e}"))?;
        }
        let conn = Connection::open(path).map_err(|e| format!("failed to open database: {e}"))?;
        let db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    /// Forward-only migrations tracked with `PRAGMA user_version`.
    /// Add new migrations as `if version < N { ...; PRAGMA user_version = N; }`.
    fn migrate(&self) -> Result<(), String> {
        let version: i64 = self
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|e| format!("migration check failed: {e}"))?;

        if version < 1 {
            self.conn
                .execute_batch(
                    "
                    CREATE TABLE sites (
                        id          TEXT PRIMARY KEY,
                        name        TEXT NOT NULL,
                        slug        TEXT NOT NULL UNIQUE,
                        path        TEXT NOT NULL,
                        port        INTEGER NOT NULL,
                        wp_version  TEXT NOT NULL,
                        php_version TEXT NOT NULL,
                        status      TEXT NOT NULL DEFAULT 'creating',
                        admin_user  TEXT NOT NULL DEFAULT '',
                        admin_pass  TEXT NOT NULL DEFAULT '',
                        created_at  TEXT NOT NULL
                    );
                    PRAGMA user_version = 1;
                    ",
                )
                .map_err(|e| format!("migration 1 failed: {e}"))?;
        }
        if version < 2 {
            // NOTE: API keys are stored in plaintext in the local SQLite DB —
            // acceptable for v1 (documented); a keyring migration can come later.
            self.conn
                .execute_batch(
                    "
                    CREATE TABLE serverkit_connections (
                        id         TEXT PRIMARY KEY,
                        label      TEXT NOT NULL,
                        url        TEXT NOT NULL,
                        api_key    TEXT NOT NULL,
                        created_at TEXT NOT NULL
                    );
                    PRAGMA user_version = 2;
                    ",
                )
                .map_err(|e| format!("migration 2 failed: {e}"))?;
        }
        if version < 3 {
            self.conn
                .execute_batch(
                    "
                    CREATE TABLE sync_history (
                        id            TEXT PRIMARY KEY,
                        site_id       TEXT NOT NULL,
                        connection_id TEXT NOT NULL,
                        direction     TEXT NOT NULL,
                        kind          TEXT NOT NULL,
                        status        TEXT NOT NULL,
                        message       TEXT NOT NULL DEFAULT '',
                        created_at    TEXT NOT NULL
                    );
                    PRAGMA user_version = 3;
                    ",
                )
                .map_err(|e| format!("migration 3 failed: {e}"))?;
        }
        if version < 4 {
            // M6 local domains: key-value app settings (domains_enabled,
            // router_ca_trusted, router_last_error).
            self.conn
                .execute_batch(
                    "
                    CREATE TABLE app_settings (
                        key   TEXT PRIMARY KEY,
                        value TEXT NOT NULL
                    );
                    PRAGMA user_version = 4;
                    ",
                )
                .map_err(|e| format!("migration 4 failed: {e}"))?;
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // App settings (M6 local domains)
    // -----------------------------------------------------------------------

    pub fn get_setting(&self, key: &str) -> Result<Option<String>, String> {
        self.conn
            .query_row(
                "SELECT value FROM app_settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(format!("failed to read setting {key}: {other}")),
            })
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO app_settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map_err(|e| format!("failed to write setting {key}: {e}"))?;
        Ok(())
    }

    fn row_to_site(row: &Row) -> rusqlite::Result<Site> {
        Ok(Site {
            id: row.get("id")?,
            name: row.get("name")?,
            slug: row.get("slug")?,
            path: row.get("path")?,
            port: row.get::<_, u32>("port")? as u16,
            wp_version: row.get("wp_version")?,
            php_version: row.get("php_version")?,
            status: row.get("status")?,
            admin_user: row.get("admin_user")?,
            admin_pass: row.get("admin_pass")?,
            created_at: row.get("created_at")?,
        })
    }

    pub fn insert_site(&self, site: &Site) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO sites
                 (id, name, slug, path, port, wp_version, php_version, status, admin_user, admin_pass, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    site.id,
                    site.name,
                    site.slug,
                    site.path,
                    site.port as u32,
                    site.wp_version,
                    site.php_version,
                    site.status,
                    site.admin_user,
                    site.admin_pass,
                    site.created_at,
                ],
            )
            .map_err(|e| format!("failed to insert site: {e}"))?;
        Ok(())
    }

    pub fn set_status(&self, id: &str, status: &str) -> Result<(), String> {
        self.conn
            .execute("UPDATE sites SET status = ?1 WHERE id = ?2", params![status, id])
            .map_err(|e| format!("failed to update site status: {e}"))?;
        Ok(())
    }

    pub fn update_credentials(&self, id: &str, user: &str, pass: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE sites SET admin_user = ?1, admin_pass = ?2 WHERE id = ?3",
                params![user, pass, id],
            )
            .map_err(|e| format!("failed to update site credentials: {e}"))?;
        Ok(())
    }

    pub fn delete_site(&self, id: &str) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM sites WHERE id = ?1", params![id])
            .map_err(|e| format!("failed to delete site: {e}"))?;
        Ok(())
    }

    pub fn get_site(&self, id: &str) -> Result<Site, String> {
        self.conn
            .query_row("SELECT * FROM sites WHERE id = ?1", params![id], Self::row_to_site)
            .map_err(|_| "site not found".to_string())
    }

    pub fn list_sites(&self) -> Result<Vec<Site>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM sites ORDER BY created_at ASC")
            .map_err(|e| format!("failed to list sites: {e}"))?;
        let rows = stmt
            .query_map([], Self::row_to_site)
            .map_err(|e| format!("failed to list sites: {e}"))?;
        let mut sites = Vec::new();
        for row in rows {
            sites.push(row.map_err(|e| format!("failed to read site row: {e}"))?);
        }
        Ok(sites)
    }

    pub fn slug_exists(&self, slug: &str) -> Result<bool, String> {
        let count: i64 = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM sites WHERE slug = ?1",
                params![slug],
                |row| row.get(0),
            )
            .map_err(|e| format!("failed to check slug: {e}"))?;
        Ok(count > 0)
    }

    pub fn used_ports(&self) -> Result<Vec<u16>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT port FROM sites")
            .map_err(|e| format!("failed to read ports: {e}"))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, u32>(0))
            .map_err(|e| format!("failed to read ports: {e}"))?;
        let mut ports = Vec::new();
        for row in rows {
            ports.push(row.map_err(|e| format!("failed to read port: {e}"))? as u16);
        }
        Ok(ports)
    }

    // -----------------------------------------------------------------------
    // ServerKit connections
    // -----------------------------------------------------------------------

    fn row_to_connection(row: &Row) -> rusqlite::Result<ServerKitConnection> {
        Ok(ServerKitConnection {
            id: row.get("id")?,
            label: row.get("label")?,
            url: row.get("url")?,
            api_key: row.get("api_key")?,
            created_at: row.get("created_at")?,
        })
    }

    pub fn insert_connection(&self, conn: &ServerKitConnection) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO serverkit_connections (id, label, url, api_key, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![conn.id, conn.label, conn.url, conn.api_key, conn.created_at],
            )
            .map_err(|e| format!("failed to insert connection: {e}"))?;
        Ok(())
    }

    pub fn get_connection(&self, id: &str) -> Result<ServerKitConnection, String> {
        self.conn
            .query_row(
                "SELECT * FROM serverkit_connections WHERE id = ?1",
                params![id],
                Self::row_to_connection,
            )
            .map_err(|_| "connection not found".to_string())
    }

    pub fn list_connections(&self) -> Result<Vec<ServerKitConnection>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM serverkit_connections ORDER BY created_at ASC")
            .map_err(|e| format!("failed to list connections: {e}"))?;
        let rows = stmt
            .query_map([], Self::row_to_connection)
            .map_err(|e| format!("failed to list connections: {e}"))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| format!("failed to read connection row: {e}"))?);
        }
        Ok(out)
    }

    pub fn delete_connection(&self, id: &str) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM serverkit_connections WHERE id = ?1", params![id])
            .map_err(|e| format!("failed to delete connection: {e}"))?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Sync history
    // -----------------------------------------------------------------------

    pub fn insert_sync(&self, rec: &SyncRecord) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO sync_history
                 (id, site_id, connection_id, direction, kind, status, message, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    rec.id,
                    rec.site_id,
                    rec.connection_id,
                    rec.direction,
                    rec.kind,
                    rec.status,
                    rec.message,
                    rec.created_at,
                ],
            )
            .map_err(|e| format!("failed to insert sync record: {e}"))?;
        Ok(())
    }

    pub fn list_sync(&self, site_id: &str, limit: u32) -> Result<Vec<SyncRecord>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT * FROM sync_history WHERE site_id = ?1
                 ORDER BY created_at DESC LIMIT ?2",
            )
            .map_err(|e| format!("failed to list sync history: {e}"))?;
        let rows = stmt
            .query_map(params![site_id, limit], |row| {
                Ok(SyncRecord {
                    id: row.get("id")?,
                    site_id: row.get("site_id")?,
                    connection_id: row.get("connection_id")?,
                    direction: row.get("direction")?,
                    kind: row.get("kind")?,
                    status: row.get("status")?,
                    message: row.get("message")?,
                    created_at: row.get("created_at")?,
                })
            })
            .map_err(|e| format!("failed to list sync history: {e}"))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| format!("failed to read sync row: {e}"))?);
        }
        Ok(out)
    }
}
