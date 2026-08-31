use serde::Serialize;

/// Failures the storage layer can produce.
///
/// The variants line up one-for-one with `RepositoryErrorCode` in
/// `packages/core/src/errors.ts`, because they cross the IPC boundary and the
/// UI switches on them to decide what to tell the user. `StaleVersion` in
/// particular is not a user error — it means another device got there first,
/// and the interface must offer a choice rather than a failure.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("{0} לא נמצא")]
    NotFound(String),

    #[error("{0}")]
    Validation(String),

    #[error("הרשומה עודכנה במקום אחר")]
    StaleVersion { expected: i64, actual: i64 },

    #[error("רשומה זהה כבר קיימת")]
    Duplicate,

    /// The bundled SQLite was built without a capability the schema requires.
    /// Raised at startup rather than at the first search, so a broken build is
    /// obvious immediately.
    #[error("מסד הנתונים נבנה ללא תמיכה ב-{0}")]
    MissingCapability(&'static str),

    #[error("שגיאת מסד נתונים: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("שגיאת נתונים: {0}")]
    Serde(#[from] serde_json::Error),

    /// A shipped migration was edited after it had already been applied.
    #[error("מיגרציה {0} שונתה לאחר שהוחלה — אין לערוך מיגרציות שכבר פורסמו")]
    MigrationChanged(String),

    /// The database file is encrypted and no valid key has been provided yet.
    /// The desktop shell answers every command with this until the user
    /// supplies the recovery key. See docs/SECURITY.md.
    #[error("המאגר מוצפן ונעול — נדרש מפתח שחזור")]
    Locked,

    /// The embedding model failed to load or run. Never shown to the user in
    /// the search flow — semantic search degrades to the lexical layers — but
    /// the settings screen surfaces the state. See ADR-036.
    #[error("שגיאת מנוע סמנטי: {0}")]
    Semantic(String),
}

/// Wire form of an error, matching what `toRepositoryError` expects on the
/// TypeScript side.
#[derive(Debug, Serialize)]
pub struct SerializedError {
    pub code: &'static str,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

impl DbError {
    pub fn code(&self) -> &'static str {
        match self {
            DbError::NotFound(_) => "not_found",
            DbError::Validation(_) => "validation",
            DbError::StaleVersion { .. } => "stale_version",
            DbError::Duplicate => "duplicate",
            DbError::MissingCapability(_)
            | DbError::MigrationChanged(_)
            | DbError::Locked
            | DbError::Semantic(_) => "unavailable",
            DbError::Sqlite(_) | DbError::Serde(_) => "database",
        }
    }

    fn serialize_wire(&self) -> SerializedError {
        let details = match self {
            DbError::StaleVersion { expected, actual } => Some(serde_json::json!({
                "expected": expected,
                "actual": actual,
            })),
            _ => None,
        };

        SerializedError { code: self.code(), message: self.to_string(), details }
    }
}

/// Serialized directly so a command can `return Err(db_error)` and the frontend
/// receives `{ code, message, details }`, which is exactly what
/// `toRepositoryError` in packages/core expects.
impl Serialize for DbError {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        self.serialize_wire().serialize(serializer)
    }
}

pub type Result<T> = std::result::Result<T, DbError>;
