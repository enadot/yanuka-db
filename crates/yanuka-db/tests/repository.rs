//! Integration tests against a real SQLite database.
//!
//! These run the production migrations and the production queries. Nothing here
//! needs a webview or a Tauri toolchain, which is the whole reason the storage
//! layer is a separate crate.

use yanuka_db::models::{ContactInput, PhoneInput, SearchQuery};
use yanuka_db::{migrate, open_in_memory, repository, search, taxonomy};

fn db() -> yanuka_db::rusqlite::Connection {
    let mut connection = open_in_memory().expect("open");
    migrate(&mut connection).expect("migrate");
    connection
}

fn contact(display_name: &str) -> ContactInput {
    ContactInput { display_name: display_name.to_string(), ..Default::default() }
}

#[test]
fn migrations_apply_and_are_idempotent() {
    let mut connection = open_in_memory().unwrap();
    assert_eq!(migrate(&mut connection).unwrap(), 1);
    assert_eq!(migrate(&mut connection).unwrap(), 0);
    assert_eq!(yanuka_db::current_version(&connection).unwrap(), yanuka_db::target_version());
}

#[test]
fn bundled_sqlite_has_the_capabilities_the_schema_needs() {
    // Guards against a dependency bump silently dropping FTS5 or the trigram
    // tokenizer, which would leave search broken only at runtime.
    let connection = open_in_memory().unwrap();
    yanuka_db::connection::assert_capabilities(&connection).unwrap();
}

#[test]
fn creates_and_reads_back_a_contact() {
    let mut connection = db();
    let created = repository::create_contact(&mut connection, &contact("אברהם כהן"), None).unwrap();

    assert_eq!(created.contact.display_name, "אברהם כהן");
    assert_eq!(created.contact.version, 1);

    let fetched = repository::get_contact(&connection, &created.contact.id).unwrap().unwrap();
    assert_eq!(fetched.contact.display_name, "אברהם כהן");
}

#[test]
fn requires_only_a_name() {
    // The product exists to capture half-remembered people; a contact with
    // nothing but a description must be storable.
    let mut connection = db();
    let created =
        repository::create_contact(&mut connection, &contact("החשמלאי מאנטוורפן"), None).unwrap();
    assert!(created.phones.is_empty());
}

#[test]
fn rejects_a_blank_name() {
    let mut connection = db();
    let result = repository::create_contact(&mut connection, &contact("   "), None);
    assert!(matches!(result, Err(yanuka_db::DbError::Validation(_))));
}

#[test]
fn keeps_an_unparseable_phone_number() {
    let mut connection = db();
    let mut input = contact("מספר חלקי");
    input.phones = vec![PhoneInput { raw: "בבית של אדלר".into(), ..Default::default() }];

    let created = repository::create_contact(&mut connection, &input, None).unwrap();
    assert_eq!(created.phones.len(), 1);
    assert_eq!(created.phones[0].raw, "בבית של אדלר");
}

#[test]
fn refuses_an_update_based_on_a_stale_version() {
    let mut connection = db();
    let created = repository::create_contact(&mut connection, &contact("גרסאות"), None).unwrap();

    let mut changed = contact("גרסאות");
    changed.city = Some("ירושלים".into());
    repository::update_contact(&mut connection, &created.contact.id, &changed, Some(1)).unwrap();

    // Second write still thinks it is editing version 1.
    let result =
        repository::update_contact(&mut connection, &created.contact.id, &changed, Some(1));
    assert!(matches!(result, Err(yanuka_db::DbError::StaleVersion { .. })));
}

#[test]
fn soft_deletes_and_restores() {
    let mut connection = db();
    let created = repository::create_contact(&mut connection, &contact("למחיקה"), None).unwrap();
    let id = created.contact.id.clone();

    repository::delete_contact(&mut connection, &id).unwrap();
    let after = repository::get_contact(&connection, &id).unwrap().unwrap();
    assert!(after.contact.deleted_at.is_some(), "the row must survive so the delete can sync");

    // And must drop out of search while deleted.
    let response =
        search::search(&connection, &SearchQuery { text: "למחיקה".into(), ..Default::default() })
            .unwrap();
    assert_eq!(response.total, 0);

    repository::restore_contact(&mut connection, &id).unwrap();
    let restored = repository::get_contact(&connection, &id).unwrap().unwrap();
    assert!(restored.contact.deleted_at.is_none());
}

