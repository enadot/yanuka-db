//! `Transport` over HTTP, and the two calls that precede a transport:
//! pairing and a health check. Errors come back in the user's language —
//! "the server cannot be reached" is the whole of what the indicator has to
//! say, and the raw cause rides along for the log.

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use ureq::Agent;

use super::devices::DeviceInfo;
use super::{PullPage, PushItem, PushResult, Transport};
use crate::error::{DbError, Result};

/// A first pairing may upload the whole archive in one push; the limit is
/// generous for a slow link, and the engine's chunks keep each call small.
const TIMEOUT: Duration = Duration::from_secs(60);

fn agent() -> Agent {
    let config =
        Agent::config_builder().timeout_global(Some(TIMEOUT)).http_status_as_error(false).build();
    Agent::new_with_config(config)
}

/// `https://host:port` — whatever the person typed, trimmed, without a
/// trailing slash, with a scheme.
pub fn normalize_url(url: &str) -> Result<String> {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(DbError::Validation("יש להזין את כתובת השרת".into()));
    }
    let with_scheme = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };
    Ok(with_scheme)
}

fn unreachable(error: ureq::Error) -> DbError {
    match error {
        ureq::Error::HostNotFound => DbError::Sync("הכתובת לא נמצאה — בדקו את שם השרת".into()),
        ureq::Error::ConnectionFailed
        | ureq::Error::Io(_)
        | ureq::Error::Timeout(_)
        | ureq::Error::Tls(_) => DbError::Sync(format!("השרת אינו זמין כרגע ({error})")),
        ureq::Error::BadUri(uri) => DbError::Validation(format!("כתובת שרת אינה תקינה: {uri}")),
        other => DbError::Sync(format!("תקלה בתקשורת עם השרת ({other})")),
    }
}

fn decode<T: DeserializeOwned>(
    outcome: std::result::Result<ureq::http::Response<ureq::Body>, ureq::Error>,
) -> Result<T> {
    let mut response = outcome.map_err(unreachable)?;
    let status = response.status();
    if status == ureq::http::StatusCode::UNAUTHORIZED {
        return Err(DbError::Unauthorized);
    }
    if !status.is_success() {
        let body: Value = response.body_mut().read_json().unwrap_or(Value::Null);
        let message = body
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("השרת השיב {status}"));
        return Err(match body.get("code").and_then(Value::as_str) {
            Some("validation") => DbError::Validation(message),
            Some("not_found") => DbError::NotFound(message),
            _ => DbError::Sync(message),
        });
    }
    response
        .body_mut()
        .read_json::<T>()
        .map_err(|error| DbError::Sync(format!("תשובת השרת אינה קריאה ({error})")))
}

/// Ask a server who it is. Works without a token, so the settings screen can
/// verify an address before pairing.
pub fn health(url: &str) -> Result<Value> {
    let base = normalize_url(url)?;
    decode(agent().get(format!("{base}/api/health")).call())
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Paired {
    pub token: String,
    pub server_name: Option<String>,
    pub device_id: String,
}

/// Redeem a pairing code for a token.
pub fn pair(url: &str, code: &str, device: &DeviceInfo) -> Result<Paired> {
    let base = normalize_url(url)?;
    decode(
        agent()
            .post(format!("{base}/api/pair"))
            .send_json(json!({ "code": code.trim(), "device": device })),
    )
}

/// Mint a pairing code for another device, from a paired one.
pub fn pair_code(url: &str, token: &str) -> Result<(String, String)> {
    let base = normalize_url(url)?;
    let answer: Value = decode(
        agent()
            .post(format!("{base}/api/pair-codes"))
            .header("Authorization", format!("Bearer {token}"))
            .send_empty(),
    )?;
    let code = answer.get("code").and_then(Value::as_str).unwrap_or_default().to_string();
    let expires = answer.get("expiresAt").and_then(Value::as_str).unwrap_or_default().to_string();
    Ok((code, expires))
}

pub struct HttpTransport {
    agent: Agent,
    base: String,
    token: String,
}

impl HttpTransport {
    pub fn new(url: &str, token: &str) -> Result<Self> {
        Ok(Self { agent: agent(), base: normalize_url(url)?, token: token.to_string() })
    }

    fn bearer(&self) -> String {
        format!("Bearer {}", self.token)
    }
}

impl Transport for HttpTransport {
    fn push(&self, items: &[PushItem]) -> Result<Vec<PushResult>> {
        let mut answer: Value = decode(
            self.agent
                .post(format!("{}/api/sync/push", self.base))
                .header("Authorization", self.bearer())
                .send_json(json!({ "items": items })),
        )?;
        Ok(serde_json::from_value(answer["results"].take())?)
    }

    fn pull(&self, after: i64, limit: usize) -> Result<PullPage> {
        decode(
            self.agent
                .get(format!("{}/api/sync/pull?after={after}&limit={limit}", self.base))
                .header("Authorization", self.bearer())
                .call(),
        )
    }
}
