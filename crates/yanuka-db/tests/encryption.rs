//! Encryption at rest, end to end: create encrypted, reopen, reject a wrong
//! key, upgrade a plaintext file in place, and prove the backups stay
//! encrypted. Runs only with `--features sqlcipher` — the same build the
//! Windows desktop ships.
#![cfg(feature = "sqlcipher")]

use yanuka_db::models::{ContactInput, SearchQuery};
use yanuka_db::{encryption, migrate, repository, search, taxonomy};

const KEY_HEX: &str = "8f3a2b1c4d5e6f708192a3b4c5d6e7f8090a1b2c3d4e5f60718293a4b5c6d7e8";
const OTHER_HEX: &str = "0000000000000000000000000000000000000000000000000000000000000001";

fn contact(display_name: &str) -> ContactInput {
    ContactInput { display_name: display_name.to_string(), ..Default::default() }
}

#[test]
fn key_normalization_accepts_what_people_paste() {
    // The recovery key is displayed in dashed uppercase groups; both that and
    // the bare lowercase hex must resolve to the same pragma value.
    let dashed = "8F3A2B1C-4D5E6F70-8192A3B4-C5D6E7F8-090A1B2C-3D4E5F60-718293A4-B5C6D7E8";
    assert_eq!(
        encryption::raw_key_pragma(dashed).unwrap(),
        encryption::raw_key_pragma(KEY_HEX).unwrap(),
    );
    assert!(encryption::raw_key_pragma("קצר מדי").is_err());
}

#[test]
fn an_encrypted_database_opens_with_its_key_and_only_with_its_key() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("contacts.db");
    let key = encryption::raw_key_pragma(KEY_HEX).unwrap();

    {
        let mut connection = yanuka_db::open(&path, Some(&key)).unwrap();
        migrate(&mut connection).unwrap();
        repository::create_contact(&mut connection, &contact("אברהם כהן"), None).unwrap();
    }

    // The file on disk must not look like SQLite at all.
    assert!(!encryption::is_plaintext(&path));

    // Right key: everything works, including FTS5 search under SQLCipher.
    {
        let connection = yanuka_db::open(&path, Some(&key)).unwrap();
        let found = search::search(
            &connection,
            &SearchQuery { text: "אברהם".into(), ..Default::default() },
        )
        .unwrap();
        assert_eq!(found.results.len(), 1);
    }

    // Wrong key: refused, and recognizably so.
    let wrong = encryption::raw_key_pragma(OTHER_HEX).unwrap();
    let error = yanuka_db::open(&path, Some(&wrong)).unwrap_err();
    assert!(encryption::is_wrong_key(&error));
    let error = yanuka_db::open(&path, None).unwrap_err();
    assert!(encryption::is_wrong_key(&error));
}

#[test]
fn a_plaintext_database_upgrades_in_place_without_losing_a_row() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("contacts.db");
    let key = encryption::raw_key_pragma(KEY_HEX).unwrap();

    // A database from the pre-encryption era, with history in its journal.
    {
        let mut connection = yanuka_db::open(&path, None).unwrap();
        migrate(&mut connection).unwrap();
        let created =
            repository::create_contact(&mut connection, &contact("יעקב פרידמן"), None).unwrap();
        taxonomy::add_note(&mut connection, &created.contact.id, "מומלץ על ידי הרב", false)
            .unwrap();
    }
    assert!(encryption::is_plaintext(&path));

    encryption::encrypt_in_place(&path, &key).unwrap();
    assert!(!encryption::is_plaintext(&path));
    // No staging or retired plaintext file may survive the swap.
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains("encrypting") || name.contains("plaintext-old"))
        .collect();
    assert!(leftovers.is_empty(), "leftovers: {leftovers:?}");

    let connection = yanuka_db::open(&path, Some(&key)).unwrap();
    let mutations: i64 =
        connection.query_row("SELECT count(*) FROM mutations", [], |row| row.get(0)).unwrap();
    assert_eq!(mutations, 2, "the journal must survive the upgrade");
    let found =
        search::search(&connection, &SearchQuery { text: "מומלץ".into(), ..Default::default() })
            .unwrap();
    assert_eq!(found.results.len(), 1, "notes must stay searchable after the upgrade");
}

#[test]
fn backups_of_an_encrypted_database_stay_encrypted() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("contacts.db");
    let key = encryption::raw_key_pragma(KEY_HEX).unwrap();

    let mut connection = yanuka_db::open(&path, Some(&key)).unwrap();
    migrate(&mut connection).unwrap();
    repository::create_contact(&mut connection, &contact("אברהם כהן"), None).unwrap();

    let target = dir.path().join("usb").join("גיבוי.db");
    yanuka_db::backup::backup_to(&connection, &target, Some(&key)).unwrap();

    // The copy is encrypted with the same key, and complete.
    assert!(!encryption::is_plaintext(&target));
    let restored = yanuka_db::open(&target, Some(&key)).unwrap();
    let count: i64 =
        restored.query_row("SELECT count(*) FROM contacts", [], |row| row.get(0)).unwrap();
    assert_eq!(count, 1);
}
