//! HTTP envelope exchanged with the host's `http_request` function.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::PluginError;

#[derive(Debug, Serialize)]
pub(crate) struct HttpRequest {
    pub method: String,
    pub url: String,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub headers: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct HttpResponse {
    pub status: u16,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: String,
}

/// Cap responses so a hostile server can't force megabyte-scale parsing.
/// Real AllDebrid JSON payloads weigh a few kilobytes.
pub(crate) const MAX_BODY_BYTES: usize = 1024 * 1024;

impl HttpResponse {
    pub fn into_success_body(self) -> Result<String, PluginError> {
        if (200..300).contains(&self.status) {
            if self.body.len() > MAX_BODY_BYTES {
                return Err(PluginError::HttpStatus {
                    status: self.status,
                    message: format!("body exceeds {MAX_BODY_BYTES} bytes"),
                });
            }
            Ok(self.body)
        } else if self.status == 404 || self.status == 410 {
            Err(PluginError::HosterUnavailable(format!(
                "status {}",
                self.status
            )))
        } else {
            Err(PluginError::HttpStatus {
                status: self.status,
                message: truncate(&self.body, 256),
            })
        }
    }
}

pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut cut = max;
        while !s.is_char_boundary(cut) && cut > 0 {
            cut -= 1;
        }
        format!("{}…", &s[..cut])
    }
}

pub fn parse_http_response(raw: &str) -> Result<HttpResponse, PluginError> {
    serde_json::from_str(raw).map_err(|e| PluginError::HostResponse(e.to_string()))
}

/// Percent-encode a value for use in a query string.
///
/// Only the unreserved set survives, so a hoster URL round-trips intact
/// through `?link=`.
pub(crate) fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_2xx_response_yields_its_body() {
        let response = HttpResponse {
            status: 200,
            headers: HashMap::new(),
            body: "{}".into(),
        };
        assert_eq!(response.into_success_body().expect("2xx is success"), "{}");
    }

    #[test]
    fn a_404_response_reports_the_link_as_unavailable() {
        let response = HttpResponse {
            status: 404,
            headers: HashMap::new(),
            body: String::new(),
        };
        let error = response.into_success_body().expect_err("404 is a failure");
        assert_eq!(error.code(), "HOSTER_NO_FILE");
    }

    #[test]
    fn percent_encode_escapes_every_reserved_character_of_a_url() {
        assert_eq!(
            percent_encode("https://a.io/f?x=1&y=2"),
            "https%3A%2F%2Fa.io%2Ff%3Fx%3D1%26y%3D2"
        );
    }

    #[test]
    fn percent_encode_leaves_the_unreserved_set_alone() {
        assert_eq!(percent_encode("aZ0-._~"), "aZ0-._~");
    }
}
