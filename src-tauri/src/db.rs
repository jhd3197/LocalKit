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
        if version < 5 {
            // Plan 18: where a site came from. Set on sites created by an
            // import; NULL on every hand-made site, which is why both columns
            // are nullable rather than defaulted.
            self.conn
                .execute_batch(
                    "
                    ALTER TABLE sites ADD COLUMN connection_id  TEXT;
                    ALTER TABLE sites ADD COLUMN remote_site_id INTEGER;
                    PRAGMA user_version = 5;
                    ",
                )
                .map_err(|e| format!("migration 5 failed: {e}"))?;
        }
        if version < 6 {
            // Plan 22: the stack kind + its per-kind settings. Constant defaults
            // migrate every existing row to the WordPress stack it already is —
            // `config_json = '{}'` deserializes to the WordPress `SiteConfig`
            // defaults (service `wordpress`, sync path `wp-content`).
            self.conn
                .execute_batch(
                    "
                    ALTER TABLE sites ADD COLUMN kind        TEXT NOT NULL DEFAULT 'wordpress';
                    ALTER TABLE sites ADD COLUMN config_json TEXT NOT NULL DEFAULT '{}';
                    PRAGMA user_version = 6;
                    ",
                )
                .map_err(|e| format!("migration 6 failed: {e}"))?;
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

    pub fn get_all_settings(&self) -> Result<std::collections::HashMap<String, String>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT key, value FROM app_settings")
            .map_err(|e| format!("failed to read settings: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| format!("failed to read settings: {e}"))?;
        let mut map = std::collections::HashMap::new();
        for row in rows {
            let (k, v) = row.map_err(|e| format!("failed to read settings: {e}"))?;
            map.insert(k, v);
        }
        Ok(map)
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

    pub fn delete_setting(&self, key: &str) -> Result<(), String> {
        self.conn
            .execute("DELETE FROM app_settings WHERE key = ?1", params![key])
            .map_err(|e| format!("failed to delete setting {key}: {e}"))?;
        Ok(())
    }

    fn row_to_site(row: &Row) -> rusqlite::Result<Site> {
        // `config_json` parses to the WordPress defaults when empty/`{}` or
        // unreadable, so a legacy row is always the fully-capable WP stack.
        let config_json: String = row.get("config_json")?;
        let config: crate::site::SiteConfig =
            serde_json::from_str(&config_json).unwrap_or_default();
        let kind: String = row.get("kind")?;
        let mut site = Site {
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
            connection_id: row.get("connection_id")?,
            remote_site_id: row.get("remote_site_id")?,
            kind,
            config,
            capabilities: crate::site::Capabilities::default(),
        };
        site.refresh_capabilities();
        Ok(site)
    }

    pub fn insert_site(&self, site: &Site) -> Result<(), String> {
        // The derived `capabilities` field is never persisted — it is
        // recomputed from `kind`/`config` on every read.
        let config_json = serde_json::to_string(&site.config)
            .map_err(|e| format!("failed to serialize site config: {e}"))?;
        self.conn
            .execute(
                "INSERT INTO sites
                 (id, name, slug, path, port, wp_version, php_version, status, admin_user, admin_pass,
                  created_at, connection_id, remote_site_id, kind, config_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
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
                    site.connection_id,
                    site.remote_site_id,
                    site.kind,
                    config_json,
                ],
            )
            .map_err(|e| format!("failed to insert site: {e}"))?;
        Ok(())
    }

    /// Sites imported from a given remote site (plan 18) — the `pre_import`
    /// guard's "you already have a copy of this" check.
    pub fn sites_from_remote(
        &self,
        connection_id: &str,
        remote_site_id: i64,
    ) -> Result<Vec<Site>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT * FROM sites WHERE connection_id = ?1 AND remote_site_id = ?2
                 ORDER BY created_at ASC",
            )
            .map_err(|e| format!("failed to look up imported sites: {e}"))?;
        let rows = stmt
            .query_map(params![connection_id, remote_site_id], Self::row_to_site)
            .map_err(|e| format!("failed to look up imported sites: {e}"))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row.map_err(|e| format!("failed to read site row: {e}"))?);
        }
        Ok(out)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!("localkit-dbtest-{}-{tag}", std::process::id()))
            .join("localkit.db")
    }

    fn site(id: &str, slug: &str) -> Site {
        let mut s = Site {
            id: id.into(),
            name: slug.into(),
            slug: slug.into(),
            path: format!("/tmp/{slug}"),
            port: 8081,
            wp_version: "6.7".into(),
            php_version: "8.3".into(),
            status: "running".into(),
            admin_user: "admin".into(),
            admin_pass: "secret".into(),
            created_at: "2026-07-20T00:00:00Z".into(),
            connection_id: None,
            remote_site_id: None,
            kind: crate::site::KIND_WORDPRESS.into(),
            config: crate::site::SiteConfig::default(),
            capabilities: crate::site::Capabilities::default(),
        };
        s.refresh_capabilities();
        s
    }

    /// The pre-plan-18 schema, verbatim: a database created by the shipped
    /// app before migration 5 existed. Migrating this is the upgrade path
    /// every existing user takes, so it is what the test actually exercises —
    /// a freshly created database would prove nothing about ALTER TABLE.
    fn seed_v4(path: &std::path::Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
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
            CREATE TABLE serverkit_connections (
                id TEXT PRIMARY KEY, label TEXT NOT NULL, url TEXT NOT NULL,
                api_key TEXT NOT NULL, created_at TEXT NOT NULL
            );
            CREATE TABLE sync_history (
                id TEXT PRIMARY KEY, site_id TEXT NOT NULL, connection_id TEXT NOT NULL,
                direction TEXT NOT NULL, kind TEXT NOT NULL, status TEXT NOT NULL,
                message TEXT NOT NULL DEFAULT '', created_at TEXT NOT NULL
            );
            CREATE TABLE app_settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            INSERT INTO sites (id, name, slug, path, port, wp_version, php_version, status,
                               admin_user, admin_pass, created_at)
            VALUES ('old-1', 'Legacy', 'legacy', '/tmp/legacy', 8081, '6.7', '8.3', 'running',
                    'admin', 'pw', '2026-01-01T00:00:00Z');
            PRAGMA user_version = 4;
            ",
        )
        .unwrap();
    }

    #[test]
    fn migrations_upgrade_a_v4_database_without_touching_existing_rows() {
        let path = temp_db_path("v4");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        seed_v4(&path);

        let db = Db::open(&path).unwrap();
        let version: i64 = db
            .conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, 6);

        // The pre-existing site survives, reads back with a NULL origin, and —
        // crucially for plan 22 — migrates to the fully-capable WordPress stack:
        // kind `wordpress`, the WordPress `SiteConfig` defaults, all caps true.
        let legacy = db.get_site("old-1").unwrap();
        assert_eq!(legacy.slug, "legacy");
        assert_eq!(legacy.connection_id, None);
        assert_eq!(legacy.remote_site_id, None);
        assert_eq!(legacy.kind, crate::site::KIND_WORDPRESS);
        assert_eq!(legacy.config, crate::site::SiteConfig::default());
        assert_eq!(legacy.config.service, "wordpress");
        assert_eq!(legacy.config.sync_path, "wp-content");
        assert_eq!(legacy.capabilities, crate::site::Capabilities::WORDPRESS);

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn docker_kind_round_trips_config_and_derives_capabilities() {
        let path = temp_db_path("docker-kind");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        let db = Db::open(&path).unwrap();

        let mut app = site("d-1", "my-api");
        app.kind = crate::site::KIND_DOCKER.into();
        app.config = crate::site::SiteConfig {
            service: "app".into(),
            sync_path: ".".into(),
            app_port: Some(3000),
            db_engine: Some("postgres".into()),
            db_service: Some("db".into()),
        };
        app.refresh_capabilities();
        db.insert_site(&app).unwrap();

        let back = db.get_site("d-1").unwrap();
        assert_eq!(back.kind, crate::site::KIND_DOCKER);
        assert_eq!(back.config.service, "app");
        assert_eq!(back.config.app_port, Some(3000));
        assert_eq!(back.config.db_engine.as_deref(), Some("postgres"));
        // A docker app with a recognized DB engine earns db_sync but never the
        // WordPress-only capabilities.
        assert!(back.capabilities.code_sync);
        assert!(back.capabilities.db_sync, "a recognized DB engine flips db_sync on");
        assert!(!back.capabilities.wp_tools);
        assert!(!back.capabilities.one_click_login);

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn imported_sites_are_found_by_their_remote() {
        let path = temp_db_path("origin");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
        let db = Db::open(&path).unwrap();

        let mut imported = site("s-1", "client-blog");
        imported.connection_id = Some("conn-a".into());
        imported.remote_site_id = Some(7);
        db.insert_site(&imported).unwrap();
        db.insert_site(&site("s-2", "handmade")).unwrap();

        let hits = db.sites_from_remote("conn-a", 7).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].slug, "client-blog");
        assert_eq!(hits[0].remote_site_id, Some(7));

        // A different remote, and a different connection, are both misses —
        // the guard must not confuse "site #7 on prod" with "site #7 on staging".
        assert!(db.sites_from_remote("conn-a", 8).unwrap().is_empty());
        assert!(db.sites_from_remote("conn-b", 7).unwrap().is_empty());

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
