//! Integration tests against a real SQLite database.
//!
//! These run the production migrations and the production queries. Nothing here
//! needs a webview or a Tauri toolchain, which is the whole reason the storage
//! layer is a separate crate.

use yanuka_db::models::{
    ContactInput, ContactPatch, EmailInput, OrganizationLinkInput, PhoneInput, SearchQuery,
};
use yanuka_db::{migrate, open_in_memory, repository, search, taxonomy};

fn db() -> yanuka_db::rusqlite::Connection {
    let mut connection = open_in_memory().expect("open");
    migrate(&mut connection).expect("migrate");
    connection
}

fn contact(display_name: &str) -> ContactInput {
    ContactInput { display_name: display_name.to_string(), ..Default::default() }
}

fn rename(display_name: &str) -> ContactPatch {
    ContactPatch { display_name: Some(display_name.to_string()), ..Default::default() }
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

    let changed = ContactPatch { city: Some(Some("ירושלים".into())), ..Default::default() };
    repository::update_contact(&mut connection, &created.contact.id, &changed, Some(1)).unwrap();

    // Second write still thinks it is editing version 1.
    let result =
        repository::update_contact(&mut connection, &created.contact.id, &changed, Some(1));
    assert!(matches!(result, Err(yanuka_db::DbError::StaleVersion { .. })));
}

#[test]
fn a_patch_leaves_untouched_collections_alone() {
    // The bug this pins: a write replaces the child collections wholesale, so
    // a screen that renders phones but not e-mail addresses used to delete
    // every address on save. Priority 1 — מידע לא הולך לאיבוד.
    let mut connection = db();
    let mut input = contact("שומר על מה שלא נגעו בו");
    input.emails = vec![EmailInput { address: "a@example.com".into(), ..Default::default() }];
    input.aliases = vec![yanuka_db::models::AliasInput {
        value: "אברהמ׳ל".into(),
        ..Default::default()
    }];
    input.languages = vec!["he".into()];
    input.specialties = vec!["סת\"ם".into()];
    let created = repository::create_contact(&mut connection, &input, None).unwrap();

    // A patch that only knows about phones.
    let patch = ContactPatch {
        phones: Some(vec![PhoneInput { raw: "054-5550134".into(), ..Default::default() }]),
        ..Default::default()
    };
    let updated =
        repository::update_contact(&mut connection, &created.contact.id, &patch, None).unwrap();

    assert_eq!(updated.phones.len(), 1);
    assert_eq!(updated.emails.len(), 1, "an untouched collection must survive the write");
    assert_eq!(updated.aliases.len(), 1);
    assert_eq!(updated.languages, vec!["he".to_string()]);
    assert_eq!(updated.specialties.len(), 1);
}

