//! Two devices, one archive.
//!
//! Every test here runs two real SQLite databases and moves mutations between
//! them by hand, which is exactly what a sync engine will do once there is a
//! transport. Nothing is mocked: the rows, the merge and the conflict detection
//! are the production ones.
//!
//! What these are guarding is not "does sync run" — it is the far narrower and
//! far more important question of whether anything a person typed can vanish
//! while it runs. A sync bug does not look like a crash. It looks like a phone
//! number that used to be there.

use yanuka_db::apply::{self, Applied};
use yanuka_db::models::{ContactInput, ContactPatch, PhoneInput};
use yanuka_db::{migrate, open_in_memory, repository, taxonomy};

type Db = yanuka_db::rusqlite::Connection;

fn db() -> Db {
    let mut connection = open_in_memory().expect("open");
    migrate(&mut connection).expect("migrate");
    connection
}

/// Two devices with distinct identities.
///
/// The device id is minted on first use and stamped onto every write, so a pair
/// of databases that never called `device_id` would both report `None` and the
/// conflict records could not say which machine produced which value.
fn pair() -> (Db, Db) {
    let a = db();
    let b = db();
    repository::device_id(&a).unwrap();
    repository::device_id(&b).unwrap();
    (a, b)
}

fn contact(display_name: &str) -> ContactInput {
    ContactInput { display_name: display_name.to_string(), ..Default::default() }
}

/// Push everything `from` has not yet sent into `to`.
///
/// Loops while anything was deferred, because a note can legitimately arrive
/// before the contact it belongs to and become applicable only once the earlier
/// mutation lands. Returns the outcomes so a test can assert on them.
fn deliver(from: &Db, to: &mut Db) -> Vec<Applied> {
    let mut outcomes = Vec::new();
    loop {
        let queue = apply::pending(from, 500).unwrap();
        let mut progressed = false;
        let mut deferred = 0;

        for mutation in &queue {
            let outcome = apply::apply(to, mutation).unwrap();
            match outcome {
                Applied::Deferred => deferred += 1,
                Applied::AlreadySeen => {}
                _ => progressed = true,
            }
            outcomes.push(outcome);
        }

        if deferred == 0 || !progressed {
            assert_eq!(deferred, 0, "a mutation never became applicable: {queue:?}");
            return outcomes;
        }
    }
}

/// The data both devices are supposed to agree on, in a comparable form.
///
/// Version counters and device ids are left out on purpose: they legitimately
/// differ between two machines that reached the same state by different routes.
/// What must match is what the user would see.
fn snapshot(connection: &Db) -> String {
    let mut out = String::new();
    for (label, query) in [
        (
            "contact",
            "SELECT display_name, COALESCE(city, ''), COALESCE(profession, ''),
                    COALESCE(deleted_at, '')
               FROM contacts ORDER BY id",
        ),
        ("phone", "SELECT raw FROM contact_phones WHERE deleted_at IS NULL ORDER BY raw"),
        ("email", "SELECT address FROM contact_emails WHERE deleted_at IS NULL ORDER BY address"),
        ("note", "SELECT body FROM notes WHERE deleted_at IS NULL ORDER BY body"),
        (
            "edge",
            "SELECT type, COALESCE(notes, '') FROM relationships
              WHERE deleted_at IS NULL ORDER BY id",
        ),
        ("tag", "SELECT name FROM tags WHERE deleted_at IS NULL ORDER BY name"),
        ("organization", "SELECT name FROM organizations WHERE deleted_at IS NULL ORDER BY name"),
    ] {
        let mut statement = connection.prepare(query).unwrap();
        let columns = statement.column_count();
        let rows = statement
            .query_map([], |row| {
                let mut cells = Vec::new();
                for index in 0..columns {
                    cells.push(row.get::<_, String>(index)?);
                }
                Ok(cells.join("|"))
            })
            .unwrap();
        for row in rows {
            out.push_str(label);
            out.push(' ');
            out.push_str(&row.unwrap());
            out.push('\n');
        }
    }
    out
}

