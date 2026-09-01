//! What happens after two devices disagree.
//!
//! `sync.rs` proves that a collision is preserved rather than silently
//! flattened. That is only half of the promise, and on its own it is a worse
//! product than the one it replaced: an archive that accumulates unanswered
//! questions is an archive nobody trusts. These tests are the other half — a
//! decision has to be reachable, and once made it has to end the disagreement
//! on *both* machines rather than only on the one where it was made.
//!
//! The trap this file exists to guard is specific. Choosing the value that is
//! already here changes nothing locally, so the ordinary "log what changed"
//! path records nothing, and the other device would keep its own answer
//! forever — two machines quietly disagreeing, each believing it is settled,
//! with no open conflict anywhere to show it.

use yanuka_db::apply::{self, Applied};
use yanuka_db::conflicts::{self, FieldChoice, Side};
use yanuka_db::models::{ContactInput, ContactPatch};
use yanuka_db::{migrate, open_in_memory, repository};

type Db = yanuka_db::rusqlite::Connection;

fn db() -> Db {
    let mut connection = open_in_memory().expect("open");
    migrate(&mut connection).expect("migrate");
    connection
}

fn pair() -> (Db, Db) {
    let a = db();
    let b = db();
    repository::device_id(&a).unwrap();
    repository::device_id(&b).unwrap();
    (a, b)
}

fn deliver(from: &Db, to: &mut Db) -> Vec<Applied> {
    let queue = apply::pending(from, 500).unwrap();
    queue.iter().map(|mutation| apply::apply(to, mutation).unwrap()).collect()
}

fn city(connection: &Db, id: &str) -> Option<String> {
    repository::get_contact(connection, id).unwrap().unwrap().contact.city
}

fn profession(connection: &Db, id: &str) -> Option<String> {
    repository::get_contact(connection, id).unwrap().unwrap().contact.profession
}

fn set(connection: &mut Db, id: &str, patch: ContactPatch) {
    repository::update_contact(connection, id, &patch, None).unwrap();
}

fn city_patch(value: &str) -> ContactPatch {
    ContactPatch { city: Some(Some(value.to_string())), ..Default::default() }
}

/// Two devices that both hold the contact, then disagree about the city.
///
/// Returns the contact id. Both sides are left with exactly one open conflict,
/// which is the state a person would actually be looking at.
fn disagree_about_the_city(a: &mut Db, b: &mut Db) -> String {
    let input = ContactInput { display_name: "יעקב פרידמן".into(), ..Default::default() };
    let id = repository::create_contact(a, &input, None).unwrap().contact.id;
    deliver(a, b);

    set(a, &id, city_patch("אנטוורפן"));
    set(b, &id, city_patch("לונדון"));

    deliver(a, b);
    deliver(b, a);

    assert_eq!(conflicts::open(a).unwrap().len(), 1, "A should be holding a question");
    assert_eq!(conflicts::open(b).unwrap().len(), 1, "B should be holding the same question");
    id
}

fn choose(connection: &mut Db, field: &str, side: Side) {
    let open = conflicts::open(connection).unwrap();
    let conflict = open.first().expect("an open conflict");
    conflicts::resolve(connection, &conflict.id, &[FieldChoice { field: field.to_string(), side }])
        .unwrap();
}

#[test]
fn an_open_conflict_carries_both_answers_and_says_whose_they_are() {
    // The screen cannot ask a useful question from a field name alone. It needs
    // both values, when each was written, and which machine wrote it — that
    // last part being how a user who remembers typing something at home tells
    // their own answer from the one that arrived.
    let (mut a, mut b) = pair();
    let id = disagree_about_the_city(&mut a, &mut b);

    let open = conflicts::open(&a).unwrap();
    let conflict = &open[0];
    assert_eq!(conflict.entity_type, "contact");
    assert_eq!(conflict.entity_id, id);
    assert_eq!(conflict.display_name.as_deref(), Some("יעקב פרידמן"));

    let field = &conflict.fields[0];
    assert_eq!(field.field, "city");
    assert_eq!(field.local_value, "אנטוורפן");
    assert_eq!(field.remote_value, "לונדון");
    assert!(field.local_device_id.is_some(), "the local value must name its device");
    assert!(field.remote_device_id.is_some(), "the arriving value must name its device");
    assert_ne!(field.local_device_id, field.remote_device_id);
    assert!(!field.local_updated_at.is_empty());
    assert!(!field.remote_updated_at.is_empty());
}

#[test]
fn keeping_the_local_answer_reaches_the_other_device() {
    // The one that would break without a mutation of its own. Nothing changes
    // on A — "אנטוורפן" was already there — so if resolving only wrote a
    // resolved_at, B would keep "לונדון" indefinitely and neither machine
    // would have anything open to show for it.
    let (mut a, mut b) = pair();
    let id = disagree_about_the_city(&mut a, &mut b);

    choose(&mut a, "city", Side::Local);

    assert_eq!(city(&a, &id).as_deref(), Some("אנטוורפן"));
    assert!(conflicts::open(&a).unwrap().is_empty(), "the decision closed nothing on A");

    let outcomes = deliver(&a, &mut b);
    assert!(
        !outcomes.iter().any(|outcome| matches!(outcome, Applied::Conflicted(_))),
        "a resolution must settle the disagreement, not restate it"
    );
    assert_eq!(city(&b, &id).as_deref(), Some("אנטוורפן"), "B never heard the decision");
    assert!(conflicts::open(&b).unwrap().is_empty(), "B is still asking a settled question");
}

