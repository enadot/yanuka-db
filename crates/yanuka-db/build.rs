//! Rebuild when the shared SQL migrations change.
//!
//! `src/migrate.rs` pulls the schema in with `include_str!`, which cargo does
//! not track as a dependency across package boundaries. Without this, editing a
//! migration would leave a stale schema compiled into the binary.

fn main() {
    println!("cargo:rerun-if-changed=../../packages/database/migrations/sqlite");
}
