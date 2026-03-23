use std::path::{Path, PathBuf};

use include_dir::{include_dir, Dir};
use switchy_database::Database;
use switchy_database_connection::init_sqlite_rusqlite;
use switchy_schema::runner::MigrationRunner;

use crate::Error;

static MIGRATIONS: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/migrations");

/// Returns the default database path: ~/.local/share/tone-clone/tone-clone.db
pub fn default_db_path() -> Result<PathBuf, Error> {
    let data_dir = dirs::data_local_dir()
        .ok_or_else(|| Error::Config("could not determine local data directory".into()))?;
    Ok(data_dir.join("tone-clone").join("tone-clone.db"))
}

/// Open (or create) the SQLite database and run migrations.
pub async fn open(path: &Path) -> Result<Box<dyn Database>, Error> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            Error::Config(format!(
                "failed to create db directory {}: {e}",
                parent.display()
            ))
        })?;
    }

    let db = init_sqlite_rusqlite(Some(path))?;

    // Run embedded migrations
    let runner = MigrationRunner::new_embedded(&MIGRATIONS);
    runner.run(db.as_ref()).await?;

    Ok(db)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_open_in_memory() {
        let db = init_sqlite_rusqlite(None).unwrap();
        let runner = MigrationRunner::new_embedded(&MIGRATIONS);
        runner.run(db.as_ref()).await.unwrap();

        // Verify tables exist
        let tables = db.list_tables().await.unwrap();
        assert!(tables.contains(&"sources".to_string()));
        assert!(tables.contains(&"posts".to_string()));
        assert!(tables.contains(&"scrape_log".to_string()));
    }
}
