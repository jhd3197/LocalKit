//! Engine-native database export/import, dispatched on a site's kind + config
//! (plan 26 phase 2).
//!
//! WordPress keeps its wp-cli path (`wp db export`/`import`). Every other kind
//! that claims `db_sync` dumps via the database engine's own client, run inside
//! the DB service container: `mariadb-dump`/`mariadb` for mariadb,
//! `mysqldump`/`mysql` for mysql, `pg_dump`/`psql` for postgres. The client's
//! password is handed over as an environment variable (`MYSQL_PWD`/`PGPASSWORD`)
//! so it never lands on a command line.
//!
//! This is the single dispatch table Phase 2's "every kind × operation has an
//! explicit handler or a clean unsupported error" guarantee is tested against.

use std::io::BufReader;
use std::path::Path;

use crate::{docker, site, wordpress};
use site::Site;

/// The database engine's dump binary + the fixed flags a dump needs. The flags
/// are chosen to work as the app DB user (not root): `--single-transaction`
/// gives an InnoDB-consistent snapshot without a global lock, `--no-tablespaces`
/// avoids the PROCESS privilege a non-root user lacks.
fn dump_args(engine: &str, user: &str, db: &str) -> Result<Vec<String>, String> {
    let s = |v: &str| v.to_string();
    Ok(match engine {
        "mariadb" => vec![
            s("mariadb-dump"),
            s("--single-transaction"),
            s("--no-tablespaces"),
            s("-u"),
            s(user),
            s(db),
        ],
        "mysql" => vec![
            s("mysqldump"),
            s("--single-transaction"),
            s("--no-tablespaces"),
            s("-u"),
            s(user),
            s(db),
        ],
        "postgres" | "postgresql" => vec![
            s("pg_dump"),
            s("--clean"),
            s("--if-exists"),
            s("-U"),
            s(user),
            s(db),
        ],
        other => return Err(unsupported(other)),
    })
}

/// The database engine's import client — reads a dump on stdin. A mysql/mariadb
/// dump carries `DROP TABLE IF EXISTS`, and `pg_dump --clean --if-exists` does
/// the same, so importing over an existing database is idempotent.
fn import_args(engine: &str, user: &str, db: &str) -> Result<Vec<String>, String> {
    let s = |v: &str| v.to_string();
    Ok(match engine {
        "mariadb" => vec![s("mariadb"), s("-u"), s(user), s(db)],
        "mysql" => vec![s("mysql"), s("-u"), s(user), s(db)],
        "postgres" | "postgresql" => vec![s("psql"), s("-U"), s(user), s("-d"), s(db)],
        other => return Err(unsupported(other)),
    })
}

/// The environment variable the engine's clients read a password from.
fn password_env(engine: &str) -> &'static str {
    match engine {
        "postgres" | "postgresql" => "PGPASSWORD",
        _ => "MYSQL_PWD",
    }
}

fn unsupported(engine: &str) -> String {
    format!("no database dump support for engine `{engine}`")
}

/// The DB engine + service for an engine-native site, or a clean error if the
/// site's config never recorded one (a code-only kind).
fn engine_service(site: &Site) -> Result<(String, String), String> {
    match (site.config.db_engine.as_deref(), site.config.db_service.as_deref()) {
        (Some(engine), Some(service)) => Ok((engine.to_string(), service.to_string())),
        _ => Err(format!(
            "{} has no database engine to sync (code-only site)",
            site.name
        )),
    }
}

/// The app DB user / database / password from the site's `.env`.
fn creds(dir: &Path) -> (String, String, String) {
    (site::db_user(dir), site::db_name(dir), site::db_password(dir))
}

/// Export the site's database as SQL text.
///
/// WordPress dumps through wp-cli (with a short retry for the stopped-site boot
/// race); every other `db_sync` kind dumps engine-native. The dump is returned
/// as a `String`, matching what the snapshot/sync layers already expected from
/// the wp-cli path.
pub async fn export_sql(site: &Site, dir: &Path) -> Result<String, String> {
    if site.kind == site::KIND_WORDPRESS {
        return wp_export(dir).await;
    }
    let (engine, service) = engine_service(site)?;
    let (user, db, password) = creds(dir);
    let args = dump_args(&engine, &user, &db)?;
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    // The DB must be up + healthy to dump; bring just it up (idempotent) so a
    // snapshot of a stopped site works, mirroring what wp-cli got via depends_on.
    docker::compose_up_wait_service(dir, &service).await?;
    let out = docker::compose_exec_env(dir, &service, &[(password_env(&engine), &password)], &arg_refs)
        .await?;
    if out.trim().is_empty() {
        return Err("the database export came back empty".into());
    }
    Ok(out)
}

