use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, Transaction};

const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    sql: r#"
        CREATE TABLE IF NOT EXISTS settings(
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS images(
            id INTEGER PRIMARY KEY,
            file_name TEXT NOT NULL,
            sort_index INTEGER NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE UNIQUE INDEX IF NOT EXISTS idx_images_sort_index ON images(sort_index);
    "#,
}];

struct Migration {
    version: i64,
    sql: &'static str,
}

pub fn initialize(database_path: &Path) -> Result<()> {
    ensure_parent_directory(database_path)?;

    let mut conn = Connection::open(database_path)
        .with_context(|| format!("failed to open sqlite db at {}", database_path.display()))?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .context("failed to enable sqlite foreign_keys")?;

    run_migrations(&mut conn)?;

    Ok(())
}

fn ensure_parent_directory(database_path: &Path) -> Result<()> {
    match database_path.parent() {
        Some(parent) => {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create sqlite parent directory {}",
                    parent.display()
                )
            })?;
            Ok(())
        }
        None => bail!(
            "database path '{}' has no parent directory",
            database_path.display()
        ),
    }
}

fn run_migrations(conn: &mut Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations(version INTEGER PRIMARY KEY NOT NULL);",
    )
    .context("failed to create schema_migrations table")?;

    let tx = conn.transaction().context("failed to begin migration transaction")?;
    let current_version = current_version(&tx)?;

    for migration in MIGRATIONS
        .iter()
        .filter(|migration| migration.version > current_version)
    {
        tx.execute_batch(migration.sql)
            .with_context(|| format!("failed to apply migration {}", migration.version))?;
        tx.execute(
            "INSERT INTO schema_migrations(version) VALUES (?)",
            [migration.version],
        )
        .with_context(|| format!("failed to record migration {}", migration.version))?;
    }

    tx.commit().context("failed to commit migrations")?;

    Ok(())
}

fn current_version(tx: &Transaction<'_>) -> Result<i64> {
    let mut stmt = tx
        .prepare("SELECT COALESCE(MAX(version), 0) FROM schema_migrations")
        .context("failed to prepare migration version query")?;
    let current_version = stmt
        .query_row([], |row| row.get(0))
        .context("failed to query migration version")?;

    Ok(current_version)
}
