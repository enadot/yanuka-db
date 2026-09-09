//! The whole protocol in one process: two devices and a server, three
//! in-memory databases, no network. What the transport would carry over HTTP
//! is carried by function calls; everything else — dirty detection, merge
//! rules, conflicts, tombstones, the stream — is the production code.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use serde_json::json;
use yanuka_db::models::{CategoryInput, ContactInput, PhoneInput};
use yanuka_db::sync::{self, conflicts, engine, hub, PullPage, PushItem, PushResult, Transport};
use yanuka_db::{categories, merge, migrate, open_in_memory, repository, taxonomy, DbError};

type Hub = Rc<RefCell<yanuka_db::rusqlite::Connection>>;

fn db() -> RefCell<yanuka_db::rusqlite::Connection> {
    let mut connection = open_in_memory().expect("open");
    migrate(&mut connection).expect("migrate");
    RefCell::new(connection)
}

fn hub() -> Hub {
    Rc::new(db())
}

/// A device's line to the server.
struct Wire<'a> {
    hub: Hub,
    /// The device's own database, watched but never used: see
    /// `assert_database_released`.
    client: &'a RefCell<yanuka_db::rusqlite::Connection>,
    device: String,
    revoked: Cell<bool>,
    /// The server cannot be reached at all — no network, not a refusal.
    offline: Cell<bool>,
}

impl<'a> Wire<'a> {
    fn new(hub: &Hub, client: &'a RefCell<yanuka_db::rusqlite::Connection>) -> Self {
        let device = repository::device_id(&client.borrow()).unwrap();
        Self {
            hub: hub.clone(),
            client,
            device,
            revoked: Cell::new(false),
            offline: Cell::new(false),
        }
    }

    /// The promise docs/SYNC.md makes: the engine never holds the database
    /// while the transport is on the wire, so a save is never queued behind
    /// a slow link (ADR-041). With a `RefCell` this is deterministic — a
    /// borrow still alive here means `run_cycle` called us from inside
    /// `with()` — and every test in this file checks it on every call.
    fn assert_database_released(&self) {
        assert!(
            self.client.try_borrow_mut().is_ok(),
            "the engine must not hold the database while the transport is on the wire"
        );
    }

    fn gate(&self) -> yanuka_db::Result<()> {
        self.assert_database_released();
        if self.offline.get() {
            return Err(DbError::Sync("אין רשת".into()));
        }
        if self.revoked.get() {
            return Err(DbError::Unauthorized);
        }
        Ok(())
    }
}

impl Transport for Wire<'_> {
    fn push(&self, items: &[PushItem]) -> yanuka_db::Result<Vec<PushResult>> {
        self.gate()?;
        hub::accept(&mut self.hub.borrow_mut(), &self.device, items)
    }

    fn pull(&self, after: i64, limit: usize) -> yanuka_db::Result<PullPage> {
        self.gate()?;
        hub::pull(&self.hub.borrow(), after, limit, &self.device)
    }
}

fn cycle(
    client: &RefCell<yanuka_db::rusqlite::Connection>,
    wire: &Wire<'_>,
) -> engine::CycleReport {
    sync::run_cycle(client, wire, None).expect("cycle")
}

fn contact(display_name: &str, phone: &str) -> ContactInput {
    ContactInput {
        display_name: display_name.to_string(),
        city: Some("ירושלים".into()),
        profession: Some("סופר סת\"ם".into()),
        phones: vec![PhoneInput { raw: phone.into(), ..Default::default() }],
        ..Default::default()
    }
}

fn phone_of(client: &RefCell<yanuka_db::rusqlite::Connection>, id: &str) -> Vec<String> {
    let found = repository::get_contact(&client.borrow(), id).unwrap().expect("contact exists");
    found.phones.iter().map(|p| p.raw.clone()).collect()
}

fn open_conflicts(client: &RefCell<yanuka_db::rusqlite::Connection>) -> i64 {
    conflicts::open_count(&client.borrow()).unwrap()
}

fn pending(client: &RefCell<yanuka_db::rusqlite::Connection>) -> i64 {
    yanuka_db::mutation::pending_count(&client.borrow()).unwrap()
}