#[test]
fn every_write_appends_to_the_mutation_log() {
    // Nothing may change on disk without a queued mutation, or an edit made
    // offline would silently never reach another device.
    let mut connection = db();
    let created = repository::create_contact(&mut connection, &contact("יומן"), None).unwrap();
    repository::update_contact(&mut connection, &created.contact.id, &contact("יומן שונה"), None)
        .unwrap();
    repository::delete_contact(&mut connection, &created.contact.id).unwrap();

    let pending = yanuka_db::mutation::pending_count(&connection).unwrap();
    assert_eq!(pending, 3, "create, update and delete must each be logged");
}

#[test]
fn finds_a_contact_by_its_profession() {
    let mut connection = db();
    let mut input = contact("ישראל סופר");
    input.profession = Some("סופר סת\"ם".into());
    input.city = Some("ירושלים".into());
    repository::create_contact(&mut connection, &input, None).unwrap();

    let response = search::search(
        &connection,
        &SearchQuery { text: "סופר סתם".into(), ..Default::default() },
    )
    .unwrap();

    assert_eq!(response.total, 1);
    assert_eq!(response.results[0].contact.display_name, "ישראל סופר");
}

#[test]
fn finds_a_contact_by_a_word_from_its_notes() {
    let mut connection = db();
    let mut input = contact("שם שלא זוכרים");
    input.notes = Some("יהודי מלונדון שעוסק בנדלן והומלץ על ידי הרב".into());
    repository::create_contact(&mut connection, &input, None).unwrap();

    let response =
        search::search(&connection, &SearchQuery { text: "נדלן".into(), ..Default::default() })
            .unwrap();
    assert_eq!(response.total, 1);
}

#[test]
fn ignores_honorifics_on_both_sides() {
    let mut connection = db();
    repository::create_contact(&mut connection, &contact("הרב אברהם כהן"), None).unwrap();

    for query in ["אברהם כהן", "הרב אברהם כהן", "ר' אברהם כהן"] {
        let response = search::search(
            &connection,
            &SearchQuery { text: query.into(), ..Default::default() },
        )
        .unwrap();
        assert_eq!(response.total, 1, "query {query:?} should find the contact");
    }
}

#[test]
fn matches_a_phone_number_in_any_format() {
    let mut connection = db();
    let mut input = contact("בעל טלפון");
    input.country = Some("IL".into());
    input.phones = vec![PhoneInput { raw: "054-555-0134".into(), ..Default::default() }];
    repository::create_contact(&mut connection, &input, None).unwrap();

    for query in ["0545550134", "054-555-0134", "5550134"] {
        let response = search::search(
            &connection,
            &SearchQuery { text: query.into(), ..Default::default() },
        )
        .unwrap();
        assert_eq!(response.total, 1, "query {query:?} should find the contact");
    }
}

#[test]
fn adding_a_word_narrows_the_result_set() {
    let mut connection = db();
    for (name, city) in [("סופר א", "ירושלים"), ("סופר ב", "לונדון"), ("סופר ג", "ניו יורק")] {
        let mut input = contact(name);
        input.profession = Some("סופר סתם".into());
        input.city = Some(city.into());
        repository::create_contact(&mut connection, &input, None).unwrap();
    }

    let broad =
        search::search(&connection, &SearchQuery { text: "סופר".into(), ..Default::default() })
            .unwrap();
    let narrow = search::search(
        &connection,
        &SearchQuery { text: "סופר לונדון".into(), ..Default::default() },
    )
    .unwrap();

    assert_eq!(broad.total, 3);
    assert_eq!(narrow.total, 1);
}

#[test]
fn returns_facet_counts_with_the_results() {
    let mut connection = db();
    for (name, country) in [("א", "IL"), ("ב", "IL"), ("ג", "GB")] {
        let mut input = contact(name);
        input.profession = Some("סופר".into());
        input.country = Some(country.into());
        repository::create_contact(&mut connection, &input, None).unwrap();
    }

    let response =
        search::search(&connection, &SearchQuery { text: "סופר".into(), ..Default::default() })
            .unwrap();

    let countries = response.facets.get("country").expect("country facet");
    let israel = countries.iter().find(|f| f.value == "IL").expect("IL facet");
    assert_eq!(israel.count, 2);
}

