use std::{collections::HashSet, fs, path::Path};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, Transaction};
use serde::Serialize;

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

#[derive(Debug, Clone, Serialize)]
pub struct StoredImage {
    pub id: i64,
    pub file_name: String,
    pub sort_index: i64,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct AdminSettings {
    pub slideshow_interval_seconds: u64,
    pub night_mode_start: String,
    pub night_mode_end: String,
}

pub fn initialize(database_path: &Path) -> Result<()> {
    ensure_parent_directory(database_path)?;

    let mut conn = Connection::open(database_path)
        .with_context(|| format!("failed to open sqlite db at {}", database_path.display()))?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA busy_timeout = 5000;
         PRAGMA foreign_keys = ON;",
    )
    .context("failed to configure sqlite database pragmas")?;

    run_migrations(&mut conn)?;

    Ok(())
}

pub fn list_images(database_path: &Path) -> Result<Vec<StoredImage>> {
    let conn = open_connection(database_path)?;
    let mut stmt = conn
        .prepare(
            "SELECT id, file_name, sort_index, created_at
             FROM images
             ORDER BY sort_index ASC, id ASC",
        )
        .context("failed to prepare image list query")?;

    let images = stmt
        .query_map([], |row| {
            Ok(StoredImage {
                id: row.get(0)?,
                file_name: row.get(1)?,
                sort_index: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .context("failed to query image list")?
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("failed to decode image list rows")?;

    Ok(images)
}

pub fn insert_image(database_path: &Path, file_name: &str) -> Result<StoredImage> {
    let mut conn = open_connection(database_path)?;
    let tx = conn
        .transaction()
        .context("failed to begin image insert transaction")?;
    let next_sort_index: i64 = tx
        .query_row("SELECT COALESCE(MAX(sort_index) + 1, 0) FROM images", [], |row| {
            row.get(0)
        })
        .context("failed to compute next image sort index")?;

    tx.execute(
        "INSERT INTO images(file_name, sort_index, created_at)
         VALUES(?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
        (file_name, next_sort_index),
    )
    .context("failed to insert image metadata")?;

    let image_id = tx.last_insert_rowid();
    let image = tx
        .query_row(
            "SELECT id, file_name, sort_index, created_at FROM images WHERE id = ?",
            [image_id],
            |row| {
                Ok(StoredImage {
                    id: row.get(0)?,
                    file_name: row.get(1)?,
                    sort_index: row.get(2)?,
                    created_at: row.get(3)?,
                })
            },
        )
        .context("failed to load inserted image metadata")?;

    tx.commit().context("failed to commit image insert")?;

    Ok(image)
}

pub fn reorder_images(database_path: &Path, ordered_ids: &[i64]) -> Result<()> {
    let mut conn = open_connection(database_path)?;
    let tx = conn
        .transaction()
        .context("failed to begin image reorder transaction")?;

    let current_ids = load_all_image_ids(&tx)?;
    let current_set: HashSet<i64> = current_ids.into_iter().collect();
    let requested_set: HashSet<i64> = ordered_ids.iter().copied().collect();

    if current_set != requested_set || ordered_ids.len() != requested_set.len() {
        bail!("reorder payload must contain each image id exactly once");
    }

    let reorder_window = i64::try_from(ordered_ids.len())
        .context("image count does not fit in sqlite integer range")?;
    tx.execute(
        "UPDATE images SET sort_index = sort_index + ?",
        [reorder_window],
    )
    .context("failed preparing transient sort indexes for reorder")?;

    for (index, image_id) in ordered_ids.iter().enumerate() {
        tx.execute(
            "UPDATE images SET sort_index = ? WHERE id = ?",
            (index as i64, image_id),
        )
        .with_context(|| format!("failed updating sort index for image id {image_id}"))?;
    }

    tx.commit().context("failed to commit image reorder")?;

    Ok(())
}

pub fn delete_image(database_path: &Path, image_id: i64) -> Result<Option<String>> {
    let mut conn = open_connection(database_path)?;
    let tx = conn
        .transaction()
        .context("failed to begin image delete transaction")?;

    let existing = tx
        .query_row(
            "SELECT file_name, sort_index FROM images WHERE id = ?",
            [image_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()
        .context("failed to load image metadata for delete")?;

    let Some((file_name, removed_sort_index)) = existing else {
        tx.commit()
            .context("failed to commit noop image delete transaction")?;
        return Ok(None);
    };

    tx.execute("DELETE FROM images WHERE id = ?", [image_id])
        .with_context(|| format!("failed to delete image metadata for id {image_id}"))?;
    tx.execute(
        "UPDATE images SET sort_index = sort_index - 1 WHERE sort_index > ?",
        [removed_sort_index],
    )
    .with_context(|| format!("failed to compact sort indexes after deleting id {image_id}"))?;

    tx.commit().context("failed to commit image delete")?;
    Ok(Some(file_name))
}

pub fn load_admin_settings(database_path: &Path, defaults: &AdminSettings) -> Result<AdminSettings> {
    let conn = open_connection(database_path)?;
    let mut settings = defaults.clone();
    let mut stmt = conn
        .prepare(
            "SELECT key, value FROM settings
             WHERE key IN ('slideshow_interval_seconds', 'night_mode_start', 'night_mode_end')",
        )
        .context("failed to prepare settings query")?;

    let rows = stmt
        .query_map([], |row| {
            let key: String = row.get(0)?;
            let value: String = row.get(1)?;
            Ok((key, value))
        })
        .context("failed to query settings")?;

    for row in rows {
        let (key, value) = row.context("failed decoding settings row")?;
        match key.as_str() {
            "slideshow_interval_seconds" => {
                settings.slideshow_interval_seconds = value.parse().with_context(|| {
                    "invalid numeric value for settings.slideshow_interval_seconds"
                })?;
            }
            "night_mode_start" => settings.night_mode_start = value,
            "night_mode_end" => settings.night_mode_end = value,
            _ => {}
        }
    }

    Ok(settings)
}

pub fn save_admin_settings(database_path: &Path, settings: &AdminSettings) -> Result<()> {
    let mut conn = open_connection(database_path)?;
    let tx = conn
        .transaction()
        .context("failed to begin settings transaction")?;

    upsert_setting(
        &tx,
        "slideshow_interval_seconds",
        &settings.slideshow_interval_seconds.to_string(),
    )?;
    upsert_setting(&tx, "night_mode_start", &settings.night_mode_start)?;
    upsert_setting(&tx, "night_mode_end", &settings.night_mode_end)?;

    tx.commit().context("failed to commit settings transaction")?;
    Ok(())
}

fn open_connection(database_path: &Path) -> Result<Connection> {
    let conn = Connection::open(database_path)
        .with_context(|| format!("failed to open sqlite db at {}", database_path.display()))?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA busy_timeout = 5000;
         PRAGMA foreign_keys = ON;",
    )
    .context("failed to configure sqlite connection pragmas")?;
    Ok(conn)
}

fn load_all_image_ids(tx: &Transaction<'_>) -> Result<Vec<i64>> {
    let mut stmt = tx
        .prepare("SELECT id FROM images")
        .context("failed to prepare image id query")?;
    let ids = stmt
        .query_map([], |row| row.get(0))
        .context("failed to query image ids")?
        .collect::<rusqlite::Result<Vec<i64>>>()
        .context("failed decoding image ids")?;
    Ok(ids)
}

fn upsert_setting(tx: &Transaction<'_>, key: &str, value: &str) -> Result<()> {
    tx.execute(
        "INSERT INTO settings(key, value) VALUES(?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        (key, value),
    )
    .with_context(|| format!("failed to upsert setting '{key}'"))?;
    Ok(())
}

fn ensure_parent_directory(database_path: &Path) -> Result<()> {
    match database_path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create sqlite parent directory {}",
                    parent.display()
                )
            })?;
            Ok(())
        }
        _ => Ok(()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("photoframe-test-{nanos}.sqlite"))
    }

    #[test]
    fn test_db_lifecycle_and_operations() {
        let db_path = temp_db_path();
        initialize(&db_path).unwrap();

        // 1. Initial image list should be empty
        let images = list_images(&db_path).unwrap();
        assert!(images.is_empty());

        // 2. Insert images
        let img1 = insert_image(&db_path, "test1.jpg").unwrap();
        assert_eq!(img1.sort_index, 0);
        let img2 = insert_image(&db_path, "test2.jpg").unwrap();
        assert_eq!(img2.sort_index, 1);
        let img3 = insert_image(&db_path, "test3.jpg").unwrap();
        assert_eq!(img3.sort_index, 2);

        let list = list_images(&db_path).unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].file_name, "test1.jpg");
        assert_eq!(list[1].file_name, "test2.jpg");
        assert_eq!(list[2].file_name, "test3.jpg");

        // 3. Reorder images: put img3 first, then img1, then img2
        reorder_images(&db_path, &[img3.id, img1.id, img2.id]).unwrap();
        let reordered = list_images(&db_path).unwrap();
        assert_eq!(reordered[0].id, img3.id);
        assert_eq!(reordered[0].sort_index, 0);
        assert_eq!(reordered[1].id, img1.id);
        assert_eq!(reordered[1].sort_index, 1);
        assert_eq!(reordered[2].id, img2.id);
        assert_eq!(reordered[2].sort_index, 2);

        // 4. Invalid reorder payload (missing id or duplicate)
        assert!(reorder_images(&db_path, &[img3.id, img1.id]).is_err());
        assert!(reorder_images(&db_path, &[img3.id, img1.id, img1.id]).is_err());

        // 5. Delete middle image (img1, sort_index 1) and verify compacting
        let deleted_file = delete_image(&db_path, img1.id).unwrap();
        assert_eq!(deleted_file, Some("test1.jpg".to_string()));

        let after_delete = list_images(&db_path).unwrap();
        assert_eq!(after_delete.len(), 2);
        assert_eq!(after_delete[0].id, img3.id);
        assert_eq!(after_delete[0].sort_index, 0);
        assert_eq!(after_delete[1].id, img2.id);
        assert_eq!(after_delete[1].sort_index, 1); // compacted from 2 to 1!

        // 6. Delete non-existent image
        let non_existent = delete_image(&db_path, 9999).unwrap();
        assert_eq!(non_existent, None);

        // 7. Admin settings load defaults and update
        let defaults = AdminSettings {
            slideshow_interval_seconds: 30,
            night_mode_start: "20:00".to_string(),
            night_mode_end: "06:00".to_string(),
        };
        let loaded = load_admin_settings(&db_path, &defaults).unwrap();
        assert_eq!(loaded.slideshow_interval_seconds, 30);
        assert_eq!(loaded.night_mode_start, "20:00");

        let updated = AdminSettings {
            slideshow_interval_seconds: 60,
            night_mode_start: "22:00".to_string(),
            night_mode_end: "07:30".to_string(),
        };
        save_admin_settings(&db_path, &updated).unwrap();
        let reloaded = load_admin_settings(&db_path, &defaults).unwrap();
        assert_eq!(reloaded.slideshow_interval_seconds, 60);
        assert_eq!(reloaded.night_mode_start, "22:00");
        assert_eq!(reloaded.night_mode_end, "07:30");

        let _ = std::fs::remove_file(&db_path);
    }
}