#[test]
fn a_first_pairing_sends_the_archive_and_a_second_device_receives_it_whole() {
    let hub = hub();
    let a = db();
    let b = db();
    let wire_a = Wire::new(&hub, &a);
    let wire_b = Wire::new(&hub, &b);

    // Device A has an archive: a tag, a category, two contacts, a note and a
    // relationship — every entity type, with the references between them.
    let (rabbi, scribe) = {
        let mut conn = a.borrow_mut();
        let tag = taxonomy::create_tag(&mut conn, "תורם", Some("#aa0000")).unwrap();
        let shelf = categories::create_category(
            &mut conn,
            &CategoryInput { name: "מדף לבדיקה".into(), ..Default::default() },
            None,
        )
        .unwrap();
        let mut input = contact("הרב יעקב לוי", "050-1111111");
        input.tag_ids = vec![tag.id.clone()];
        input.category_ids = vec![shelf.id.clone()];
        let rabbi = repository::create_contact(&mut conn, &input, None).unwrap();
        let scribe =
            repository::create_contact(&mut conn, &contact("ישראל סופר", "052-2222222"), None)
                .unwrap();
        taxonomy::add_note(&mut conn, &rabbi.contact.id, "הכיר אותנו דרך הישיבה", false).unwrap();
        taxonomy::create_relationship(
            &mut conn,
            &rabbi.contact.id,
            &scribe.contact.id,
            "recommended",
            None,
        )
        .unwrap();
        (rabbi.contact.id, scribe.contact.id)
    };
    assert!(pending(&a) > 0, "local writes are journaled before any sync");

    let report = cycle(&a, &wire_a);
    assert_eq!(report.pulled, 0);
    assert!(report.pushed >= 6, "tag, category, two contacts, note, relationship: {report:?}");
    assert_eq!(report.rejected, 0);
    assert_eq!(pending(&a), 0, "everything acknowledged");

    // A second, empty device pulls the archive.
    let report = cycle(&b, &wire_b);
    assert_eq!(report.applied, report.pulled);
    assert!(report.applied >= 6);
    assert_eq!(report.pushed, 0, "nothing of its own to send");

    let got = repository::get_contact(&b.borrow(), &rabbi).unwrap().expect("rabbi arrived");
    assert_eq!(got.contact.display_name, "הרב יעקב לוי");
    assert_eq!(got.phones.len(), 1);
    assert_eq!(got.phones[0].raw, "050-1111111");
    assert_eq!(got.tags.len(), 1);
    assert_eq!(got.tags[0].name, "תורם");
    assert_eq!(got.categories.len(), 1);
    assert_eq!(got.categories[0].category.name, "מדף לבדיקה");
    assert_eq!(got.categories[0].membership, "manual");
    assert_eq!(got.contact_notes.len(), 1);
    assert_eq!(got.contact_notes[0].body, "הכיר אותנו דרך הישיבה");
    assert_eq!(got.relationships.len(), 1);
    assert_eq!(got.relationships[0].other_contact.id, scribe);

    // Search on B finds the pulled contact: the index was rebuilt on apply.
    let hits = yanuka_db::search::search(
        &b.borrow(),
        &yanuka_db::models::SearchQuery { text: "לוי".into(), ..Default::default() },
    )
    .unwrap();
    assert_eq!(hits.results.len(), 1);

    // The card history on B says device A made the record.
    let history = yanuka_db::mutation::history(&b.borrow(), Some(&rabbi), 20).unwrap();
    assert!(history.iter().any(|e| e["deviceId"] == json!(wire_a.device)));

    // A second cycle on either side is quiet.
    let report = cycle(&b, &wire_b);
    assert_eq!((report.pulled, report.applied, report.pushed), (0, 0, 0));
    let report = cycle(&a, &wire_a);
    assert_eq!(report.applied, 0, "A does not re-apply its own pushes: {report:?}");
    assert_eq!(report.pushed, 0);
}

