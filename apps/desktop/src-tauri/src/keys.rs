//! Where the database key lives.
//!
//! A random 256-bit key in the OS credential store (Windows Credential
//! Manager), not a passphrase. The reasoning is priority 1: a passphrase the
//! user forgets turns encryption into total data loss — database and every
//! backup at once — while a random key costs nothing to remember and is shown
//! in settings as a recovery key to be kept off the machine. See ADR-033.
//!
//! Only the Windows backend is compiled in. On Linux/macOS development builds
//! the store reports itself unavailable and the shell runs unencrypted — the
//! shipped product is Windows-only.

use keyring::Entry;

/// Credential identity in the OS store. Stable across versions: renaming
/// either string would orphan every existing installation's key.
const SERVICE: &str = "digital.baram.yanuka";
const USER: &str = "database-key";

pub struct DatabaseKey {
    /// 64 lowercase hex digits.
    pub hex: String,
    /// Whether the key is held by the OS store (as opposed to memory only).
    pub persisted: bool,
}

fn is_valid(hex: &str) -> bool {
    hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit())
}

fn generate() -> Option<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).ok()?;
    Some(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Fetch the key, creating and storing one on first run.
///
/// `None` means no usable credential store on this platform — the caller
/// opens the database unencrypted. A present-but-garbled entry is replaced:
/// a value that was never a valid key cannot be the key any database was
/// encrypted with, so nothing can be lost by overwriting it.
pub fn load_or_create() -> Option<DatabaseKey> {
    let entry = Entry::new(SERVICE, USER).ok()?;
    match entry.get_password() {
        Ok(stored) if is_valid(&stored) => {
            Some(DatabaseKey { hex: stored.to_ascii_lowercase(), persisted: true })
        }
        Ok(_) | Err(keyring::Error::NoEntry) => {
            let hex = generate()?;
            let persisted = entry.set_password(&hex).is_ok();
            Some(DatabaseKey { hex, persisted })
        }
        Err(_) => None,
    }
}

/// Store a key the user typed into the recovery screen, so the next launch
/// unlocks by itself. Best effort — a failure leaves the key memory-only for
/// this run, which still opens the data.
pub fn persist(hex: &str) -> bool {
    Entry::new(SERVICE, USER).and_then(|entry| entry.set_password(hex)).is_ok()
}

/// The display form of the recovery key: uppercase, dashed groups of eight.
/// `encryption::raw_key_pragma` accepts exactly this back, separators and all.
pub fn format_for_display(hex: &str) -> String {
    hex.to_ascii_uppercase()
        .as_bytes()
        .chunks(8)
        .map(|chunk| std::str::from_utf8(chunk).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("-")
}