#[test]
fn taking_the_other_answer_settles_both_sides_too() {
    let (mut a, mut b) = pair();
    let id = disagree_about_the_city(&mut a, &mut b);

    choose(&mut a, "city", Side::Remote);

    assert_eq!(city(&a, &id).as_deref(), Some("לונדון"));
    assert!(conflicts::open(&a).unwrap().is_empty());

    deliver(&a, &mut b);
    assert_eq!(city(&b, &id).as_deref(), Some("לונדון"));
    assert!(conflicts::open(&b).unwrap().is_empty(), "B agreed all along and must stop asking");
}

#[test]
fn a_decision_made_elsewhere_closes_the_question_here() {
    // The user resolves on the laptop. The desktop must not greet them with the
    // same choice next time it syncs — every question that turns out not to be
    // one costs attention the next real question needs.
    let (mut a, mut b) = pair();
    let id = disagree_about_the_city(&mut a, &mut b);

    choose(&mut b, "city", Side::Local);
    deliver(&b, &mut a);

    assert_eq!(city(&a, &id).as_deref(), Some("לונדון"));
    assert!(conflicts::open(&a).unwrap().is_empty());
    assert_eq!(conflicts::open_count(&a).unwrap(), 0);
}

#[test]
fn the_resolved_value_is_findable() {
    // A contact whose city was decided but never reindexed would be missing
    // from exactly the search that a person resolving a conflict is most likely
    // to run next.
    let (mut a, mut b) = pair();
    let id = disagree_about_the_city(&mut a, &mut b);

    choose(&mut a, "city", Side::Remote);

    let query = yanuka_db::models::SearchQuery {
        text: "לונדון".into(),
        limit: Some(10),
        ..Default::default()
    };
    let hits = yanuka_db::search::search(&a, &query).unwrap();
    assert!(
        hits.results.iter().any(|hit| hit.contact.id == id),
        "the decided value is not searchable"
    );
}

#[test]
fn deciding_one_field_leaves_the_other_open() {
    // Some disagreements are obvious and some need a phone call. The obvious
    // one should not have to wait for the other.
    let (mut a, mut b) = pair();
    let input = ContactInput { display_name: "רבקה שטרן".into(), ..Default::default() };
    let id = repository::create_contact(&mut a, &input, None).unwrap().contact.id;
    deliver(&a, &mut b);

    set(
        &mut a,
        &id,
        ContactPatch {
            city: Some(Some("חיפה".into())),
            profession: Some(Some("מורה".into())),
            ..Default::default()
        },
    );
    set(
        &mut b,
        &id,
        ContactPatch {
            city: Some(Some("צפת".into())),
            profession: Some(Some("גננת".into())),
            ..Default::default()
        },
    );
    deliver(&b, &mut a);

    let open = conflicts::open(&a).unwrap();
    assert_eq!(open[0].fields.len(), 2);

    conflicts::resolve(
        &mut a,
        &open[0].id,
        &[FieldChoice { field: "city".into(), side: Side::Remote }],
    )
    .unwrap();

    assert_eq!(city(&a, &id).as_deref(), Some("צפת"), "the decided field did not move");
    assert_eq!(profession(&a, &id).as_deref(), Some("מורה"), "the undecided field must not move");

    let still_open = conflicts::open(&a).unwrap();
    assert_eq!(still_open.len(), 1, "the record closed with a question still in it");
    assert_eq!(still_open[0].fields.len(), 1);
    assert_eq!(still_open[0].fields[0].field, "profession");
}

#[test]
fn resolving_something_already_settled_is_not_an_error() {
    // Two windows open, or a sync that landed between the screen being drawn
    // and the button being pressed. The outcome the user wanted already holds,
    // so refusing would be a message about bookkeeping, not about their data.
    let (mut a, mut b) = pair();
    let id = disagree_about_the_city(&mut a, &mut b);
    let conflict_id = conflicts::open(&a).unwrap()[0].id.clone();

    conflicts::resolve(
        &mut a,
        &conflict_id,
        &[FieldChoice { field: "city".into(), side: Side::Local }],
    )
    .unwrap();
    conflicts::resolve(
        &mut a,
        &conflict_id,
        &[FieldChoice { field: "city".into(), side: Side::Remote }],
    )
    .expect("a second decision on a closed conflict must not fail");

    assert_eq!(city(&a, &id).as_deref(), Some("אנטוורפן"), "the closed decision was overwritten");
}

#[test]
fn a_decision_survives_a_restart() {
    // The resolution is a row, not a flag in memory. Worth pinning because the
    // whole point of the mutation is that it is durable before it is sent.
    let (mut a, mut b) = pair();
    let id = disagree_about_the_city(&mut a, &mut b);
    choose(&mut a, "city", Side::Local);

    let pending = apply::pending(&a, 500).unwrap();
    assert!(
        pending.iter().any(|mutation| {
            mutation.entity_id == id
                && mutation
                    .payload
                    .as_ref()
                    .and_then(|payload| payload.get("city"))
                    .is_some_and(|value| value == "אנטוורפן")
        }),
        "the decision is not queued for the other device"
    );
}