#[test]
fn edits_to_different_fields_of_one_record_merge_without_a_conflict() {
    let hub = hub();
    let a = db();
    let b = db();
    let wire_a = Wire::new(&hub, &a);
    let wire_b = Wire::new(&hub, &b);
    let id = repository::create_contact(&mut a.borrow_mut(), &contact("משה כהן", "050-1"), None)
        .unwrap()
        .contact
        .id;
    cycle(&a, &wire_a);
    cycle(&b, &wire_b);

    // Offline, A changes the city and B the profession.
    {
        let mut conn = a.borrow_mut();
        let current = repository::get_contact(&conn, &id).unwrap().unwrap();
        let mut input = contact("משה כהן", "050-1");
        input.city = Some("בני ברק".into());
        repository::update_contact(&mut conn, &id, &input, Some(current.contact.version)).unwrap();
    }
    {
        let mut conn = b.borrow_mut();
        let current = repository::get_contact(&conn, &id).unwrap().unwrap();
        let mut input = contact("משה כהן", "050-1");
        input.profession = Some("דיין".into());
        repository::update_contact(&mut conn, &id, &input, Some(current.contact.version)).unwrap();
    }

    cycle(&a, &wire_a);
    let report = cycle(&b, &wire_b);
    assert_eq!(report.conflicts, 0);
    assert_eq!(report.pushed, 1, "B's profession went up after merging A's city: {report:?}");
    let on_b = repository::get_contact(&b.borrow(), &id).unwrap().unwrap();
    assert_eq!(on_b.contact.city.as_deref(), Some("בני ברק"));
    assert_eq!(on_b.contact.profession.as_deref(), Some("דיין"));

    cycle(&a, &wire_a);
    let on_a = repository::get_contact(&a.borrow(), &id).unwrap().unwrap();
    assert_eq!(on_a.contact.city.as_deref(), Some("בני ברק"));
    assert_eq!(on_a.contact.profession.as_deref(), Some("דיין"));
    assert_eq!(open_conflicts(&a) + open_conflicts(&b), 0);
}

#[test]
fn the_same_field_changed_on_both_devices_is_kept_for_a_person_to_choose() {
    let hub = hub();
    let a = db();
    let b = db();
    let wire_a = Wire::new(&hub, &a);
    let wire_b = Wire::new(&hub, &b);
    let id = repository::create_contact(&mut a.borrow_mut(), &contact("דוד לוי", "050-0000"), None)
        .unwrap()
        .contact
        .id;
    cycle(&a, &wire_a);
    cycle(&b, &wire_b);

    let edit_phone = |client: &RefCell<yanuka_db::rusqlite::Connection>, phone: &str| {
        let mut conn = client.borrow_mut();
        let current = repository::get_contact(&conn, &id).unwrap().unwrap();
        repository::update_contact(
            &mut conn,
            &id,
            &contact("דוד לוי", phone),
            Some(current.contact.version),
        )
        .unwrap();
    };
    edit_phone(&a, "054-1111111");
    edit_phone(&b, "052-2222222");

    cycle(&a, &wire_a);
    let report = cycle(&b, &wire_b);
    assert_eq!(report.conflicts, 1, "{report:?}");
    assert_eq!(report.pushed, 0, "a held record is not pushed over the other side");
    assert_eq!(phone_of(&b, &id), vec!["052-2222222"], "B keeps its own keystrokes meanwhile");
    assert_eq!(phone_of(&a, &id), vec!["054-1111111"]);

    let open = conflicts::list(&b.borrow()).unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0]["entityLabel"], json!("דוד לוי"));
    let fields = open[0]["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0]["field"], json!("phones"));
    assert_eq!(fields[0]["remoteDeviceId"], json!(wire_a.device));

    // The person picks the other device's value.
    let conflict_id = open[0]["id"].as_str().unwrap().to_string();
    conflicts::resolve(&mut b.borrow_mut(), &conflict_id, "remote").unwrap();
    assert_eq!(phone_of(&b, &id), vec!["054-1111111"]);
    assert_eq!(open_conflicts(&b), 0);
    let report = cycle(&b, &wire_b);
    assert_eq!(report.conflicts, 0);
    cycle(&a, &wire_a);
    assert_eq!(phone_of(&a, &id), vec!["054-1111111"], "A is untouched: it already had this");

    // And the other way round: this time B's word wins and travels to A.
    edit_phone(&a, "054-3333333");
    edit_phone(&b, "052-4444444");
    cycle(&a, &wire_a);
    cycle(&b, &wire_b);
    let open = conflicts::list(&b.borrow()).unwrap();
    assert_eq!(open.len(), 1);
    let conflict_id = open[0]["id"].as_str().unwrap().to_string();
    conflicts::resolve(&mut b.borrow_mut(), &conflict_id, "local").unwrap();
    let report = cycle(&b, &wire_b);
    assert_eq!(report.pushed, 1, "the chosen local value is pushed: {report:?}");
    cycle(&a, &wire_a);
    assert_eq!(phone_of(&a, &id), vec!["052-4444444"]);
    assert_eq!(phone_of(&b, &id), vec!["052-4444444"]);
    assert_eq!(open_conflicts(&a), 0, "A had nothing unsent, so nothing collided there");
}

