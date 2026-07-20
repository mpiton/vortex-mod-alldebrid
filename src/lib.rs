//! AllDebrid debrid plugin for Vortex.
//!
//! One job: hand a covered hoster link to AllDebrid and give the host back
//! the direct CDN URL it should download from, never the original page
//! (R-02). The API key lives in the host keyring and is read through
//! `get_credential`; it is never logged or persisted here (R-05).

pub mod api;
pub mod error;
pub mod http;
pub mod responses;
pub mod url_matcher;

#[cfg(target_family = "wasm")]
mod plugin_api;

use serde::Serialize;

use crate::error::PluginError;
use crate::responses::UnlockedLink;

pub const SERVICE_NAME: &str = "vortex-mod-alldebrid";

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ExtractLinksResponse {
    pub kind: &'static str,
    pub files: Vec<FileLink>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct FileLink {
    /// The hoster URL we were asked about, echoed back.
    pub url: String,
    pub filename: Option<String>,
    pub size_bytes: Option<u64>,
    /// The unrestricted CDN URL the host downloads from.
    pub direct_url: String,
    pub resumable: bool,
    pub requires_captcha: bool,
}

pub fn handle_can_handle(url: &str) -> String {
    url_matcher::is_supported(url).to_string()
}

pub fn handle_supports_playlist(_url: &str) -> String {
    "false".to_string()
}

pub fn ensure_supported_url(url: &str) -> Result<(), PluginError> {
    if url_matcher::is_supported(url) {
        Ok(())
    } else {
        Err(PluginError::UnsupportedUrl(url.to_string()))
    }
}

pub fn build_unlock_response(url: &str, unlocked: UnlockedLink) -> ExtractLinksResponse {
    ExtractLinksResponse {
        kind: "file",
        files: vec![FileLink {
            url: url.to_string(),
            filename: unlocked.filename,
            size_bytes: unlocked.size_bytes,
            direct_url: unlocked.direct_url,
            // AllDebrid CDN links honour range requests.
            resumable: true,
            requires_captcha: false,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "https://www.mediafire.com/file/abc/archive.zip/file";

    #[test]
    fn can_handle_claims_a_covered_hoster_and_declines_anything_else() {
        assert_eq!(handle_can_handle(URL), "true");
        assert_eq!(handle_can_handle("https://example.com/x"), "false");
    }

    #[test]
    fn supports_playlist_is_always_false_for_a_debrid() {
        assert_eq!(handle_supports_playlist(URL), "false");
    }

    #[test]
    fn ensure_supported_url_rejects_a_hoster_outside_the_coverage_list() {
        let error = ensure_supported_url("https://example.com/x").expect_err("not covered");
        assert!(matches!(error, PluginError::UnsupportedUrl(_)));
    }

    #[test]
    fn build_unlock_response_echoes_the_source_url_and_carries_the_direct_link() {
        let response = build_unlock_response(
            URL,
            UnlockedLink {
                direct_url: "https://cdn.alldebrid/x/archive.zip".into(),
                filename: Some("archive.zip".into()),
                size_bytes: Some(1024),
            },
        );

        let file = &response.files[0];
        assert_eq!(file.url, URL);
        assert_eq!(file.direct_url, "https://cdn.alldebrid/x/archive.zip");
        assert!(!file.requires_captcha);
        assert!(file.resumable);
    }

    #[test]
    fn the_serialised_payload_matches_the_host_wire_format() {
        let response = build_unlock_response(
            URL,
            UnlockedLink {
                direct_url: "https://cdn.alldebrid/x".into(),
                filename: None,
                size_bytes: None,
            },
        );
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&response).expect("serialise"))
                .expect("valid json");

        assert_eq!(json["files"][0]["url"], URL);
        assert_eq!(json["files"][0]["direct_url"], "https://cdn.alldebrid/x");
        assert_eq!(json["files"][0]["requires_captcha"], false);
    }
}