#[test]
fn a_contact_created_on_one_device_arrives_whole_on_the_other() {
    // The baseline the whole feature rests on. Before ADR-033 this test would
    // have passed on the name alone and failed on everything else, which is
    // precisely the shape of the bug it now guards.
    let (mut a, mut b) = pair();

    let mut input = contact("שרה כהן");
    input.city = Some("ירושלים".into());
    input.phones = vec![PhoneInput { raw: "052-1234567".into(), ..Default::default() }];
    let created = repository::create_contact(&mut a, &input, None).unwrap();
    taxonomy::add_note(&mut a, &created.contact.id, "הכרנו דרך הרב", false).unwrap();

    deliver(&a, &mut b);

    let arrived = repository::get_contact(&b, &created.contact.id).unwrap().expect("contact");
    assert_eq!(arrived.contact.display_name, "שרה כהן");
    assert_eq!(arrived.contact.city.as_deref(), Some("ירושלים"));
    assert_eq!(arrived.phones.len(), 1, "the phone number did not survive the trip");
    assert_eq!(arrived.phones[0].raw, "052-1234567");
    assert_eq!(arrived.contact_notes.len(), 1);
    assert_eq!(arrived.contact_notes[0].body, "הכרנו דרך הרב");
}

#[test]
fn edits_to_different_fields_of_one_contact_both_survive() {
    // The payoff for a field-level log. Two people, both offline, both editing
    // the same person — one adds the city, the other the profession. Neither
    // edit may cost the other, and neither should need a human.
    let (mut a, mut b) = pair();
    let created = repository::create_contact(&mut a, &contact("משה לוי"), None).unwrap();
    let id = created.contact.id.clone();
    deliver(&a, &mut b);

    repository::update_contact(
        &mut a,
        &id,
        &ContactPatch { city: Some(Some("בני ברק".into())), ..Default::default() },
        None,
    )
    .unwrap();
    repository::update_contact(
        &mut b,
        &id,
        &ContactPatch { profession: Some(Some("סופר סת\"ם".into())), ..Default::default() },
        None,
    )
    .unwrap();

    deliver(&a, &mut b);
    deliver(&b, &mut a);

    for (name, connection) in [("A", &a), ("B", &b)] {
        let contact = repository::get_contact(connection, &id).unwrap().unwrap().contact;
        assert_eq!(contact.city.as_deref(), Some("בני ברק"), "city lost on {name}");
        assert_eq!(contact.profession.as_deref(), Some("סופר סת\"ם"), "profession lost on {name}");
    }

    let open: i64 = a.query_row("SELECT COUNT(*) FROM conflicts", [], |row| row.get(0)).unwrap();
    assert_eq!(open, 0, "disjoint edits must not need a human");
}

#[test]
fn the_same_field_edited_on_both_devices_is_kept_as_a_conflict() {
    // The case where no rule can be right. Two devices set different cities;
    // whichever arrives second must not overwrite the first in silence. Both
    // values are retained and a human decides — a temporary duplicate is always
    // better than a lost keystroke.
    let (mut a, mut b) = pair();
    let created = repository::create_contact(&mut a, &contact("יעקב פרידמן"), None).unwrap();
    let id = created.contact.id.clone();
    deliver(&a, &mut b);

    repository::update_contact(
        &mut a,
        &id,
        &ContactPatch { city: Some(Some("אנטוורפן".into())), ..Default::default() },
        None,
    )
    .unwrap();
    repository::update_contact(
        &mut b,
        &id,
        &ContactPatch { city: Some(Some("לונדון".into())), ..Default::default() },
        None,
    )
    .unwrap();

    let outcomes = deliver(&a, &mut b);
    assert!(
        outcomes.iter().any(|outcome| matches!(outcome, Applied::Conflicted(fields)
            if fields.iter().any(|field| field == "city"))),
        "the collision was not reported: {outcomes:?}"
    );

    // B keeps what B typed; A's value is not thrown away, it is on record.
    let contact = repository::get_contact(&b, &id).unwrap().unwrap().contact;
    assert_eq!(contact.city.as_deref(), Some("לונדון"));

    let fields: String = b
        .query_row("SELECT fields FROM conflicts WHERE resolved_at IS NULL", [], |row| row.get(0))
        .unwrap();
    assert!(fields.contains("לונדון"), "the local value is missing from the conflict: {fields}");
    assert!(fields.contains("אנטוורפן"), "the remote value was dropped: {fields}");
}