#[test]
fn a_deletion_travels_as_a_tombstone_and_a_restore_brings_it_back() {
    let hub = hub();
    let a = db();
    let b = db();
    let wire_a = Wire::new(&hub, &a);
    let wire_b = Wire::new(&hub, &b);
    let id = repository::create_contact(&mut a.borrow_mut(), &contact("שרה מזרחי", "1"), None)
        .unwrap()
        .contact
        .id;
    cycle(&a, &wire_a);
    cycle(&b, &wire_b);

    repository::delete_contact(&mut a.borrow_mut(), &id).unwrap();
    cycle(&a, &wire_a);
    cycle(&b, &wire_b);
    let on_b = repository::get_contact(&b.borrow(), &id).unwrap().unwrap();
    assert!(on_b.contact.deleted_at.is_some(), "in the trash on B, not gone");
    let trash = repository::list_deleted_contacts(&b.borrow(), 10).unwrap();
    assert_eq!(trash.len(), 1);

    repository::restore_contact(&mut b.borrow_mut(), &id).unwrap();
    cycle(&b, &wire_b);
    cycle(&a, &wire_a);
    let on_a = repository::get_contact(&a.borrow(), &id).unwrap().unwrap();
    assert!(on_a.contact.deleted_at.is_none(), "restored on A too");
}

#[test]
fn a_merge_on_one_device_arrives_whole_on_the_other() {
    let hub = hub();
    let a = db();
    let b = db();
    let wire_a = Wire::new(&hub, &a);
    let wire_b = Wire::new(&hub, &b);
    let (keep, gone) = {
        let mut conn = a.borrow_mut();
        let keep = repository::create_contact(&mut conn, &contact("אברהם כץ", "050-5"), None)
            .unwrap()
            .contact
            .id;
        let gone = repository::create_contact(&mut conn, &contact("אברהם כץ", "02-6"), None)
            .unwrap()
            .contact
            .id;
        (keep, gone)
    };
    cycle(&a, &wire_a);
    cycle(&b, &wire_b);

    merge::merge_contacts(&mut a.borrow_mut(), &keep, &gone).unwrap();
    cycle(&a, &wire_a);
    cycle(&b, &wire_b);

    let kept = repository::get_contact(&b.borrow(), &keep).unwrap().unwrap();
    let mut phones = phone_of(&b, &keep);
    phones.sort();
    assert_eq!(phones, vec!["02-6", "050-5"], "the merged phone moved over: {kept:?}");
    let merged = repository::get_contact(&b.borrow(), &gone).unwrap().unwrap();
    assert!(merged.contact.deleted_at.is_some());
}

