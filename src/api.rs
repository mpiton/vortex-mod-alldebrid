//! AllDebrid API v4 — requests, envelope, and error classification.
//!
//! AllDebrid answers HTTP 200 for almost every failure and puts the real
//! outcome in `status`, so the body is always parsed before anything is
//! treated as success.

use std::collections::HashMap;

use serde::Deserialize;

use crate::error::PluginError;
use crate::http::{percent_encode, truncate, HttpRequest, HttpResponse};

const API_BASE: &str = "https://api.alldebrid.com/v4";
/// AllDebrid rejects requests that do not name the calling software.
const AGENT: &str = "vortex";

#[derive(Debug, Deserialize)]
struct Envelope {
    status: String,
    #[serde(default)]
    data: Option<serde_json::Value>,
    #[serde(default)]
    error: Option<ApiError>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    code: String,
    #[serde(default)]
    message: String,
}

pub fn build_user_request(api_key: &str) -> Result<String, PluginError> {
    serialize(HttpRequest {
        method: "GET".into(),
        url: format!("{API_BASE}/user?agent={AGENT}"),
        headers: auth_headers(api_key),
        body: None,
    })
}

pub fn build_unlock_request(url: &str, api_key: &str) -> Result<String, PluginError> {
    serialize(HttpRequest {
        method: "GET".into(),
        url: format!(
            "{API_BASE}/link/unlock?agent={AGENT}&link={}",
            percent_encode(url)
        ),
        headers: auth_headers(api_key),
        body: None,
    })
}

fn auth_headers(api_key: &str) -> HashMap<String, String> {
    HashMap::from([
        ("Authorization".to_string(), format!("Bearer {api_key}")),
        ("Accept".to_string(), "application/json".to_string()),
    ])
}

fn serialize(request: HttpRequest) -> Result<String, PluginError> {
    serde_json::to_string(&request).map_err(PluginError::SerdeJson)
}

/// Turn a transport-level response into a body, mapping the few statuses
/// AllDebrid does send onto account errors before the body is even read.
pub fn into_api_body(response: HttpResponse) -> Result<String, PluginError> {
    match response.status {
        401 | 403 => Err(PluginError::InvalidCredentials),
        429 => Err(PluginError::RateLimited(truncate(&response.body, 128))),
        _ => response.into_success_body(),
    }
}

/// Unwrap `{"status":"success","data":…}`, or turn `{"status":"error",…}`
/// into the typed error its code deserves.
pub fn parse_envelope(body: &str) -> Result<serde_json::Value, PluginError> {
    let envelope: Envelope = serde_json::from_str(body)
        .map_err(|e| PluginError::InvalidApiResponse(format!("{e}: {}", truncate(body, 128))))?;
    match envelope.status.as_str() {
        "success" => envelope
            .data
            .ok_or_else(|| PluginError::InvalidApiResponse("success without data".into())),
        "error" => Err(match envelope.error {
            Some(error) => classify_error(&error.code, &error.message),
            None => PluginError::InvalidApiResponse("error without an error object".into()),
        }),
        other => Err(PluginError::InvalidApiResponse(format!(
            "unknown status '{other}'"
        ))),
    }
}

/// Map an AllDebrid error code onto the host's machine codes.
///
/// `LINK_HOST_NOT_SUPPORTED` is the one that matters most: it is how the
/// cascade learns this debrid does not cover the hoster and falls through
/// to the next tier instead of reporting a fake success (R-04).
///
/// The account-level codes are the dangerous direction. An account error
/// makes the host stop using the account for *every* hoster, so a failure
/// that is really the hoster's or AllDebrid's must never be classified as
/// one — that would disable a paid account over someone else's outage.
fn classify_error(code: &str, message: &str) -> PluginError {
    let detail = format!("{code}: {}", truncate(message, 128));
    match code {
        "AUTH_MISSING_APIKEY" | "AUTH_BAD_APIKEY" | "AUTH_BLOCKED" | "AUTH_USER_BANNED" => {
            PluginError::InvalidCredentials
        }
        "MUST_BE_PREMIUM" => PluginError::AccountExpired(detail),
        // `NO_SERVER` is AllDebrid having no download server free, not the
        // account being out of quota.
        "LINK_TEMPORARY_UNAVAILABLE"
        | "LINK_HOST_FULL"
        | "LINK_TOO_MANY_DOWNLOADS"
        | "NO_SERVER" => PluginError::RateLimited(detail),
        "FREE_TRIAL_LIMIT_REACHED" => PluginError::QuotaExceeded(detail),
        // `LINK_HOST_UNAVAILABLE` is the hoster being down, so it falls
        // through to the next tier and leaves the account usable.
        "LINK_IS_MISSING"
        | "BAD_LINK"
        | "LINK_DOWN"
        | "LINK_HOST_NOT_SUPPORTED"
        | "LINK_HOST_UNAVAILABLE" => PluginError::HosterUnavailable(detail),
        "LINK_PASS_PROTECTED" => PluginError::LinkPasswordRequired(detail),
        _ => PluginError::InvalidApiResponse(detail),
    }
}

/// The host hands credentials over as `{"password": "<api key>"}`.
pub fn parse_credential_response(raw: &str) -> Result<String, PluginError> {
    #[derive(Deserialize)]
    struct CredentialResponse {
        #[serde(default)]
        password: String,
    }
    // The serde error is deliberately dropped: on a type mismatch it quotes
    // the offending value, and here that value is the API key (R-05).
    let credential: CredentialResponse = serde_json::from_str(raw).map_err(|_| {
        PluginError::HostResponse("credential payload is not the expected shape".into())
    })?;
    let key = credential.password.trim();
    if key.is_empty() {
        return Err(PluginError::InvalidCredentials);
    }
    Ok(key.to_string())
}
