//! Two devices, one server, nothing simulated.
//!
//! Real SQLite databases, a real axum server on a real TCP port, real
//! PostgreSQL behind it, real HTTP over the loopback, real XChaCha20 sealing.
//! The earlier suites each proved one layer; this proves they compose, which is
//! a different claim and the one the user actually cares about.
//!
//! Requires `DATABASE_URL`. Skipped loudly without it — a test that quietly
//! passes because it could not run is worse than no test.

use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;
use yanuka_db::models::{ContactInput, ContactPatch, PhoneInput};
use yanuka_db::{migrate, open_in_memory, repository, taxonomy};
use yanuka_sync_client::{connect, sync_once, Database, OwnedDatabase, SyncSettings};
use yanuka_sync_proto::{ConnectionCode, SyncKey};

const SECRET: &str = "an-end-to-end-enrolment-secret-value";

/// A server listening on a real port, and the code that joins it.
struct Fixture {
    url: String,
    code: String,
}

async fn start(schema: &str) -> Option<Fixture> {
    let url = std::env::var("DATABASE_URL").ok()?;

    let admin = PgPoolOptions::new().max_connections(2).connect(&url).await.expect("connect");
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP SCHEMA IF EXISTS {schema} CASCADE; CREATE SCHEMA {schema};"
    )))
    .execute(&admin)
    .await
    .expect("schema");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .after_connect({
            let schema = schema.to_string();
            move |connection, _| {
                let schema = schema.clone();
                Box::pin(async move {
                    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("SET search_path TO {schema}")))
                        .execute(&mut *connection)
                        .await?;
                    Ok(())
                })
            }
        })
        .connect(&url)
        .await
        .expect("pool");

    yanuka_sync_server::migrate(&pool).await.expect("migrate");

    let state =
        Arc::new(yanuka_sync_server::AppState { pool, enrolment_secret: SECRET.to_string() });
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
    let address = listener.local_addr().expect("addr");

    tokio::spawn(async move {
        let _ = axum::serve(listener, yanuka_sync_server::router(state)).await;
    });

    let server_url = format!("http://{address}");
    let code = ConnectionCode {
        server_url: server_url.clone(),
        enrolment_secret: SECRET.to_string(),
        key: *SyncKey::generate().as_bytes(),
    }
    .encode();

    Some(Fixture { url: server_url, code })
}

macro_rules! require_server {
    ($schema:expr) => {
        match start($schema).await {
            Some(fixture) => fixture,
            None => {
                eprintln!(
                    "SKIPPED {}: set DATABASE_URL to a PostgreSQL the tests may use",
                    $schema
                );
                return;
            }
        }
    };
}

/// A device, held the way the desktop holds it: behind the same handle the
/// sync loop takes and releases around every network call.
fn device() -> OwnedDatabase {
    let mut connection = open_in_memory().expect("open");
    migrate(&mut connection).expect("migrate");
    repository::device_id(&connection).expect("device id");
    OwnedDatabase::new(connection)
}

fn contact(display_name: &str) -> ContactInput {
    ContactInput { display_name: display_name.to_string(), ..Default::default() }
}

async fn join(db: &OwnedDatabase, code: &str, name: &str) -> SyncSettings {
    connect(db, code, name, "desktop").await.expect("connect")
}

// Thin wrappers so each test reads as what the user did, rather than as lock
// bookkeeping. Every one of them takes the database exactly as the sync loop
// does — briefly, and never across a network call.

fn create(db: &OwnedDatabase, input: &ContactInput) -> String {
    db.with(|connection| repository::create_contact(connection, input, None)).unwrap().contact.id
}

fn note(db: &OwnedDatabase, contact_id: &str, body: &str) {
    db.with(|connection| taxonomy::add_note(connection, contact_id, body, false)).unwrap();
}

fn edit(db: &OwnedDatabase, contact_id: &str, patch: ContactPatch) {
    db.with(|connection| repository::update_contact(connection, contact_id, &patch, None)).unwrap();
}

fn fetch(db: &OwnedDatabase, contact_id: &str) -> Option<yanuka_db::models::ContactWithRelations> {
    db.with(|connection| repository::get_contact(connection, contact_id)).unwrap()
}

fn finds(db: &OwnedDatabase, text: &str, contact_id: &str) -> bool {
    db.with(|connection| {
        yanuka_db::search::search(
            connection,
            &yanuka_db::models::SearchQuery { text: text.into(), ..Default::default() },
        )
    })
    .unwrap()
    .results
    .iter()
    .any(|hit| hit.contact.id == contact_id)
}

