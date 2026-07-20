//! Parser tests against captured AllDebrid payloads.
//!
//! The service answers HTTP 200 for nearly every failure, so these fixtures
//! are what stop an error envelope from being read as a success (R-04).

use vortex_mod_alldebrid::api::{
    build_unlock_request, build_user_request, parse_credential_response,
};
use vortex_mod_alldebrid::responses::{parse_unlock_response, parse_user_response};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap_or_else(|e| panic!("fixture {name}: {e}"))
}

#[test]
fn test_parse_user_premium_account_returns_the_expiry_timestamp() {
    let status = parse_user_response(&fixture("user_premium.json")).expect("premium is valid");

    assert_eq!(status.valid_until, Some(1798761600));
}

#[test]
fn test_parse_user_free_account_reports_expired() {
    let error = parse_user_response(&fixture("user_free.json")).expect_err("free is unusable");

    assert_eq!(error.code(), "ACCOUNT_EXPIRED");
}

#[test]
fn test_parse_user_bad_apikey_reports_invalid_credentials() {
    let error =
        parse_user_response(&fixture("auth_bad_apikey.json")).expect_err("bad key is a failure");

    assert_eq!(error.code(), "ACCOUNT_INVALID_CREDENTIALS");
}

#[test]
fn test_parse_unlock_success_returns_the_direct_cdn_url() {
    let link = parse_unlock_response(&fixture("unlock_success.json")).expect("unlock succeeded");

    assert_eq!(
        link.direct_url,
        "https://s3.alldebrid.com/dl/abc123/archive.zip"
    );
    assert_eq!(link.filename.as_deref(), Some("archive.zip"));
    assert_eq!(link.size_bytes, Some(104857600));
}

#[test]
fn test_parse_unlock_uncovered_hoster_reports_no_file_so_the_cascade_falls_through() {
    let error = parse_unlock_response(&fixture("unlock_host_not_supported.json"))
        .expect_err("an uncovered hoster is not a success");

    assert_eq!(error.code(), "HOSTER_NO_FILE");
}

#[test]
fn test_parse_unlock_delayed_link_reports_cooldown_instead_of_an_empty_url() {
    let error = parse_unlock_response(&fixture("unlock_delayed.json"))
        .expect_err("a delayed link has no direct url yet");

    assert_eq!(error.code(), "ACCOUNT_COOLDOWN");
}

#[test]
fn test_parse_unlock_password_protected_link_asks_for_authentication() {
    let error = parse_unlock_response(&fixture("unlock_pass_protected.json"))
        .expect_err("a protected link needs a password");

    assert_eq!(error.code(), "HOSTER_AUTHENTICATION_REQUIRED");
}

#[test]
fn test_parse_unlock_exhausted_quota_reports_quota_exceeded() {
    let error = parse_unlock_response(&fixture("unlock_quota_reached.json"))
        .expect_err("an exhausted quota is a failure");

    assert_eq!(error.code(), "ACCOUNT_QUOTA_EXCEEDED");
}

#[test]
fn test_parse_unlock_rejects_a_truncated_payload() {
    let error = parse_unlock_response("{\"status\":\"succ").expect_err("truncated json");

    assert_eq!(error.code(), "PLUGIN_ERROR");
}

#[test]
fn test_build_user_request_names_the_agent_and_carries_the_bearer_token() {
    let request: serde_json::Value =
        serde_json::from_str(&build_user_request("k3y").expect("build")).expect("valid json");

    assert_eq!(request["method"], "GET");
    assert_eq!(
        request["url"],
        "https://api.alldebrid.com/v4/user?agent=vortex"
    );
    assert_eq!(request["headers"]["Authorization"], "Bearer k3y");
}

#[test]
fn test_build_unlock_request_percent_encodes_the_hoster_link() {
    let request: serde_json::Value = serde_json::from_str(
        &build_unlock_request("https://mediafire.com/file/a?b=1", "k3y").expect("build"),
    )
    .expect("valid json");

    assert_eq!(
        request["url"],
        "https://api.alldebrid.com/v4/link/unlock?agent=vortex\
         &link=https%3A%2F%2Fmediafire.com%2Ffile%2Fa%3Fb%3D1"
    );
}

#[test]
fn test_the_api_key_never_appears_in_an_error_message() {
    // R-05: a leaked key in a log line is exactly what the keyring is for.
    let error = parse_user_response(&fixture("auth_bad_apikey.json")).expect_err("bad key");

    assert!(!error.to_string().contains("k3y"), "{error}");
}

#[test]
fn test_a_malformed_credential_payload_never_echoes_the_api_key() {
    // serde quotes the offending value on a type mismatch, and on this path
    // the offending value is the key itself (R-05).
    let error = parse_credential_response(r#"{"password": 90210}"#).expect_err("not a string");

    assert!(!error.to_string().contains("90210"), "{error}");
}

fn error_envelope(code: &str) -> String {
    serde_json::json!({"status": "error", "error": {"code": code, "message": "stubbed"}})
        .to_string()
}

#[test]
fn test_someone_elses_outage_is_never_blamed_on_the_account() {
    // An ACCOUNT_* code makes the host stop using the account for every
    // hoster. A hoster outage or an AllDebrid capacity problem must not
    // cost the user their paid account (R-04).
    for code in [
        "LINK_HOST_UNAVAILABLE",
        "LINK_DOWN",
        "LINK_HOST_NOT_SUPPORTED",
    ] {
        let error = parse_unlock_response(&error_envelope(code)).expect_err("not a success");
        assert_eq!(error.code(), "HOSTER_NO_FILE", "{code} should fall through");
    }

    let error = parse_unlock_response(&error_envelope("NO_SERVER")).expect_err("not a success");
    assert_eq!(
        error.code(),
        "ACCOUNT_COOLDOWN",
        "NO_SERVER is AllDebrid being busy, not the account being out of quota"
    );
}

#[test]
fn test_real_account_problems_still_reach_the_host() {
    for (code, expected) in [
        ("MUST_BE_PREMIUM", "ACCOUNT_EXPIRED"),
        ("AUTH_BAD_APIKEY", "ACCOUNT_INVALID_CREDENTIALS"),
        ("FREE_TRIAL_LIMIT_REACHED", "ACCOUNT_QUOTA_EXCEEDED"),
        ("LINK_TOO_MANY_DOWNLOADS", "ACCOUNT_COOLDOWN"),
        ("LINK_PASS_PROTECTED", "HOSTER_AUTHENTICATION_REQUIRED"),
    ] {
        let error = parse_unlock_response(&error_envelope(code)).expect_err("not a success");
        assert_eq!(error.code(), expected, "{code}");
    }
}
