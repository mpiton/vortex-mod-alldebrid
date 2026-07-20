//! Real ABI smoke tests for every runtime export of the release WASM artifact.
//!
//! Requires the WASM artifact at
//! `target/wasm32-wasip1/release/vortex_mod_alldebrid.wasm`. To produce it:
//!
//! ```bash
//! cargo build --target wasm32-wasip1 --release
//! ```

use std::path::PathBuf;

use extism::{Function, UserData, Val, PTR};
use serde_json::{json, Value};

const WASM_REL_PATH: &str = "target/wasm32-wasip1/release/vortex_mod_alldebrid.wasm";
const FILE_URL: &str = "https://www.mediafire.com/file/abc/archive.zip/file";
const DIRECT_URL: &str = "https://s3.alldebrid.com/dl/abc123/archive.zip";

fn wasm_path() -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(WASM_REL_PATH);
    assert!(
        path.is_file(),
        "missing release WASM artifact at {}; run `cargo build --target wasm32-wasip1 --release` first",
        path.display()
    );
    path
}

/// Answer `/user` and `/link/unlock` with the payloads AllDebrid really sends.
fn stub_http_request() -> Function {
    stub_http_body(|request| {
        if request.contains("/v4/user") {
            json!({"status": "success", "data": {"user": {
                "isPremium": true, "premiumUntil": 1798761600
            }}})
        } else {
            json!({"status": "success", "data": {
                "link": DIRECT_URL, "filename": "archive.zip", "filesize": 104857600
            }})
        }
        .to_string()
    })
}

/// HTTP 200 carrying an AllDebrid error envelope — the shape that would read
/// as success if the body were not parsed.
fn stub_http_api_error(code: &'static str) -> Function {
    stub_http_body(move |_| {
        json!({"status": "error", "error": {"code": code, "message": "stubbed"}}).to_string()
    })
}

fn stub_http_body<F>(make_body: F) -> Function
where
    F: Fn(&str) -> String + Send + Sync + 'static,
{
    Function::new(
        "http_request",
        [PTR],
        [PTR],
        UserData::<()>::default(),
        move |plugin, inputs, outputs, _user_data: UserData<()>| {
            let request: Vec<u8> = plugin.memory_get_val(&inputs[0])?;
            let request = String::from_utf8(request)?;
            let response =
                json!({"status": 200, "headers": {}, "body": make_body(&request)}).to_string();
            let handle = plugin.memory_new(&response)?;
            outputs[0] = Val::I64(handle.offset() as i64);
            Ok(())
        },
    )
}

fn stub_http_status(status: u16) -> Function {
    Function::new(
        "http_request",
        [PTR],
        [PTR],
        UserData::<()>::default(),
        move |plugin, _inputs, outputs, _user_data: UserData<()>| {
            let response =
                json!({"status": status, "headers": {}, "body": "untrusted"}).to_string();
            let handle = plugin.memory_new(&response)?;
            outputs[0] = Val::I64(handle.offset() as i64);
            Ok(())
        },
    )
}

fn stub_get_credential() -> Function {
    Function::new(
        "get_credential",
        [PTR],
        [PTR],
        UserData::<()>::default(),
        |plugin, _inputs, outputs, _user_data: UserData<()>| {
            let credential = json!({"username": "", "password": "test-api-key"}).to_string();
            let handle = plugin.memory_new(&credential)?;
            outputs[0] = Val::I64(handle.offset() as i64);
            Ok(())
        },
    )
}

fn stub_get_credential_missing() -> Function {
    Function::new(
        "get_credential",
        [PTR],
        [PTR],
        UserData::<()>::default(),
        |_plugin, _inputs, _outputs, _user_data: UserData<()>| {
            Err(extism::Error::msg("get_credential: no credential found"))
        },
    )
}

fn load_plugin() -> extism::Plugin {
    load_plugin_with(stub_http_request(), stub_get_credential())
}

fn load_plugin_with(http_request: Function, get_credential: Function) -> extism::Plugin {
    let manifest = extism::Manifest::new([extism::Wasm::file(wasm_path())]);
    extism::Plugin::new(&manifest, [http_request, get_credential], true).expect("load wasm")
}

#[test]
fn wasm_can_handle_claims_a_covered_hoster_and_declines_the_rest() {
    let mut plugin = load_plugin();

    let covered: String = plugin.call("can_handle", FILE_URL).expect("can_handle");
    let other: String = plugin
        .call("can_handle", "https://example.com/file/abc")
        .expect("can_handle");

    assert_eq!(covered.trim(), "true");
    assert_eq!(other.trim(), "false");
}