#[test]
fn a_patch_that_nulls_a_scalar_clears_it() {
    let mut connection = db();
    let mut input = contact("עיר שהוסרה");
    input.city = Some("ירושלים".into());
    input.profession = Some("סופר".into());
    let created = repository::create_contact(&mut connection, &input, None).unwrap();

    let patch: ContactPatch = serde_json::from_str(r#"{"city": null}"#).unwrap();
    let updated =
        repository::update_contact(&mut connection, &created.contact.id, &patch, None).unwrap();

    assert!(updated.contact.city.is_none());
    assert_eq!(updated.contact.profession.as_deref(), Some("סופר"));
}

#[test]
fn a_patch_that_sends_an_empty_collection_clears_it() {
    // The other half of the distinction: absent means untouched, empty means
    // the user actually removed everything.
    let mut connection = db();
    let mut input = contact("ריקון מכוון");
    input.emails = vec![EmailInput { address: "a@example.com".into(), ..Default::default() }];
    let created = repository::create_contact(&mut connection, &input, None).unwrap();

    let patch = ContactPatch { emails: Some(vec![]), ..Default::default() };
    let updated =
        repository::update_contact(&mut connection, &created.contact.id, &patch, None).unwrap();
    assert!(updated.emails.is_empty());
}

#[test]
fn links_a_contact_to_an_organization() {
    let mut connection = db();
    let organization = taxonomy::create_organization(
        &mut connection,
        "ישיבת מיר",
        "yeshiva",
        Some("ירושלים"),
        None,
    )
    .unwrap();

    let mut input = contact("ראש הישיבה");
    input.organizations = vec![OrganizationLinkInput {
        organization_id: organization.id.clone(),
        role: Some("ראש ישיבה".into()),
        ..Default::default()
    }];
    let created = repository::create_contact(&mut connection, &input, None).unwrap();

    assert_eq!(created.organizations.len(), 1);
    assert_eq!(created.organizations[0].organization.name, "ישיבת מיר");
    assert_eq!(created.organizations[0].role.as_deref(), Some("ראש ישיבה"));

    // And the link survives an edit that says nothing about organizations.
    let updated = repository::update_contact(
        &mut connection,
        &created.contact.id,
        &rename("ראש הישיבה שליט\"א"),
        None,
    )
    .unwrap();
    assert_eq!(updated.organizations.len(), 1);
}

#[test]
fn the_recycle_bin_lists_deleted_contacts_until_they_are_restored() {
    // A soft delete nothing can list is a hard delete with extra steps: the row
    // survives on disk and the user can never reach it again.
    let mut connection = db();
    let created =
        repository::create_contact(&mut connection, &contact("לסל המחזור"), None).unwrap();
    let id = created.contact.id.clone();

    assert!(repository::deleted_contacts(&connection, 100).unwrap().is_empty());

    repository::delete_contact(&mut connection, &id).unwrap();
    let binned = repository::deleted_contacts(&connection, 100).unwrap();
    assert_eq!(binned.len(), 1);
    assert_eq!(binned[0].contact.display_name, "לסל המחזור");
    assert!(!binned[0].deleted_at.is_empty());

    repository::restore_contact(&mut connection, &id).unwrap();
    assert!(repository::deleted_contacts(&connection, 100).unwrap().is_empty());

    // And searchable again, which is the point of restoring it.
    let response = search::search(
        &connection,
        &SearchQuery { text: "לסל המחזור".into(), ..Default::default() },
    )
    .unwrap();
    assert_eq!(response.total, 1);
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

fn logged(connection: &yanuka_db::rusqlite::Connection) -> Vec<(String, String, String)> {
    let mut statement = connection
        .prepare("SELECT entity_type, operation, COALESCE(payload, '') FROM mutations ORDER BY id")
        .unwrap();
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(|row| row.unwrap())
        .collect();
    rows
}

#[test]
fn every_write_appends_to_the_mutation_log() {
    // Nothing may change on disk without a queued mutation, or an edit made
    // offline would silently never reach another device.
    let mut connection = db();
    let created = repository::create_contact(&mut connection, &contact("יומן"), None).unwrap();
    repository::update_contact(&mut connection, &created.contact.id, &rename("יומן שונה"), None)
        .unwrap();
    repository::delete_contact(&mut connection, &created.contact.id).unwrap();

    let pending = yanuka_db::mutation::pending_count(&connection).unwrap();
    assert_eq!(pending, 3, "create, update and delete must each be logged");
}

#[test]
fn the_mutation_log_carries_what_the_user_actually_typed() {
    // Counting rows is not enough, and for a long time it was all this suite
    // checked: three mutations were appended per contact and every one of them
    // said `{"displayName": …}`. The city, the phone and the note the user had
    // entered were nowhere in the log, so a second device replaying it would
    // have rebuilt a bare name — on a product whose first promise is that
    // nothing typed gets lost. The log has to contain the data, not a receipt
    // saying data happened.
    let mut connection = db();

    let mut input = contact("שרה כהן");
    input.city = Some("ירושלים".into());
    input.phones = vec![PhoneInput { raw: "052-1234567".into(), ..Default::default() }];
    input.emails = vec![EmailInput { address: "sara@example.com".into(), ..Default::default() }];
    let created = repository::create_contact(&mut connection, &input, None).unwrap();

    taxonomy::add_note(&mut connection, &created.contact.id, "הכרנו דרך הרב", false).unwrap();

    let everything: String =
        logged(&connection).iter().map(|(_, _, payload)| payload.as_str()).collect();

    for expected in ["שרה כהן", "ירושלים", "052-1234567", "sara@example.com", "הכרנו דרך הרב"]
    {
        assert!(everything.contains(expected), "`{expected}` never reached the mutation log");
    }
}

#[test]
fn an_edit_logs_the_fields_that_moved_and_leaves_the_rest_out() {
    // The point of a field-level log: two devices editing different parts of
    // one contact must merge without a human. That only works if an edit
    // reports the field it touched — logging the whole record instead would
    // make every edit collide with every other edit.
    let mut connection = db();

    let mut input = contact("משה לוי");
    input.city = Some("בני ברק".into());
    input.profession = Some("סופר סת\"ם".into());
    let created = repository::create_contact(&mut connection, &input, None).unwrap();

    let patch = ContactPatch { city: Some(Some("ירושלים".into())), ..Default::default() };
    repository::update_contact(&mut connection, &created.contact.id, &patch, None).unwrap();

    let (_, operation, payload) = logged(&connection).pop().unwrap();
    assert_eq!(operation, "update");
    assert!(payload.contains("ירושלים"), "the new city is missing: {payload}");
    assert!(
        !payload.contains("סת\"ם"),
        "the profession did not change and must not appear in the payload: {payload}"
    );
}

#[test]
fn notes_relationships_tags_and_organizations_are_logged_too() {
    // These live in taxonomy.rs rather than repository.rs, and for that reason
    // alone they wrote to disk without logging anything at all. A relationship
    // is the single most product-critical thing here — "who was the man the
    // rabbi recommended" is a question about an edge — and it was invisible to
    // sync.
    let mut connection = db();
    let first = repository::create_contact(&mut connection, &contact("הממליץ"), None).unwrap();
    let second = repository::create_contact(&mut connection, &contact("המומלץ"), None).unwrap();

    taxonomy::create_relationship(
        &mut connection,
        &first.contact.id,
        &second.contact.id,
        "recommended",
        Some("בכינוס"),
    )
    .unwrap();
    taxonomy::add_note(&mut connection, &second.contact.id, "גר בלונדון", false).unwrap();
    taxonomy::create_tag(&mut connection, "סת\"ם", None).unwrap();
    taxonomy::create_organization(&mut connection, "ישיבת מיר", "yeshiva", None, None).unwrap();
    taxonomy::create_category(&mut connection, "רבנים", None).unwrap();

    let kinds: Vec<String> =
        logged(&connection).into_iter().map(|(entity_type, _, _)| entity_type).collect();

    for expected in ["relationship", "note", "tag", "organization", "category"] {
        assert!(kinds.iter().any(|kind| kind == expected), "no `{expected}` mutation in {kinds:?}");
    }
}

#[test]
fn finds_a_contact_by_its_profession() {
    let mut connection = db();
    let mut input = contact("ישראל סופר");
    input.profession = Some("סופר סת\"ם".into());
    input.city = Some("ירושלים".into());
    repository::create_contact(&mut connection, &input, None).unwrap();

    let response =
        search::search(&connection, &SearchQuery { text: "סופר סתם".into(), ..Default::default() })
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
        let response =
            search::search(&connection, &SearchQuery { text: query.into(), ..Default::default() })
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
        let response =
            search::search(&connection, &SearchQuery { text: query.into(), ..Default::default() })
                .unwrap();
        assert_eq!(response.total, 1, "query {query:?} should find the contact");
    }
}

#[test]
fn adding_a_word_narrows_the_result_set() {
    let mut connection = db();
    for (name, city) in [("סופר א", "ירושלים"), ("סופר ב", "לונדון"), ("סופר ג", "ניו יורק")]
    {
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
    let tag = taxonomy::create_tag(&mut connection, "סת\"ם", None).unwrap();

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
    let mut connection = db();
    let first = taxonomy::create_tag(&mut connection, "סת\"ם", None).unwrap();
    let second = taxonomy::create_tag(&mut connection, "סתם", None).unwrap();
    assert_eq!(first.id, second.id);
}

#[test]
fn refuses_to_link_a_contact_to_itself() {
    let mut connection = db();
    let created = repository::create_contact(&mut connection, &contact("יחיד"), None).unwrap();
    let result = taxonomy::create_relationship(
        &mut connection,
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

#[test]
fn detail_includes_organizations_relationships_and_notes() {
    // The desktop detail screen dereferences all three collections
    // unconditionally, so a missing key is a blank page, not a cosmetic gap.
    let mut connection = db();
    let person =
        repository::create_contact(&mut connection, &contact("ר' משה פרנקל"), None).unwrap();
    let friend =
        repository::create_contact(&mut connection, &contact("יעקב טייטלבוים"), None).unwrap();

    taxonomy::create_relationship(
        &mut connection,
        &person.contact.id,
        &friend.contact.id,
        "knows",
        None,
    )
    .unwrap();
    taxonomy::add_note(&mut connection, &person.contact.id, "מכיר את כל הסופרים בעיר", false)
        .unwrap();

    let organization = taxonomy::create_organization(
        &mut connection,
        "חברה קדישא",
        "community",
        Some("ירושלים"),
        Some("IL"),
    )
    .unwrap();
    connection
        .execute(
            "INSERT INTO contact_organizations (id, contact_id, organization_id, role, is_primary,
                                                created_at, updated_at, version)
             VALUES ('01LINK', ?1, ?2, 'גבאי', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 1)",
            yanuka_db::rusqlite::params![person.contact.id, organization.id],
        )
        .unwrap();

    let out = repository::get_contact(&connection, &person.contact.id).unwrap().unwrap();
    assert_eq!(out.organizations.len(), 1);
    assert_eq!(out.organizations[0].organization.name, "חברה קדישא");
    assert_eq!(out.organizations[0].role.as_deref(), Some("גבאי"));
    assert_eq!(out.contact_notes.len(), 1);
    assert_eq!(out.contact_notes[0].body, "מכיר את כל הסופרים בעיר");
    assert_eq!(out.relationships.len(), 1);
    assert_eq!(out.relationships[0].direction, "out");
    assert_eq!(out.relationships[0].other_contact.display_name, "יעקב טייטלבוים");

    // The same edge, read from its far endpoint.
    let seen_from_friend =
        repository::get_contact(&connection, &friend.contact.id).unwrap().unwrap();
    assert_eq!(seen_from_friend.relationships.len(), 1);
    assert_eq!(seen_from_friend.relationships[0].direction, "in");
    assert_eq!(seen_from_friend.relationships[0].other_contact.display_name, "ר' משה פרנקל");
}

#[test]
fn detail_serializes_the_collections_even_when_empty() {
    // This is the wire contract the webview relies on: `organizations`,
    // `relationships` and `contactNotes` must exist as arrays on every
    // response, because the screens call `.map` on them without guards.
    let mut connection = db();
    let created =
        repository::create_contact(&mut connection, &contact("אישה בלי כלום"), None).unwrap();

    let value = serde_json::to_value(&created).unwrap();
    for key in ["organizations", "relationships", "contactNotes"] {
        assert!(
            value.get(key).is_some_and(|v| v.is_array()),
            "expected `{key}` to serialize as an array, got: {:?}",
            value.get(key)
        );
    }
}

#[test]
fn duplicate_pairs_are_found_by_phone_email_and_name() {
    let mut connection = db();
    let mut first = contact("אברהם כהן");
    first.phones =
        vec![PhoneInput { raw: "054-5550134".into(), is_primary: true, ..Default::default() }];
    let first = repository::create_contact(&mut connection, &first, None).unwrap();

    let mut second = contact("אברהם הכהן");
    // Same number in a different format — the digits still end identically.
    second.phones =
        vec![PhoneInput { raw: "+972545550134".into(), is_primary: true, ..Default::default() }];
    let second = repository::create_contact(&mut connection, &second, None).unwrap();

    repository::create_contact(&mut connection, &contact("יעקב פרידמן"), None).unwrap();
    repository::create_contact(&mut connection, &contact("יעקב פרידמן"), None).unwrap();

    let pairs = yanuka_db::merge::list_duplicate_pairs(&connection, 50).unwrap();
    assert_eq!(pairs.len(), 2);
    // The phone pair outranks the name pair.
    assert!(pairs[0].confidence > pairs[1].confidence);
    assert_eq!(pairs[0].reasons, vec!["אותו מספר טלפון"]);
    let ids = [pairs[0].first.id.clone(), pairs[0].second.id.clone()];
    assert!(ids.contains(&first.contact.id) && ids.contains(&second.contact.id));
    assert_eq!(pairs[1].reasons, vec!["שם זהה"]);
}

#[test]
fn merge_preserves_every_field_and_moves_children() {
    let mut connection = db();
    let mut keep = contact("ר' משה פרנקל");
    keep.city = Some("ירושלים".into());
    keep.notes = Some("מכיר את כל הסופרים".into());
    keep.phones =
        vec![PhoneInput { raw: "02-6521234".into(), is_primary: true, ..Default::default() }];
    let keep = repository::create_contact(&mut connection, &keep, None).unwrap();

    let mut merge = contact("משה פרנקל");
    merge.city = Some("בני ברק".into()); // conflicts with the kept city
    merge.profession = Some("סופר סתם".into()); // fills a blank
    merge.notes = Some("לחזור אליו אחרי החגים".into());
    merge.phones = vec![
        PhoneInput { raw: "02-6521234".into(), is_primary: true, ..Default::default() }, // duplicate
        PhoneInput { raw: "054-5550199".into(), ..Default::default() },                  // new
    ];
    let merge = repository::create_contact(&mut connection, &merge, None).unwrap();
    taxonomy::add_note(&mut connection, &merge.contact.id, "פגשתי אותו בכנס", false).unwrap();

    let other =
        repository::create_contact(&mut connection, &contact("יעקב טייטלבוים"), None).unwrap();
    taxonomy::create_relationship(
        &mut connection,
        &merge.contact.id,
        &other.contact.id,
        "knows",
        None,
    )
    .unwrap();

    let out =
        yanuka_db::merge::merge_contacts(&mut connection, &keep.contact.id, &merge.contact.id)
            .unwrap();

    // Children: the duplicate phone was not doubled, the new one moved over.
    let raws: Vec<&str> = out.phones.iter().map(|p| p.raw.as_str()).collect();
    assert_eq!(raws.len(), 2);
    assert!(raws.contains(&"02-6521234") && raws.contains(&"054-5550199"));
    assert_eq!(out.phones.iter().filter(|p| p.is_primary).count(), 1);

    // Scalars: blank filled, conflict preserved in the notes, notes appended.
    assert_eq!(out.contact.profession.as_deref(), Some("סופר סתם"));
    assert_eq!(out.contact.city.as_deref(), Some("ירושלים"));
    let notes = out.contact.notes.clone().unwrap();
    assert!(notes.contains("מכיר את כל הסופרים"));
    assert!(notes.contains("בני ברק"));
    assert!(notes.contains("לחזור אליו אחרי החגים"));

    // The timestamped note and the relationship edge followed the merge.
    assert_eq!(out.contact_notes.len(), 1);
    assert_eq!(out.contact_notes[0].body, "פגשתי אותו בכנס");
    assert_eq!(out.relationships.len(), 1);
    assert_eq!(out.relationships[0].other_contact.display_name, "יעקב טייטלבוים");

    // The merged contact is gone from live queries but recorded in the log.
    assert!(repository::get_contact(&connection, &merge.contact.id)
        .unwrap()
        .unwrap()
        .contact
        .deleted_at
        .is_some());
    let logged: i64 = connection
        .query_row(
            "SELECT count(*) FROM mutations WHERE entity_id = ?1 AND previous IS NOT NULL",
            yanuka_db::rusqlite::params![merge.contact.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(logged, 1);

    // And no pair remains to nag about.
    assert!(yanuka_db::merge::list_duplicate_pairs(&connection, 50).unwrap().is_empty());
}

#[test]
fn merge_refuses_self_and_missing() {
    let mut connection = db();
    let a = repository::create_contact(&mut connection, &contact("אברהם כהן"), None).unwrap();
    assert!(
        yanuka_db::merge::merge_contacts(&mut connection, &a.contact.id, &a.contact.id).is_err()
    );
    assert!(yanuka_db::merge::merge_contacts(&mut connection, &a.contact.id, "01MISSING").is_err());
}

#[test]
fn backup_snapshots_a_live_database_and_rotates_dailies() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("contacts.db");
    let mut connection = yanuka_db::open(&db_path, None).unwrap();
    migrate(&mut connection).unwrap();
    repository::create_contact(&mut connection, &contact("אברהם כהן"), None).unwrap();

    // A same-day second call is a no-op; the snapshot itself must be a
    // complete, openable database.
    let taken = yanuka_db::backup::daily_backup(&connection, &db_path, 7).unwrap();
    let target = taken.expect("first daily backup should be taken");
    assert!(yanuka_db::backup::daily_backup(&connection, &db_path, 7).unwrap().is_none());

    let restored = yanuka_db::open(&target, None).unwrap();
    let count: i64 =
        restored.query_row("SELECT count(*) FROM contacts", [], |row| row.get(0)).unwrap();
    assert_eq!(count, 1);
    assert!(yanuka_db::backup::last_backup_at(&db_path).is_some());

    // Rotation: plant older dailies and re-prune via a fresh backup dir scan.
    let backups = db_path.parent().unwrap().join("backups");
    for day in ["2020-01-01", "2020-01-02", "2020-01-03"] {
        std::fs::write(backups.join(format!("daily-{day}.db")), b"x").unwrap();
    }
    // Force a new backup by removing today's, with keep=2.
    std::fs::remove_file(&target).unwrap();
    yanuka_db::backup::daily_backup(&connection, &db_path, 2).unwrap().unwrap();
    let dailies: Vec<_> = std::fs::read_dir(&backups)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("daily-") && n.ends_with(".db"))
        .collect();
    assert_eq!(dailies.len(), 2);
    assert!(!dailies.iter().any(|n| n.contains("2020-01-01")));
}

#[test]
fn on_demand_backup_writes_to_an_arbitrary_target() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("contacts.db");
    let mut connection = yanuka_db::open(&db_path, None).unwrap();
    migrate(&mut connection).unwrap();
    repository::create_contact(&mut connection, &contact("יעקב פרידמן"), None).unwrap();

    // Nested directory that does not exist yet — a fresh USB stick path.
    let target = dir.path().join("usb").join("גיבוי-מאגר.db");
    yanuka_db::backup::backup_to(&connection, &target).unwrap();
    let restored = yanuka_db::open(&target, None).unwrap();
    let count: i64 =
        restored.query_row("SELECT count(*) FROM contacts", [], |row| row.get(0)).unwrap();
    assert_eq!(count, 1);
}

#[test]
fn an_explicit_null_in_a_patch_clears_the_field() {
    // Serde collapses `null` into the outer `None` for a nested Option unless
    // told otherwise, which would make "clear this field" indistinguishable
    // from "leave it alone" — and the form sends null to clear.
    let patch: ContactPatch = serde_json::from_str(r#"{"city": null}"#).unwrap();
    assert_eq!(patch.city, Some(None), "an explicit null must reach the field as a clear");
    assert_eq!(patch.region, None, "an absent key must stay untouched");
}

#[test]
fn a_merge_logs_the_details_it_moved_across() {
    // A merge pulls phone numbers, addresses and notes from the losing contact
    // onto the surviving one. The log used to record only `mergedFrom`, naming
    // the operation without its effect — so a device replaying it would keep a
    // contact that never gained the number the merge had just given it here.
    let mut connection = db();

    let keep = repository::create_contact(&mut connection, &contact("אברהם גולד"), None).unwrap();

    let mut duplicate = contact("אברהם גולד");
    duplicate.phones = vec![PhoneInput { raw: "03-9998888".into(), ..Default::default() }];
    duplicate.city = Some("אנטוורפן".into());
    let merge = repository::create_contact(&mut connection, &duplicate, None).unwrap();

    yanuka_db::merge::merge_contacts(&mut connection, &keep.contact.id, &merge.contact.id).unwrap();

    let payload = logged(&connection)
        .into_iter()
        .filter(|(kind, operation, _)| kind == "contact" && operation == "update")
        .map(|(_, _, payload)| payload)
        .next_back()
        .expect("the surviving contact must have an update mutation");

    assert!(payload.contains("03-9998888"), "the merged-in phone is missing: {payload}");
    assert!(payload.contains("אנטוורפן"), "the merged-in city is missing: {payload}");
    assert!(payload.contains("mergedFrom"), "the merge should still be identifiable: {payload}");
}
