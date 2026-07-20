//! Typed views over the two AllDebrid payloads this plugin consumes.

use serde::Deserialize;

use crate::api::parse_envelope;
use crate::error::PluginError;

/// What `validate_account` reports back to the Accounts view (R-01).
///
/// AllDebrid has no account-wide byte quota — `limitedHostersQuotas` counts
/// downloads per hoster, not bytes — so only the expiry is surfaced.
#[derive(Debug, PartialEq, Eq)]
pub struct AccountStatus {
    /// Unix timestamp, in seconds, when premium lapses.
    pub valid_until: Option<u64>,
}

/// A hoster link AllDebrid has turned into a direct CDN URL (R-02).
#[derive(Debug, PartialEq, Eq)]
pub struct UnlockedLink {
    pub direct_url: String,
    pub filename: Option<String>,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct UserData {
    user: User,
}

#[derive(Debug, Deserialize)]
struct User {
    #[serde(rename = "isPremium", default)]
    is_premium: bool,
    #[serde(rename = "premiumUntil", default)]
    premium_until: u64,
}

#[derive(Debug, Deserialize)]
struct UnlockData {
    #[serde(default)]
    link: String,
    #[serde(default)]
    filename: Option<String>,
    #[serde(default)]
    filesize: Option<u64>,
    /// Non-zero when the link needs polling before it is servable.
    #[serde(default)]
    delayed: u64,
}

pub fn parse_user_response(body: &str) -> Result<AccountStatus, PluginError> {
    let data: UserData = deserialize(parse_envelope(body)?)?;
    if !data.user.is_premium {
        return Err(PluginError::AccountExpired(
            "AllDebrid reports this account is not premium".into(),
        ));
    }
    Ok(AccountStatus {
        valid_until: (data.user.premium_until > 0).then_some(data.user.premium_until),
    })
}

pub fn parse_unlock_response(body: &str) -> Result<UnlockedLink, PluginError> {
    let data: UnlockData = deserialize(parse_envelope(body)?)?;
    // A delayed link has no URL yet. Returning the entry anyway would be the
    // faux succès R-04 forbids, so it surfaces as a cooldown the host retries.
    if data.delayed > 0 {
        return Err(PluginError::RateLimited(format!(
            "AllDebrid is still preparing this link (delayed id {})",
            data.delayed
        )));
    }
    if data.link.trim().is_empty() {
        return Err(PluginError::InvalidApiResponse(
            "unlock succeeded without a direct link".into(),
        ));
    }
    Ok(UnlockedLink {
        direct_url: data.link,
        filename: data.filename.filter(|name| !name.trim().is_empty()),
        size_bytes: data.filesize.filter(|size| *size > 0),
    })
}

fn deserialize<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<T, PluginError> {
    serde_json::from_value(value)
        .map_err(|e| PluginError::InvalidApiResponse(format!("unexpected payload shape: {e}")))
}