fn contacts(db: &OwnedDatabase) -> usize {
    db.with(|connection| repository::list_contacts(connection, None, 50, None)).unwrap().items.len()
}

#[tokio::test]
async fn a_contact_typed_on_one_machine_appears_on_the_other() {
    // The single sentence the user would use to describe why any of this
    // exists, executed literally.
    let fixture = require_server!("e2e_basic");

    let first = device();
    let second = device();
    let mut first_settings = join(&first, &fixture.code, "מחשב ראשי").await;
    let mut second_settings = join(&second, &fixture.code, "מחשב שני").await;

    let mut input = contact("שרה כהן");
    input.city = Some("ירושלים".into());
    input.phones = vec![PhoneInput { raw: "052-1234567".into(), ..Default::default() }];
    let created = create(&first, &input);
    note(&first, &created, "הכרנו דרך הרב");

    let sent = sync_once(&first, &mut first_settings).await.unwrap();
    assert!(sent.pushed >= 2, "nothing left the first machine: {sent:?}");

    let received = sync_once(&second, &mut second_settings).await.unwrap();
    assert!(received.applied >= 2, "nothing arrived on the second machine: {received:?}");

    let arrived = fetch(&second, &created).expect("contact");
    assert_eq!(arrived.contact.display_name, "שרה כהן");
    assert_eq!(arrived.contact.city.as_deref(), Some("ירושלים"));
    assert_eq!(arrived.phones[0].raw, "052-1234567");
    assert_eq!(arrived.contact_notes[0].body, "הכרנו דרך הרב");

    // And it is findable, not merely present. A contact that arrived without
    // being indexed is invisible to the only screen anyone uses, which would
    // look exactly like it never arrived.
    assert!(
        finds(&second, "שרה", &created),
        "the synced contact is not searchable on the receiving machine"
    );

    // Including by a word from the note, which only works if the note reached
    // the index too.
    assert!(finds(&second, "הרב", &created), "the synced note did not reach the search index");
}

#[tokio::test]
async fn work_done_offline_on_both_machines_survives_reconnection() {
    // The scenario this product is shaped around: the main machine has no
    // network for days. Both sides keep working. Neither loses anything.
    let fixture = require_server!("e2e_offline");

    let first = device();
    let second = device();
    let mut first_settings = join(&first, &fixture.code, "מחשב ראשי").await;
    let mut second_settings = join(&second, &fixture.code, "מחשב שני").await;

    let shared = create(&first, &contact("קשר משותף"));
    sync_once(&first, &mut first_settings).await.unwrap();
    sync_once(&second, &mut second_settings).await.unwrap();

    // Now both go quiet and keep working.
    edit(
        &first,
        &shared,
        ContactPatch { city: Some(Some("בני ברק".into())), ..Default::default() },
    );
    let only_on_first = create(&first, &contact("איש של א"));

    edit(
        &second,
        &shared,
        ContactPatch { profession: Some(Some("סופר סת\"ם".into())), ..Default::default() },
    );
    note(&second, &shared, "הערה מהמחשב השני");

    // Reconnect, both ways, twice — the second pass carries what the first
    // uploaded.
    for _ in 0..2 {
        sync_once(&first, &mut first_settings).await.unwrap();
        sync_once(&second, &mut second_settings).await.unwrap();
    }
    sync_once(&first, &mut first_settings).await.unwrap();

    for (name, db) in [("ראשי", &first), ("שני", &second)] {
        let merged = fetch(db, &shared).unwrap();
        assert_eq!(merged.contact.city.as_deref(), Some("בני ברק"), "city lost on {name}");
        assert_eq!(
            merged.contact.profession.as_deref(),
            Some("סופר סת\"ם"),
            "profession lost on {name}"
        );
        assert_eq!(merged.contact_notes.len(), 1, "the note is missing on {name}");
    }
    assert!(
        fetch(&second, &only_on_first).is_some(),
        "a contact created offline never reached the other machine"
    );
}

#[tokio::test]
async fn syncing_twice_with_nothing_new_does_nothing() {
    // The common case, run every few minutes forever. It must be cheap and it
    // must not churn the database.
    let fixture = require_server!("e2e_idle");

    let first = device();
    let mut settings = join(&first, &fixture.code, "מחשב").await;
    create(&first, &contact("מישהו"));

    let first_run = sync_once(&first, &mut settings).await.unwrap();
    assert!(first_run.pushed > 0);

    let second_run = sync_once(&first, &mut settings).await.unwrap();
    assert_eq!(second_run.pushed, 0, "an already-settled change was sent again");
    assert_eq!(second_run.applied, 0, "a change this device made was applied back onto it");
}

