//! Command line and startup for the sync server. See `lib.rs`.
//!
//! ```text
//! yanuka-server                 # serve (default)
//! yanuka-server pair-code       # mint a pairing code for a new device
//! yanuka-server devices         # list paired devices
//! yanuka-server revoke <id>     # cut a device off
//! yanuka-server backup <file>   # consistent copy of the database
//! ```

use yanuka_db::rusqlite::Connection;
use yanuka_db::sync::devices;
use yanuka_server::{open_database, router, AppState, Settings, VERSION};

fn print_pair_code(connection: &Connection) -> Result<(), String> {
    let (code, expires_at) = devices::create_pair_code(connection).map_err(|e| e.to_string())?;
    println!();
    println!("  קוד צימוד למכשיר חדש:   {code}");
    println!("  תקף עד:                 {expires_at}");
    println!("  בהגדרות ← סנכרון בין מכשירים ← הזינו את כתובת השרת ואת הקוד.");
    println!();
    Ok(())
}

async fn serve(settings: Settings) -> Result<(), String> {
    let connection = open_database(&settings)?;
    let active = devices::active_count(&connection).map_err(|e| e.to_string())?;
    eprintln!(
        "אוצר שלמה server {VERSION} — {} — data in {}",
        settings.name,
        settings.data_dir.display()
    );
    eprintln!("{active} paired device(s); listening on http://{}", settings.bind);
    if active == 0 {
        // Nobody can talk to a server nobody is paired with.
        print_pair_code(&connection)?;
    }

    let state = AppState::new(connection, &settings.name);
    let listener = tokio::net::TcpListener::bind(settings.bind)
        .await
        .map_err(|error| format!("bind {}: {error}", settings.bind))?;
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            eprintln!("shutting down");
        })
        .await
        .map_err(|error| error.to_string())
}

fn main() {
    let settings = match Settings::from_env() {
        Ok(settings) => settings,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let outcome = match args.first().map(String::as_str) {
        None | Some("serve") => tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|error| error.to_string())
            .and_then(|runtime| runtime.block_on(serve(settings))),
        Some("pair-code") => {
            open_database(&settings).and_then(|connection| print_pair_code(&connection))
        }
        Some("devices") => open_database(&settings).and_then(|connection| {
            let list = devices::list(&connection).map_err(|e| e.to_string())?;
            if list.is_empty() {
                println!("אין מכשירים מצומדים.");
            }
            for device in list {
                println!(
                    "{}  {}  ({}{})  נראה לאחרונה: {}",
                    device.id,
                    device.name,
                    device.kind,
                    if device.revoked_at.is_some() { ", מנותק" } else { "" },
                    device.last_seen_at.unwrap_or_else(|| "—".into())
                );
            }
            Ok(())
        }),
        Some("revoke") => match args.get(1) {
            Some(id) => open_database(&settings)
                .and_then(|connection| devices::revoke(&connection, id).map_err(|e| e.to_string()))
                .map(|_| println!("המכשיר {id} נותק.")),
            None => Err("usage: yanuka-server revoke <device-id>".into()),
        },
        Some("backup") => match args.get(1) {
            Some(target) => open_database(&settings).and_then(|connection| {
                let pragma = settings
                    .key
                    .as_deref()
                    .map(yanuka_db::encryption::raw_key_pragma)
                    .transpose()
                    .map_err(|e| e.to_string())?;
                yanuka_db::backup::backup_to(
                    &connection,
                    std::path::Path::new(target),
                    pragma.as_deref(),
                )
                .map_err(|e| e.to_string())?;
                println!("הגיבוי נשמר ב־{target}");
                Ok(())
            }),
            None => Err("usage: yanuka-server backup <file>".into()),
        },
        Some(other) => Err(format!(
            "unknown command {other:?}; commands: serve, pair-code, devices, revoke, backup"
        )),
    };
    if let Err(error) = outcome {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
