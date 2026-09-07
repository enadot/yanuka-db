//! The server on a real socket, driven by the desktop's own HTTP transport:
//! pairing, the first archive upload, a second device receiving it, and a
//! revoked device being turned away.

use std::cell::RefCell;

use serde_json::{json, Value};
use yanuka_db::models::{ContactInput, PhoneInput};
use yanuka_db::sync::devices::{self, DeviceInfo};
use yanuka_db::sync::http::{self, HttpTransport};
use yanuka_db::sync::Transport;
use yanuka_db::{migrate, open_in_memory, repository, DbError};
use yanuka_server::{router, AppState};

fn client() -> RefCell<yanuka_db::rusqlite::Connection> {
    let mut connection = open_in_memory().unwrap();
    migrate(&mut connection).unwrap();
    RefCell::new(connection)
}

fn info(client: &RefCell<yanuka_db::rusqlite::Connection>, name: &str) -> DeviceInfo {
    DeviceInfo {
        id: repository::device_id(&client.borrow()).unwrap(),
        name: name.into(),
        kind: "desktop".into(),
        platform: Some("test".into()),
        app_version: Some(yanuka_server::VERSION.into()),
    }
}

#[tokio::test]
async fn two_devices_pair_and_sync_through_the_server() {
    let mut hub = open_in_memory().unwrap();
    migrate(&mut hub).unwrap();
    let (first_code, _) = devices::create_pair_code(&hub).unwrap();
    let state = AppState::new(hub, "שרת הבדיקה");
    let db = state.db.clone();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router(state)).await.unwrap();
    });
    let url = format!("http://{address}");

    tokio::task::spawn_blocking(move || {
        // Anyone can ask who the server is.
        let health = http::health(&url).unwrap();
        assert_eq!(health["name"], json!("שרת הבדיקה"));
        assert_eq!(health["status"], json!("ok"));

        // Nobody can sync without a token.
        let stranger = HttpTransport::new(&url, "nope").unwrap();
        assert!(matches!(stranger.pull(0, 10), Err(DbError::Unauthorized)));

        // Device A pairs with the code the server printed at first start.
        let a = client();
        let paired_a = http::pair(&url, &first_code, &info(&a, "המחשב המרכזי")).unwrap();
        assert_eq!(paired_a.server_name.as_deref(), Some("שרת הבדיקה"));
        assert!(matches!(
            http::pair(&url, &first_code, &info(&a, "שוב")),
            Err(DbError::Unauthorized)
        ));

        // A paired device mints the code for the next one.
        let (second_code, _) = http::pair_code(&url, &paired_a.token).unwrap();
        let b = client();
        let paired_b = http::pair(&url, &second_code, &info(&b, "המחשב בבית")).unwrap();

        // A's archive goes up and comes down on B.
        let id = repository::create_contact(
            &mut a.borrow_mut(),
            &ContactInput {
                display_name: "הרב יעקב לוי".into(),
                phones: vec![PhoneInput { raw: "050-1234567".into(), ..Default::default() }],
                ..Default::default()
            },
            None,
        )
        .unwrap()
        .contact
        .id;
        let wire_a = HttpTransport::new(&url, &paired_a.token).unwrap();
        let report = yanuka_db::sync::run_cycle(&a, &wire_a, None).unwrap();
        assert_eq!(report.pushed, 1);

        let wire_b = HttpTransport::new(&url, &paired_b.token).unwrap();
        let report = yanuka_db::sync::run_cycle(&b, &wire_b, None).unwrap();
        assert_eq!(report.applied, 1);
        let on_b = repository::get_contact(&b.borrow(), &id).unwrap().unwrap();
        assert_eq!(on_b.contact.display_name, "הרב יעקב לוי");
        assert_eq!(on_b.phones[0].raw, "050-1234567");

        // The devices endpoint lists both; revoking B turns it away.
        let agent = ureq::Agent::new_with_config(
            ureq::Agent::config_builder().http_status_as_error(false).build(),
        );
        let bearer = format!("Bearer {}", paired_a.token);
        let list: Value = agent
            .get(format!("{url}/api/devices"))
            .header("Authorization", &bearer)
            .call()
            .unwrap()
            .body_mut()
            .read_json()
            .unwrap();
        assert_eq!(list.as_array().unwrap().len(), 2);
        let b_id = repository::device_id(&b.borrow()).unwrap();
        let response = agent
            .post(format!("{url}/api/devices/{b_id}/revoke"))
            .header("Authorization", &bearer)
            .send_empty()
            .unwrap();
        assert_eq!(response.status(), 200);
        assert!(matches!(
            yanuka_db::sync::run_cycle(&b, &wire_b, None),
            Err(DbError::Unauthorized)
        ));
        // A device cannot cut itself off.
        let a_id = repository::device_id(&a.borrow()).unwrap();
        let response = agent
            .post(format!("{url}/api/devices/{a_id}/revoke"))
            .header("Authorization", &bearer)
            .send_empty()
            .unwrap();
        assert_eq!(response.status(), 400);

        let status: Value = agent
            .get(format!("{url}/api/status"))
            .header("Authorization", &bearer)
            .call()
            .unwrap()
            .body_mut()
            .read_json()
            .unwrap();
        assert_eq!(status["contacts"], json!(1));
        assert_eq!(status["devices"], json!(1));

        // The server's own copy is a searchable archive.
        let guard = db.lock().unwrap();
        let hits = yanuka_db::search::search(
            &guard,
            &yanuka_db::models::SearchQuery { text: "לוי".into(), ..Default::default() },
        )
        .unwrap();
        assert_eq!(hits.results.len(), 1);
    })
    .await
    .unwrap();
}
