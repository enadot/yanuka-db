//! Semantic search, end to end with the real embedding model: index, find by
//! meaning, stay fresh through edits and deletes, and never leak a deleted
//! contact. Runs with `--features semantic` and the model files fetched by
//! `scripts/fetch-semantic-model.mjs`:
//!
//! ```text
//! YANUKA_SEMANTIC_MODEL_DIR=apps/desktop/src-tauri/resources/semantic \
//!   cargo test -p yanuka-db --features semantic
//! ```
#![cfg(feature = "semantic")]

use std::collections::HashSet;
use std::path::PathBuf;

use yanuka_db::models::{ContactInput, SearchQuery};
use yanuka_db::semantic::{self, SemanticEngine};
use yanuka_db::{migrate, repository, search, taxonomy};

fn engine() -> Option<SemanticEngine> {
    let Ok(dir) = std::env::var("YANUKA_SEMANTIC_MODEL_DIR") else {
        eprintln!("skipped: set YANUKA_SEMANTIC_MODEL_DIR to run the semantic tests");
        return None;
    };
    let dir = PathBuf::from(dir);
    Some(
        SemanticEngine::load(&dir.join("model.onnx"), &dir.join("tokenizer.json"))
            .expect("the model directory exists but the engine failed to load"),
    )
}

fn contact(display_name: &str) -> ContactInput {
    ContactInput { display_name: display_name.to_string(), ..Default::default() }
}

#[test]
fn finds_a_contact_by_meaning_and_tracks_every_change() {
    let Some(engine) = engine() else { return };

    let dir = tempfile::tempdir().unwrap();
    let mut connection = yanuka_db::open(&dir.path().join("contacts.db"), None).unwrap();
    migrate(&mut connection).unwrap();

    // The archive: notes written years ago, in words the query will not use.
    let notes = [
        (
            "אריה גולד",
            "יהודי מלונדון, הרב כהן המליץ עליו, מקושר בקהילה ויכול לסייע בבניית בתי כנסת",
        ),
        ("שמעון וייס", "סופר סת\"ם מבני ברק, כותב מזוזות ותפילין, עבודה מהודרת"),
        ("דוד ברגר", "ראש קהילה באנטוורפן, מארגן שיעורי תורה, קשור לסוחרי יהלומים"),
    ];
    let mut ids = Vec::new();
    for (name, body) in notes {
        let created = repository::create_contact(&mut connection, &contact(name), None).unwrap();
        taxonomy::add_note(&mut connection, &created.contact.id, body, false).unwrap();
        ids.push(created.contact.id);
    }

    // Catch-up indexing converges and reports itself done.
    loop {
        let outcome = semantic::sync_step(&connection, &engine, 2).unwrap();
        if outcome.pending == 0 {
            break;
        }
    }
    let status = semantic::status(&connection).unwrap();
    assert_eq!(status.pending, 0);
    assert_eq!(status.indexed, 6, "three profiles and three notes");

    // A paraphrase no lexical layer can answer: no shared word with the note.
    let response = search::search_with_semantic(
        &connection,
        Some(&engine),
        &SearchQuery {
            text: "עסקן מאנגליה שעוזר עם בתי כנסת".into(), ..Default::default()
        },
    )
    .unwrap();
    let first = response.results.first().expect("the semantic layer must surface a candidate");
    assert_eq!(first.contact.id, ids[0], "the London note is the meaning match");
    let reason = &first.reasons[0];
    assert_eq!(reason.source, "semantic");
    assert!(reason.snippet.as_deref().unwrap_or_default().contains("מלונדון"));

    // Without the engine the same query finds nothing — proof the hit above
    // came from meaning, not from a lexical overlap this test overlooked.
    let lexical = search::search(
        &connection,
        &SearchQuery {
            text: "עסקן מאנגליה שעוזר עם בתי כנסת".into(), ..Default::default()
        },
    )
    .unwrap();
    assert!(
        lexical.results.iter().all(|result| result.contact.id != ids[0]),
        "the query must not match the London contact lexically"
    );

    // Deleting the contact removes it from the semantic answer immediately,
    // even before any reindex — the query joins on live contacts.
    repository::delete_contact(&mut connection, &ids[0]).unwrap();
    let hits = semantic::candidates(&connection, &engine, "עסקן מאנגליה", &HashSet::new()).unwrap();
    assert!(hits.iter().all(|hit| hit.contact_id != ids[0]));

    // And reconciliation cleans its rows out of the index.
    semantic::sync_contact(&connection, &engine, &ids[0]).unwrap();
    let remaining: i64 = connection
        .query_row("SELECT count(*) FROM semantic_index WHERE contact_id = ?1", [&ids[0]], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(remaining, 0);
}

#[test]
fn an_edited_note_is_reembedded_and_a_deleted_note_forgotten() {
    let Some(engine) = engine() else { return };

    let dir = tempfile::tempdir().unwrap();
    let mut connection = yanuka_db::open(&dir.path().join("contacts.db"), None).unwrap();
    migrate(&mut connection).unwrap();

    let created = repository::create_contact(&mut connection, &contact("יעקב שטרן"), None).unwrap();
    let id = created.contact.id.clone();
    let note_id = taxonomy::add_note(&mut connection, &id, "רופא ילדים מחיפה", false).unwrap();
    semantic::sync_contact(&connection, &engine, &id).unwrap();

    let before: String = connection
        .query_row(
            "SELECT source_hash FROM semantic_index WHERE doc_id = ?1",
            [format!("n:{note_id}")],
            |row| row.get(0),
        )
        .unwrap();

    taxonomy::update_note(&mut connection, &note_id, "מוהל מומחה מצפת", None).unwrap();
    semantic::sync_contact(&connection, &engine, &id).unwrap();
    let after: String = connection
        .query_row(
            "SELECT source_hash FROM semantic_index WHERE doc_id = ?1",
            [format!("n:{note_id}")],
            |row| row.get(0),
        )
        .unwrap();
    assert_ne!(before, after, "a changed note must be re-embedded");

    let hits =
        semantic::candidates(&connection, &engine, "מי עושה ברית מילה", &HashSet::new()).unwrap();
    assert!(hits.iter().any(|hit| hit.contact_id == id), "the new meaning must be findable");

    taxonomy::delete_note(&mut connection, &note_id).unwrap();
    semantic::sync_contact(&connection, &engine, &id).unwrap();
    let rows: i64 = connection
        .query_row(
            "SELECT count(*) FROM semantic_index WHERE doc_id = ?1",
            [format!("n:{note_id}")],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rows, 0, "a deleted note leaves no vector behind");
}