#[tokio::test]
async fn a_replacement_machine_rebuilds_the_whole_archive() {
    // A laptop dies. Its replacement is enrolled fresh and must end up holding
    // the archive — which is what makes the sealed log an off-site backup as
    // well as a courier.
    let fixture = require_server!("e2e_rebuild");

    let original = device();
    let mut settings = join(&original, &fixture.code, "מחשב ישן").await;

    for name in ["ראשון", "שני", "שלישי"] {
        let created = create(&original, &contact(name));
        note(&original, &created, &format!("הערה על {name}"));
    }
    sync_once(&original, &mut settings).await.unwrap();

    // The replacement shares nothing with the old machine but the code.
    let replacement = device();
    let mut fresh = join(&replacement, &fixture.code, "מחשב חדש").await;
    let outcome = sync_once(&replacement, &mut fresh).await.unwrap();
    assert!(outcome.applied >= 6, "the rebuild did not receive the archive: {outcome:?}");

    assert_eq!(contacts(&replacement), 3, "the rebuilt machine is missing contacts");
}

#[tokio::test]
async fn a_wrong_connection_code_is_refused_rather_than_half_applied() {
    // Pasted by hand, so a wrong or stale code is a normal mistake. It must
    // fail at the door — a device that half-joins and then cannot decrypt would
    // look to the user like the archive itself was broken.
    let fixture = require_server!("e2e_bad_code");

    let db = device();
    let wrong = ConnectionCode {
        server_url: fixture.url.clone(),
        enrolment_secret: "not-the-secret".into(),
        key: *SyncKey::generate().as_bytes(),
    }
    .encode();

    assert!(connect(&db, &wrong, "מתחזה", "desktop").await.is_err());
    assert!(connect(&db, "yanuka1_garbage", "מתחזה", "desktop").await.is_err());
    assert!(
        db.with(|connection| yanuka_sync_client::load(connection)).unwrap().is_none(),
        "a refused connection left settings behind"
    );
}

#[tokio::test]
async fn a_device_with_the_wrong_key_reports_it_instead_of_writing_rubbish() {
    // Both devices enrol successfully — the server accepts them, because the
    // server does not know the data key. Only when a payload arrives does the
    // mismatch surface, and it has to surface as an error rather than as a
    // corrupted contact.
    let fixture = require_server!("e2e_wrong_key");

    let first = device();
    let mut first_settings = join(&first, &fixture.code, "מחשב ראשי").await;
    create(&first, &contact("סוד"));
    sync_once(&first, &mut first_settings).await.unwrap();

    let second = device();
    let mismatched = ConnectionCode {
        server_url: fixture.url.clone(),
        enrolment_secret: SECRET.to_string(),
        key: *SyncKey::generate().as_bytes(),
    }
    .encode();
    let mut second_settings = join(&second, &mismatched, "מחשב עם סיסמה אחרת").await;

    let outcome = sync_once(&second, &mut second_settings).await;
    assert!(outcome.is_err(), "a payload sealed under another key was accepted");

    assert_eq!(contacts(&second), 0, "something was written despite the key mismatch");
}

#[tokio::test]
async fn the_same_field_changed_on_both_machines_becomes_a_conflict_not_a_loss() {
    // End to end, through the server: the case where no rule can be right.
    let fixture = require_server!("e2e_conflict");

    let first = device();
    let second = device();
    let mut first_settings = join(&first, &fixture.code, "מחשב ראשי").await;
    let mut second_settings = join(&second, &fixture.code, "מחשב שני").await;

    let shared = create(&first, &contact("יעקב פרידמן"));
    sync_once(&first, &mut first_settings).await.unwrap();
    sync_once(&second, &mut second_settings).await.unwrap();

    edit(
        &first,
        &shared,
        ContactPatch { city: Some(Some("אנטוורפן".into())), ..Default::default() },
    );
    edit(
        &second,
        &shared,
        ContactPatch { city: Some(Some("לונדון".into())), ..Default::default() },
    );

    sync_once(&first, &mut first_settings).await.unwrap();
    let outcome = sync_once(&second, &mut second_settings).await.unwrap();

    assert_eq!(outcome.conflicts, 1, "the collision was not reported: {outcome:?}");

    let fields: String = second
        .with(|connection| {
            connection.query_row(
                "SELECT fields FROM conflicts WHERE resolved_at IS NULL",
                [],
                |row| row.get::<_, String>(0),
            )
        })
        .unwrap();
    assert!(fields.contains("לונדון"), "the local value is not in the conflict record");
    assert!(fields.contains("אנטוורפן"), "the remote value was discarded");
}