#[test]
fn notes_and_categories_follow_their_own_rules_across_devices() {
    let hub = hub();
    let a = db();
    let b = db();
    let wire_a = Wire::new(&hub, &a);
    let wire_b = Wire::new(&hub, &b);
    let id = repository::create_contact(&mut a.borrow_mut(), &contact("חיים פרץ", "1"), None)
        .unwrap()
        .contact
        .id;
    let note = taxonomy::add_note(&mut a.borrow_mut(), &id, "טיוטה", false).unwrap();
    // A rule category: membership is computed on each device, never sent.
    let shelf = categories::create_category(
        &mut a.borrow_mut(),
        &CategoryInput {
            name: "סופרים".into(),
            rule: Some(
                serde_json::from_value(json!({
                    "match": "all",
                    "conditions": [{ "field": "occupation", "op": "contains", "values": ["סופר"] }]
                }))
                .unwrap(),
            ),
            ..Default::default()
        },
        None,
    )
    .unwrap();
    cycle(&a, &wire_a);
    cycle(&b, &wire_b);

    let on_b = repository::get_contact(&b.borrow(), &id).unwrap().unwrap();
    assert_eq!(on_b.categories.len(), 1, "the rule was evaluated on B: {:?}", on_b.categories);
    assert_eq!(on_b.categories[0].category.id, shelf.id);
    assert_eq!(on_b.categories[0].membership, "rule");

    // B rewrites the note; A takes it off the shelf by hand. Both travel.
    taxonomy::update_note(&mut b.borrow_mut(), &note, "נוסח סופי", None).unwrap();
    categories::set_membership(&mut a.borrow_mut(), &shelf.id, &id, "exclude").unwrap();
    cycle(&b, &wire_b);
    cycle(&a, &wire_a);
    cycle(&b, &wire_b);

    let on_a = repository::get_contact(&a.borrow(), &id).unwrap().unwrap();
    assert_eq!(on_a.contact_notes[0].body, "נוסח סופי");
    let on_b = repository::get_contact(&b.borrow(), &id).unwrap().unwrap();
    assert!(on_b.categories.is_empty(), "the exclusion reached B: {:?}", on_b.categories);
    assert_eq!(open_conflicts(&a) + open_conflicts(&b), 0);

    // Deleting the note tombstones it on the other side.
    taxonomy::delete_note(&mut a.borrow_mut(), &note).unwrap();
    cycle(&a, &wire_a);
    cycle(&b, &wire_b);
    let on_b = repository::get_contact(&b.borrow(), &id).unwrap().unwrap();
    assert!(on_b.contact_notes.is_empty());
}

#[test]
fn the_hub_refuses_a_stale_base_and_a_dangling_reference() {
    let hub = hub();
    let a = db();
    let wire_a = Wire::new(&hub, &a);
    let id = repository::create_contact(&mut a.borrow_mut(), &contact("x", "1"), None)
        .unwrap()
        .contact
        .id;
    cycle(&a, &wire_a);

    let state = sync::state::load(&a.borrow(), "contact", &id).unwrap().unwrap();
    let stale = PushItem {
        entity_type: "contact".into(),
        entity_id: id.clone(),
        base_version: 0,
        changed: vec!["city".into()],
        state,
        created_at: yanuka_db::now_iso(),
    };
    let results = hub::accept(&mut hub.borrow_mut(), "other-device", &[stale]).unwrap();
    assert!(!results[0].accepted);
    assert_eq!(results[0].version, 1, "the hub reports where it stands");

    let orphan = PushItem {
        entity_type: "note".into(),
        entity_id: yanuka_db::new_id(),
        base_version: 0,
        changed: vec!["body".into()],
        state: json!({ "contactId": "no-such-contact", "body": "…" }),
        created_at: yanuka_db::now_iso(),
    };
    let results = hub::accept(&mut hub.borrow_mut(), "other-device", &[orphan]).unwrap();
    assert!(!results[0].accepted);
    assert_eq!(results[0].error.as_deref(), Some("missing_reference"));
    assert_eq!(hub::head(&hub.borrow()).unwrap(), 1, "nothing was appended to the stream");
}

#[test]
fn a_revoked_device_is_told_so_and_keeps_its_data() {
    let hub = hub();
    let a = db();
    let wire_a = Wire::new(&hub, &a);
    repository::create_contact(&mut a.borrow_mut(), &contact("y", "1"), None).unwrap();
    wire_a.revoked.set(true);
    let error = sync::run_cycle(&a, &wire_a, None).unwrap_err();
    assert!(matches!(error, DbError::Unauthorized));
    let status = sync::config::status(&a.borrow()).unwrap();
    assert!(status["lastError"].as_str().unwrap().contains("לצמד"));
    assert_eq!(pending(&a), 1, "the unsent change is still waiting, not lost");
}