#[test]
fn delivering_the_same_mutation_twice_changes_nothing() {
    // Any real transport is at-least-once: a pull that dies after writing but
    // before acknowledging replays on the retry. Applying twice must be the
    // same as applying once, or a redelivered note becomes two notes.
    let (mut a, mut b) = pair();
    let created = repository::create_contact(&mut a, &contact("אברהם גולד"), None).unwrap();
    taxonomy::add_note(&mut a, &created.contact.id, "מכיר את כולם", false).unwrap();

    deliver(&a, &mut b);
    let before = snapshot(&b);

    let replayed = apply::pending(&a, 500).unwrap();
    for mutation in &replayed {
        assert_eq!(apply::apply(&mut b, mutation).unwrap(), Applied::AlreadySeen);
    }

    assert_eq!(snapshot(&b), before, "a replay changed the database");
}

#[test]
fn a_relationship_waits_for_both_of_its_ends() {
    // Mutations are replayed in creation order, but a queue can still be
    // delivered in pieces. An edge whose far end has not arrived must be held,
    // not written and not discarded: half an edge is worse than no edge,
    // because nothing will ever come back to repair it.
    let (mut a, mut b) = pair();
    let first = repository::create_contact(&mut a, &contact("הרב הממליץ"), None).unwrap();
    let second = repository::create_contact(&mut a, &contact("נחום מלונדון"), None).unwrap();
    taxonomy::create_relationship(
        &mut a,
        &first.contact.id,
        &second.contact.id,
        "recommended",
        Some("בכינוס"),
    )
    .unwrap();

    let queue = apply::pending(&a, 500).unwrap();
    let edge = queue.iter().find(|m| m.entity_type == "relationship").unwrap();

    // Out of order on purpose: the edge before either of its endpoints.
    assert_eq!(apply::apply(&mut b, edge).unwrap(), Applied::Deferred);
    let held: i64 =
        b.query_row("SELECT COUNT(*) FROM relationships", [], |row| row.get(0)).unwrap();
    assert_eq!(held, 0, "an edge was written with a missing endpoint");

    deliver(&a, &mut b);

    let arrived = repository::get_contact(&b, &second.contact.id).unwrap().unwrap();
    assert_eq!(arrived.relationships.len(), 1, "the edge never landed");
    assert_eq!(arrived.relationships[0].other_contact.display_name, "הרב הממליץ");
}

#[test]
fn a_deletion_travels_and_the_record_stays_restorable() {
    // Deletion is a tombstone everywhere, not a removal. It has to reach the
    // other device — otherwise the contact reappears on the next sync — while
    // still leaving something the recycle bin can restore.
    let (mut a, mut b) = pair();
    let created = repository::create_contact(&mut a, &contact("מנדל הנמחק"), None).unwrap();
    let id = created.contact.id.clone();
    deliver(&a, &mut b);

    repository::delete_contact(&mut a, &id).unwrap();
    deliver(&a, &mut b);

    let deleted = repository::get_contact(&b, &id).unwrap().unwrap();
    assert!(deleted.contact.deleted_at.is_some(), "the deletion did not travel");

    let bin = repository::deleted_contacts(&b, 50).unwrap();
    assert_eq!(bin.len(), 1, "the deleted contact is not reachable from the bin on B");

    repository::restore_contact(&mut b, &id).unwrap();
    assert!(repository::get_contact(&b, &id).unwrap().unwrap().contact.deleted_at.is_none());
}

#[test]
fn the_same_institution_created_on_both_devices_does_not_become_two() {
    // Both devices are offline; both add "ישיבת מיר" because both need it. The
    // ids differ, so nothing links them — but the name is the identity here,
    // and letting both land would split every facet count that mentions it and
    // make "who else is from Mir" answer half the truth.
    let (mut a, mut b) = pair();
    taxonomy::create_organization(&mut a, "ישיבת מיר", "yeshiva", Some("ירושלים"), None).unwrap();
    taxonomy::create_organization(&mut b, "ישיבת מיר", "yeshiva", Some("ירושלים"), None).unwrap();

    deliver(&a, &mut b);
    deliver(&b, &mut a);

    for (name, connection) in [("A", &a), ("B", &b)] {
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM organizations WHERE deleted_at IS NULL", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1, "{name} ended up with a duplicate institution");
    }
}

