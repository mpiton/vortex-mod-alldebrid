//! WASM-only entry points: `#[plugin_fn]` exports + `#[host_fn]` imports.
//!
//! Every export that talks to AllDebrid needs the API key, so a missing or
//! empty credential is a typed error rather than a silent anonymous attempt
//! — a debrid has no free mode to fall back to.

use extism_pdk::*;

use crate::api::{
    build_unlock_request, build_user_request, into_api_body, parse_credential_response,
};
use crate::error::PluginError;
use crate::http::parse_http_response;
use crate::responses::{parse_unlock_response, parse_user_response, UnlockedLink};
use crate::{
    build_unlock_response, ensure_supported_url, handle_can_handle, handle_supports_playlist,
    SERVICE_NAME,
};

#[host_fn]
extern "ExtismHost" {
    fn http_request(req: String) -> String;
    fn get_credential(service: String) -> String;
}

#[plugin_fn]
pub fn can_handle(url: String) -> FnResult<String> {
    Ok(handle_can_handle(&url))
}

#[plugin_fn]
pub fn supports_playlist(url: String) -> FnResult<String> {
    Ok(handle_supports_playlist(&url))
}

/// Check the API key against the live service and report premium expiry
/// back to the Accounts view (R-01).
#[plugin_fn]
pub fn validate_account(_input: String) -> FnResult<String> {
    let key = read_api_key().map_err(error_to_fn_error)?;
    let body = call_api(build_user_request(&key).map_err(error_to_fn_error)?)?;
    let status = parse_user_response(&body).map_err(error_to_fn_error)?;

    let mut payload = serde_json::json!({ "valid": true });
    if let Some(valid_until) = status.valid_until {
        payload["valid_until"] = valid_until.into();
    }
    Ok(payload.to_string())
}

/// Unrestrict the link (R-02).
#[plugin_fn]
pub fn extract_links(url: String) -> FnResult<String> {
    let unlocked = unrestrict(&url)?;
    Ok(serde_json::to_string(&build_unlock_response(
        &url, unlocked,
    ))?)
}

/// Same unrestrict path, returning the bare direct URL.
///
/// Input JSON: `{ "url": "..." }` — extra fields are ignored.
#[plugin_fn]
pub fn resolve_stream_url(input: String) -> FnResult<String> {
    #[derive(serde::Deserialize)]
    struct Input {
        url: String,
    }
    let params: Input =
        serde_json::from_str(&input).map_err(|e| error_to_fn_error(PluginError::SerdeJson(e)))?;
    Ok(unrestrict(&params.url)?.direct_url)
}

fn unrestrict(url: &str) -> FnResult<UnlockedLink> {
    ensure_supported_url(url).map_err(error_to_fn_error)?;
    let key = read_api_key().map_err(error_to_fn_error)?;
    let body = call_api(build_unlock_request(url, &key).map_err(error_to_fn_error)?)?;
    parse_unlock_response(&body).map_err(error_to_fn_error)
}

fn read_api_key() -> Result<String, PluginError> {
    // SAFETY: host registers `get_credential` in the `ExtismHost` namespace before
    // any export is callable; ABI marshalled by `#[host_fn]`. See
    // src-tauri/src/adapters/driven/plugin/host_functions.rs.
    let raw = unsafe { get_credential(SERVICE_NAME.to_string()) }
        .map_err(|e| PluginError::HostResponse(format!("get_credential: {e}")))?;
    parse_credential_response(&raw)
}

fn call_api(request: String) -> FnResult<String> {
    // SAFETY: host registers `http_request` in the `ExtismHost` namespace before
    // any export is callable; capability is gated on the `http` declaration in
    // `plugin.toml`. See src-tauri/src/adapters/driven/plugin/host_functions.rs.
    let raw = unsafe { http_request(request) }
        .map_err(|e| PluginError::HostResponse(format!("http_request: {e}")))
        .map_err(error_to_fn_error)?;
    let response = parse_http_response(&raw).map_err(error_to_fn_error)?;
    into_api_body(response).map_err(error_to_fn_error)
}

fn error_to_fn_error(err: PluginError) -> WithReturnCode<extism_pdk::Error> {
    extism_pdk::Error::msg(format!("{}: {err}", err.code())).into()
}