#[test]
fn pairing_codes_and_tokens_are_one_time_and_hashed() {
    let hub = hub();
    let (code, _expires) = sync::devices::create_pair_code(&hub.borrow()).unwrap();
    assert_eq!(code.len(), 9);
    let device = sync::devices::DeviceInfo {
        id: "dev-1".into(),
        name: "המחשב במשרד".into(),
        kind: "desktop".into(),
        platform: Some("windows".into()),
        app_version: Some("0.10.0".into()),
    };
    // Typed loosely: lowercase, without the dash.
    let typed = code.replace('-', "").to_lowercase();
    let token = sync::devices::redeem_pair_code(&mut hub.borrow_mut(), &typed, &device).unwrap();
    assert_eq!(token.len(), 64);
    assert!(
        matches!(
            sync::devices::redeem_pair_code(&mut hub.borrow_mut(), &code, &device),
            Err(DbError::Unauthorized)
        ),
        "a code is spent on first use"
    );
    assert_eq!(sync::devices::authenticate(&hub.borrow(), &token).unwrap(), "dev-1");
    assert!(matches!(
        sync::devices::authenticate(&hub.borrow(), "not-a-token"),
        Err(DbError::Unauthorized)
    ));
    let stored: Option<String> = hub
        .borrow()
        .query_row("SELECT token_hash FROM devices WHERE id = 'dev-1'", [], |row| row.get(0))
        .unwrap();
    assert_ne!(stored.as_deref(), Some(token.as_str()), "only the digest is stored");

    sync::devices::revoke(&hub.borrow(), "dev-1").unwrap();
    assert!(matches!(
        sync::devices::authenticate(&hub.borrow(), &token),
        Err(DbError::Unauthorized)
    ));
    assert_eq!(sync::devices::active_count(&hub.borrow()).unwrap(), 0);
}

#[test]
fn disconnecting_forgets_the_server_but_keeps_every_record_and_its_history() {
    let hub = hub();
    let a = db();
    let wire_a = Wire::new(&hub, &a);
    let id = repository::create_contact(&mut a.borrow_mut(), &contact("z", "1"), None)
        .unwrap()
        .contact
        .id;
    sync::config::save_config(
        &a.borrow(),
        &sync::config::SyncConfig {
            server_url: "http://server".into(),
            token: "t".into(),
            server_name: Some("בית".into()),
            device_name: None,
        },
    )
    .unwrap();
    cycle(&a, &wire_a);
    assert!(sync::config::load_config(&a.borrow()).unwrap().is_some());

    sync::config::clear_config(&mut a.borrow_mut()).unwrap();
    assert!(sync::config::load_config(&a.borrow()).unwrap().is_none());
    assert!(repository::get_contact(&a.borrow(), &id).unwrap().is_some());
    assert!(!yanuka_db::mutation::history(&a.borrow(), Some(&id), 10).unwrap().is_empty());
    // A later pairing starts from "the server has never seen this".
    assert_eq!(engine::dirty(&a.borrow()).unwrap().len(), 1);
}

#[test]
fn a_cycle_that_cannot_reach_the_server_leaves_local_work_untouched() {
    let hub = hub();
    let a = db();
    let wire_a = Wire::new(&hub, &a);
    let first = repository::create_contact(&mut a.borrow_mut(), &contact("אסתר גולן", "1"), None)
        .unwrap()
        .contact
        .id;

    // No network: the cycle fails, says so, and leaves nothing half-done.
    wire_a.offline.set(true);
    let error = sync::run_cycle(&a, &wire_a, None).unwrap_err();
    assert!(matches!(error, DbError::Sync(_)), "{error:?}");
    let status = sync::config::status(&a.borrow()).unwrap();
    assert!(!status["lastError"].as_str().unwrap_or_default().is_empty());
    assert!(a.borrow().is_autocommit(), "no transaction leaked out of the failed cycle");
    assert_eq!(pending(&a), 1, "the unsent change is still waiting, not lost");

    // Offline is the archive's normal day: adding and editing go on as before.
    let second = repository::create_contact(&mut a.borrow_mut(), &contact("יונה שפירא", "2"), None)
        .unwrap()
        .contact
        .id;
    {
        let mut conn = a.borrow_mut();
        let current = repository::get_contact(&conn, &first).unwrap().unwrap();
        let mut input = contact("אסתר גולן", "1");
        input.city = Some("צפת".into());
        repository::update_contact(&mut conn, &first, &input, Some(current.contact.version))
            .unwrap();
    }
    let on_a = repository::get_contact(&a.borrow(), &first).unwrap().unwrap();
    assert_eq!(on_a.contact.city.as_deref(), Some("צפת"));
    assert!(repository::get_contact(&a.borrow(), &second).unwrap().is_some());
    assert!(pending(&a) > 1, "the offline edits are journaled for later");
    let error = sync::run_cycle(&a, &wire_a, None).unwrap_err();
    assert!(matches!(error, DbError::Sync(_)), "still offline, still failing cleanly");

    // The day the link is back, everything goes up in one cycle.
    wire_a.offline.set(false);
    let report = cycle(&a, &wire_a);
    assert_eq!(report.pushed, 2, "two records, whatever the number of edits: {report:?}");
    assert_eq!(report.rejected, 0);
    assert_eq!(pending(&a), 0);
    let status = sync::config::status(&a.borrow()).unwrap();
    assert!(status["lastError"].is_null(), "the error clears with the next good cycle");
}

