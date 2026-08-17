use std::path::{Path, PathBuf};

/// How many pre-migration backups to keep.
///
/// Three is enough to walk back through a couple of bad upgrades without the
/// backups themselves becoming the thing that fills the disk.
const KEEP: usize = 3;

/// Copy the database aside before migrations run.
///
/// Migrations are forward-only — there are no `down` scripts, because a
/// hand-written reverse migration is exactly the kind of code that silently
/// drops a column's worth of data. The backup is what makes that safe: if an
/// upgrade goes wrong, the previous file is still there, untouched.
///
/// Taken with a plain file copy rather than SQLite's backup API because at this
/// point in startup no connection is open yet, so there is nothing to be
/// consistent with — the file on disk is the whole state.
pub fn before_migration(database: &Path) -> std::io::Result<PathBuf> {
    let directory = database.parent().unwrap_or_else(|| Path::new("."));
    let backups = directory.join("backups");
    std::fs::create_dir_all(&backups)?;

    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let target = backups.join(format!("contacts-{stamp}.db"));

    std::fs::copy(database, &target)?;

    // The write-ahead log holds committed transactions that have not yet been
    // folded into the main file. Copying it too is what makes the backup a
    // complete picture rather than a slightly stale one.
    for suffix in ["-wal", "-shm"] {
        let side = PathBuf::from(format!("{}{suffix}", database.display()));
        if side.exists() {
            let _ = std::fs::copy(&side, backups.join(format!("contacts-{stamp}.db{suffix}")));
        }
    }

    prune(&backups)?;
    Ok(target)
}

fn prune(backups: &Path) -> std::io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(backups)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("contacts-")
                && entry.file_name().to_string_lossy().ends_with(".db")
        })
        .collect();

    // Names carry a unix timestamp, so lexical order is chronological order.
    entries.sort_by_key(|entry| entry.file_name());

    while entries.len() > KEEP {
        let oldest = entries.remove(0);
        let _ = std::fs::remove_file(oldest.path());
        for suffix in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(PathBuf::from(format!(
                "{}{suffix}",
                oldest.path().display()
            )));
        }
    }

    Ok(())
}