/// Import a SQL dump over the site's database.
///
/// WordPress imports through wp-cli; every other kind pipes the dump into the
/// engine's client running inside the DB container.
pub async fn import_sql(site: &Site, dir: &Path, sql: &[u8]) -> Result<(), String> {
    if site.kind == site::KIND_WORDPRESS {
        return wordpress::import_db(dir, sql).await;
    }
    let (engine, service) = engine_service(site)?;
    let (user, db, password) = creds(dir);
    let args = import_args(&engine, &user, &db)?;
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    docker::compose_up_wait_service(dir, &service).await?;
    docker::compose_exec_env_stdin_reader(
        dir,
        &service,
        &[(password_env(&engine), &password)],
        &arg_refs,
        &mut &sql[..],
    )
    .await
    .map(|_| ())
}

/// Export the database to a file on the host — used by the push flow, which
/// stages the dump for a chunked upload straight off disk (plan 19/26).
pub async fn export_to_file(site: &Site, dir: &Path, dest: &Path) -> Result<(), String> {
    let sql = export_sql(site, dir).await?;
    std::fs::write(dest, sql).map_err(|e| format!("failed to write database dump: {e}"))
}

/// Import a gzipped dump straight off disk, decompressing into the client's
/// stdin — the streaming counterpart of `import_sql` used by pull/import so a
/// remote database never exists decompressed in memory (plan 19/26).
pub async fn import_from_gz(site: &Site, dir: &Path, gz_path: &Path) -> Result<(), String> {
    if site.kind == site::KIND_WORDPRESS {
        return wordpress::import_db_from_gz(dir, gz_path).await;
    }
    let (engine, service) = engine_service(site)?;
    let (user, db, password) = creds(dir);
    let args = import_args(&engine, &user, &db)?;
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let file = std::fs::File::open(gz_path)
        .map_err(|e| format!("failed to open the downloaded dump: {e}"))?;
    let mut reader = flate2::read::GzDecoder::new(BufReader::new(file));
    docker::compose_up_wait_service(dir, &service).await?;
    docker::compose_exec_env_stdin_reader(
        dir,
        &service,
        &[(password_env(&engine), &password)],
        &arg_refs,
        &mut reader,
    )
    .await
    .map(|_| ())
}

/// `wp db export -` with a short retry loop: on a stopped site the first call
/// races the database container's first boot (same reason `wordpress::install`
/// retries). Kept here so `export_sql` is the one entry point for both paths.
async fn wp_export(dir: &Path) -> Result<String, String> {
    const ATTEMPTS: u32 = 5;
    let mut last = String::new();
    for attempt in 1..=ATTEMPTS {
        match docker::compose_run(dir, "wpcli", &["wp", "db", "export", "-"]).await {
            Ok(sql) if !sql.trim().is_empty() => return Ok(sql),
            Ok(_) => last = "the database export came back empty".into(),
            Err(e) => last = e,
        }
        if attempt < ATTEMPTS {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
    }
    Err(format!("failed to export the database: {last}"))
}

// ---------------------------------------------------------------------------
// Tests — the dispatch table
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Every engine LocalKit recognizes has an explicit dump + import handler,
    /// and the password goes in the engine's own env var (never an argument).
    #[test]
    fn every_recognized_engine_has_dump_and_import_handlers() {
        for engine in ["mariadb", "mysql", "postgres", "postgresql"] {
            let dump = dump_args(engine, "app", "appdb").unwrap();
            let imp = import_args(engine, "app", "appdb").unwrap();
            assert!(dump.iter().all(|a| a != "app-pw"), "no password on the dump line");
            assert!(imp.iter().all(|a| a != "app-pw"), "no password on the import line");
            // The db name and user are always present.
            assert!(dump.contains(&"appdb".to_string()) && dump.contains(&"app".to_string()));
            assert!(imp.contains(&"appdb".to_string()) && imp.contains(&"app".to_string()));
        }
    }

    #[test]
    fn mariadb_and_mysql_use_their_named_clients() {
        assert_eq!(dump_args("mariadb", "u", "d").unwrap()[0], "mariadb-dump");
        assert_eq!(import_args("mariadb", "u", "d").unwrap()[0], "mariadb");
        assert_eq!(dump_args("mysql", "u", "d").unwrap()[0], "mysqldump");
        assert_eq!(import_args("mysql", "u", "d").unwrap()[0], "mysql");
        assert_eq!(dump_args("postgres", "u", "d").unwrap()[0], "pg_dump");
        assert_eq!(import_args("postgres", "u", "d").unwrap()[0], "psql");
    }

    #[test]
    fn mysql_family_uses_mysql_pwd_and_postgres_uses_pgpassword() {
        assert_eq!(password_env("mariadb"), "MYSQL_PWD");
        assert_eq!(password_env("mysql"), "MYSQL_PWD");
        assert_eq!(password_env("postgres"), "PGPASSWORD");
        assert_eq!(password_env("postgresql"), "PGPASSWORD");
    }

    /// An unrecognized engine is a clean, user-displayable error — never a panic
    /// or a silently-wrong command.
    #[test]
    fn an_unknown_engine_is_a_clean_error() {
        let err = dump_args("cassandra", "u", "d").unwrap_err();
        assert!(err.contains("cassandra"), "{err}");
        assert!(import_args("mongodb", "u", "d").is_err());
    }
}