#[test]
fn the_engine_releases_the_database_before_every_network_call() {
    let hub = hub();
    let a = db();
    let b = db();
    let wire_a = Wire::new(&hub, &a);
    let wire_b = Wire::new(&hub, &b);

    // More records than one push chunk (100) and one pull page (200), so
    // every round of both loops runs — and the wire checks, on each call,
    // that the device's database is free while it is on the line.
    {
        let mut conn = a.borrow_mut();
        for i in 0..250 {
            repository::create_contact(
                &mut conn,
                &contact(&format!("איש {i}"), &format!("{i}")),
                None,
            )
            .unwrap();
        }
    }
    let report = cycle(&a, &wire_a);
    assert_eq!(report.pushed, 250, "{report:?}");
    assert_eq!(report.rejected, 0);

    let report = cycle(&b, &wire_b);
    assert_eq!(report.pulled, 250, "two pages: {report:?}");
    assert_eq!(report.applied, 250);
    let on_b: i64 =
        b.borrow().query_row("SELECT COUNT(*) FROM contacts", [], |row| row.get(0)).unwrap();
    assert_eq!(on_b, 250);
}

#[test]
fn a_record_held_by_a_conflict_can_still_be_edited_offline() {
    let hub = hub();
    let a = db();
    let b = db();
    let wire_a = Wire::new(&hub, &a);
    let wire_b = Wire::new(&hub, &b);
    let id = repository::create_contact(&mut a.borrow_mut(), &contact("נעמי אדלר", "050-0"), None)
        .unwrap()
        .contact
        .id;
    cycle(&a, &wire_a);
    cycle(&b, &wire_b);

    let edit_phone = |client: &RefCell<yanuka_db::rusqlite::Connection>, phone: &str| {
        let mut conn = client.borrow_mut();
        let current = repository::get_contact(&conn, &id).unwrap().unwrap();
        repository::update_contact(
            &mut conn,
            &id,
            &contact("נעמי אדלר", phone),
            Some(current.contact.version),
        )
        .unwrap();
    };
    edit_phone(&a, "054-1111111");
    edit_phone(&b, "052-2222222");
    cycle(&a, &wire_a);
    let report = cycle(&b, &wire_b);
    assert_eq!(report.conflicts, 1, "{report:?}");

    // Nobody is there to settle it, and the record is needed now: B keeps
    // working on it. The edit lands locally and waits with the conflict.
    edit_phone(&b, "052-9999999");
    assert_eq!(phone_of(&b, &id), vec!["052-9999999"]);
    let report = cycle(&b, &wire_b);
    assert_eq!(report.pushed, 0, "held until a person decides: {report:?}");
    assert_eq!(open_conflicts(&b), 1);
    assert_eq!(phone_of(&a, &id), vec!["054-1111111"], "A never sees the held edits");

    // The person looks at the card as it stands and says: this is right.
    let open = conflicts::list(&b.borrow()).unwrap();
    let conflict_id = open[0]["id"].as_str().unwrap().to_string();
    conflicts::resolve(&mut b.borrow_mut(), &conflict_id, "manual").unwrap();
    assert_eq!(open_conflicts(&b), 0);
    let report = cycle(&b, &wire_b);
    assert_eq!(report.pushed, 1, "the record goes up as it stands: {report:?}");
    assert_eq!(report.conflicts, 0);
    cycle(&a, &wire_a);
    assert_eq!(phone_of(&a, &id), vec!["052-9999999"]);
    assert_eq!(open_conflicts(&a), 0);
}