#[test]
fn wasm_supports_playlist_is_always_false() {
    let mut plugin = load_plugin();

    let result: String = plugin
        .call("supports_playlist", FILE_URL)
        .expect("supports_playlist");

    assert_eq!(result.trim(), "false");
}

#[test]
fn wasm_extract_links_returns_the_unrestricted_direct_url() {
    let mut plugin = load_plugin();

    let links: String = plugin
        .call("extract_links", FILE_URL)
        .expect("extract_links");
    let links: Value = serde_json::from_str(&links).expect("extract_links JSON");

    assert_eq!(links["files"][0]["url"], FILE_URL);
    assert_eq!(links["files"][0]["direct_url"], DIRECT_URL);
    assert_eq!(links["files"][0]["requires_captcha"], false);
}

#[test]
fn wasm_resolve_stream_url_returns_the_bare_direct_url() {
    let mut plugin = load_plugin();

    let direct_url: String = plugin
        .call("resolve_stream_url", json!({"url": FILE_URL}).to_string())
        .expect("resolve_stream_url");

    assert_eq!(direct_url, DIRECT_URL);
}

#[test]
fn wasm_validate_account_reports_premium_expiry_to_the_accounts_view() {
    let mut plugin = load_plugin();

    let outcome: String = plugin
        .call("validate_account", "")
        .expect("validate_account");
    let outcome: Value = serde_json::from_str(&outcome).expect("validation JSON");

    assert_eq!(outcome["valid"], true);
    assert_eq!(outcome["valid_until"], 1798761600u64);
}

#[test]
fn wasm_validate_account_fails_without_a_host_credential() {
    let mut plugin = load_plugin_with(stub_http_request(), stub_get_credential_missing());

    plugin
        .call::<_, String>("validate_account", "")
        .expect_err("a debrid has no anonymous mode");
}

#[test]
fn wasm_extract_links_declines_a_hoster_this_debrid_does_not_cover() {
    let mut plugin = load_plugin();

    let error = plugin
        .call::<_, String>("extract_links", "https://example.com/file/abc")
        .expect_err("an uncovered hoster must not resolve");

    assert!(error.to_string().contains("PLUGIN_ERROR"), "{error}");
}

#[test]
fn wasm_api_error_envelopes_keep_their_machine_codes_across_the_boundary() {
    // HTTP 200 + an error body is AllDebrid's normal failure shape. Each of
    // these must stay typed so the cascade knows why the tier declined (R-04).
    for (api_code, expected) in [
        ("LINK_HOST_NOT_SUPPORTED", "HOSTER_NO_FILE"),
        ("LINK_DOWN", "HOSTER_NO_FILE"),
        // A hoster outage stays a hoster problem, so the account survives it.
        ("LINK_HOST_UNAVAILABLE", "HOSTER_NO_FILE"),
        ("AUTH_BAD_APIKEY", "ACCOUNT_INVALID_CREDENTIALS"),
        ("MUST_BE_PREMIUM", "ACCOUNT_EXPIRED"),
        ("LINK_TOO_MANY_DOWNLOADS", "ACCOUNT_COOLDOWN"),
        ("NO_SERVER", "ACCOUNT_COOLDOWN"),
        ("FREE_TRIAL_LIMIT_REACHED", "ACCOUNT_QUOTA_EXCEEDED"),
        ("LINK_PASS_PROTECTED", "HOSTER_AUTHENTICATION_REQUIRED"),
    ] {
        let mut plugin = load_plugin_with(stub_http_api_error(api_code), stub_get_credential());
        let error = plugin
            .call::<_, String>("extract_links", FILE_URL)
            .expect_err("an error envelope is never a success");
        assert!(
            error.to_string().contains(expected),
            "{api_code} returned {error}"
        );
    }
}

#[test]
fn wasm_transport_level_failures_stay_typed() {
    for (status, expected) in [
        (401, "ACCOUNT_INVALID_CREDENTIALS"),
        (403, "ACCOUNT_INVALID_CREDENTIALS"),
        (429, "ACCOUNT_COOLDOWN"),
        (404, "HOSTER_NO_FILE"),
    ] {
        let mut plugin = load_plugin_with(stub_http_status(status), stub_get_credential());
        let error = plugin
            .call::<_, String>("validate_account", "")
            .expect_err("an HTTP failure must remain typed");
        assert!(
            error.to_string().contains(expected),
            "status {status} returned {error}"
        );
    }
}