#[test]
fn two_devices_working_independently_converge() {
    // The whole point, stated once. Each device does a spread of real work
    // while out of contact — creating, editing, annotating, connecting,
    // deleting — and after the logs are exchanged in both directions they must
    // describe the same archive.
    let (mut a, mut b) = pair();

    let shared = repository::create_contact(&mut a, &contact("קשר משותף"), None).unwrap();
    deliver(&a, &mut b);

    // A works.
    let mut input = contact("איש של A");
    input.phones = vec![PhoneInput { raw: "02-5551111".into(), ..Default::default() }];
    let from_a = repository::create_contact(&mut a, &input, None).unwrap();
    taxonomy::add_note(&mut a, &shared.contact.id, "הערה מ־A", false).unwrap();
    repository::update_contact(
        &mut a,
        &shared.contact.id,
        &ContactPatch { city: Some(Some("ירושלים".into())), ..Default::default() },
        None,
    )
    .unwrap();
    taxonomy::create_tag(&mut a, "סת\"ם", None).unwrap();

    // B works, on the same archive, with no knowledge of any of that.
    let from_b = repository::create_contact(&mut b, &contact("איש של B"), None).unwrap();
    taxonomy::add_note(&mut b, &shared.contact.id, "הערה מ־B", false).unwrap();
    repository::update_contact(
        &mut b,
        &shared.contact.id,
        &ContactPatch { profession: Some(Some("שוחט".into())), ..Default::default() },
        None,
    )
    .unwrap();
    taxonomy::create_relationship(&mut b, &shared.contact.id, &from_b.contact.id, "knows", None)
        .unwrap();

    // Exchange, twice each way: the second pass carries what the first created.
    deliver(&a, &mut b);
    deliver(&b, &mut a);
    deliver(&a, &mut b);
    deliver(&b, &mut a);

    assert_eq!(snapshot(&a), snapshot(&b), "the two devices disagree");

    // And the specifics, so a convergent-but-empty result cannot pass.
    let merged = repository::get_contact(&a, &shared.contact.id).unwrap().unwrap();
    assert_eq!(merged.contact.city.as_deref(), Some("ירושלים"));
    assert_eq!(merged.contact.profession.as_deref(), Some("שוחט"));
    assert_eq!(merged.contact_notes.len(), 2, "one of the two notes was lost");
    assert!(repository::get_contact(&b, &from_a.contact.id).unwrap().is_some());
    assert!(repository::get_contact(&a, &from_b.contact.id).unwrap().is_some());
}

#[test]
fn a_create_landing_on_an_edited_contact_does_not_flatten_it() {
    // A create carries the whole record and no `previous`, so there is nothing
    // to compare against — which makes it the one payload that could overwrite
    // local work wholesale. Normally the mutation id stops a second delivery
    // dead, but a log restored from a backup or replayed through a rebuilt
    // server can present the same contact under a fresh id. When that happens
    // the local edit must survive: an unfilled field may be filled, a filled
    // one is a disagreement.
    let (mut a, mut b) = pair();
    let mut input = contact("ישראל מאיר");
    input.city = Some("ירושלים".into());
    let created = repository::create_contact(&mut a, &input, None).unwrap();
    let id = created.contact.id.clone();
    deliver(&a, &mut b);

    // B corrects the city and adds something the create knows nothing about.
    repository::update_contact(
        &mut b,
        &id,
        &ContactPatch {
            city: Some(Some("בני ברק".into())),
            profession: Some(Some("שוחט".into())),
            ..Default::default()
        },
        None,
    )
    .unwrap();

    let mut replay = apply::pending(&a, 500)
        .unwrap()
        .into_iter()
        .find(|mutation| mutation.operation == "create")
        .unwrap();
    replay.id = format!("{}X", &replay.id[..replay.id.len() - 1]);

    let outcome = apply::apply(&mut b, &replay).unwrap();

    let after = repository::get_contact(&b, &id).unwrap().unwrap().contact;
    assert_eq!(after.city.as_deref(), Some("בני ברק"), "the local correction was overwritten");
    assert_eq!(
        after.profession.as_deref(),
        Some("שוחט"),
        "a field the create did not know was lost"
    );
    assert!(
        matches!(outcome, Applied::Conflicted(ref fields) if fields.iter().any(|f| f == "city")),
        "the disagreement was not reported: {outcome:?}"
    );
}