#[test]
fn facet_filters_restrict_the_result_set() {
    let mut connection = db();
    for (name, country) in [("א", "IL"), ("ב", "GB")] {
        let mut input = contact(name);
        input.profession = Some("סופר".into());
        input.country = Some(country.into());
        repository::create_contact(&mut connection, &input, None).unwrap();
    }

    let mut filters = std::collections::HashMap::new();
    filters.insert("country".to_string(), vec!["IL".to_string()]);

    let response = search::search(
        &connection,
        &SearchQuery { text: "סופר".into(), filters, ..Default::default() },
    )
    .unwrap();

    assert_eq!(response.total, 1);
}

#[test]
fn keyset_pagination_neither_repeats_nor_skips() {
    let mut connection = db();
    for index in 0..25 {
        repository::create_contact(&mut connection, &contact(&format!("איש {index:02}")), None)
            .unwrap();
    }

    let first = repository::list_contacts(&connection, None, 10, None).unwrap();
    assert_eq!(first.items.len(), 10);
    assert_eq!(first.total, 25);

    let cursor = first.next_cursor.clone().expect("a second page must be offered");
    let second = repository::list_contacts(&connection, Some(&cursor), 10, None).unwrap();

    let first_ids: Vec<&str> = first.items.iter().map(|c| c.id.as_str()).collect();
    for item in &second.items {
        assert!(!first_ids.contains(&item.id.as_str()), "page two repeated a row from page one");
    }
}

#[test]
fn reindexes_when_a_tag_is_attached() {
    let mut connection = db();
    let tag = taxonomy::create_tag(&connection, "סת\"ם", None).unwrap();

    let mut input = contact("בעל תגית");
    input.tag_ids = vec![tag.id.clone()];
    repository::create_contact(&mut connection, &input, None).unwrap();

    let response =
        search::search(&connection, &SearchQuery { text: "סתם".into(), ..Default::default() })
            .unwrap();
    assert_eq!(response.total, 1, "a contact must be findable by its tag");
}

#[test]
fn creating_a_tag_twice_returns_the_same_row() {
    // `סת"ם` and `סתם` normalize alike; splitting them would fragment the facet
    // counts and make the filter panel misleading.
    let connection = db();
    let first = taxonomy::create_tag(&connection, "סת\"ם", None).unwrap();
    let second = taxonomy::create_tag(&connection, "סתם", None).unwrap();
    assert_eq!(first.id, second.id);
}

#[test]
fn refuses_to_link_a_contact_to_itself() {
    let mut connection = db();
    let created = repository::create_contact(&mut connection, &contact("יחיד"), None).unwrap();
    let result = taxonomy::create_relationship(
        &connection,
        &created.contact.id,
        &created.contact.id,
        "knows",
        None,
    );
    assert!(result.is_err());
}

#[test]
fn a_note_makes_a_contact_findable() {
    let mut connection = db();
    let created = repository::create_contact(&mut connection, &contact("בעל הערה"), None).unwrap();
    taxonomy::add_note(&mut connection, &created.contact.id, "פגשנו אותו בכנס באנטוורפן", false)
        .unwrap();

    let response =
        search::search(&connection, &SearchQuery { text: "אנטוורפן".into(), ..Default::default() })
            .unwrap();
    assert_eq!(response.total, 1);
}

#[test]
fn an_empty_query_browses_rather_than_failing() {
    let mut connection = db();
    repository::create_contact(&mut connection, &contact("מישהו"), None).unwrap();

    let response = search::search(&connection, &SearchQuery::default()).unwrap();
    assert_eq!(response.total, 1);
}

#[test]
fn stats_report_the_pending_mutation_count() {
    let mut connection = db();
    repository::create_contact(&mut connection, &contact("סטטיסטיקה"), None).unwrap();

    let value = taxonomy::stats(&connection).unwrap();
    assert_eq!(value["contacts"], 1);
    assert_eq!(value["sync"]["pendingMutations"], 1);
}
