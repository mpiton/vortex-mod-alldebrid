//! Plugin error type.

use thiserror::Error;

/// Errors raised by the AllDebrid plugin.
#[derive(Debug, Error)]
pub enum PluginError {
    #[error("JSON error: {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("AllDebrid HTTP returned status {status}: {message}")]
    HttpStatus { status: u16, message: String },

    #[error("host function response invalid: {0}")]
    HostResponse(String),

    #[error("URL is not on a hoster AllDebrid unrestricts: {0}")]
    UnsupportedUrl(String),

    #[error("AllDebrid rejected the configured API key as invalid")]
    InvalidCredentials,

    #[error("the AllDebrid account is not premium: {0}")]
    AccountExpired(String),

    #[error("AllDebrid asked us to slow down: {0}")]
    RateLimited(String),

    #[error("the AllDebrid account has exhausted its quota: {0}")]
    QuotaExceeded(String),

    /// The debrid declines the link — dead, removed, or on a hoster it does
    /// not cover. The host reads this as `HosterNoFile` and moves the
    /// resolution cascade down to the next tier (R-04).
    #[error("AllDebrid cannot serve this link: {0}")]
    HosterUnavailable(String),

    #[error("the link is password-protected: {0}")]
    LinkPasswordRequired(String),

    #[error("AllDebrid returned an unexpected payload: {0}")]
    InvalidApiResponse(String),
}

impl PluginError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidCredentials => "ACCOUNT_INVALID_CREDENTIALS",
            Self::AccountExpired(_) => "ACCOUNT_EXPIRED",
            Self::RateLimited(_) => "ACCOUNT_COOLDOWN",
            Self::QuotaExceeded(_) => "ACCOUNT_QUOTA_EXCEEDED",
            Self::HosterUnavailable(_) => "HOSTER_NO_FILE",
            Self::LinkPasswordRequired(_) => "HOSTER_AUTHENTICATION_REQUIRED",
            _ => "PLUGIN_ERROR",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_errors_have_stable_machine_codes() {
        assert_eq!(
            PluginError::InvalidCredentials.code(),
            "ACCOUNT_INVALID_CREDENTIALS"
        );
        assert_eq!(
            PluginError::AccountExpired("free".into()).code(),
            "ACCOUNT_EXPIRED"
        );
        assert_eq!(
            PluginError::RateLimited("wait".into()).code(),
            "ACCOUNT_COOLDOWN"
        );
        assert_eq!(
            PluginError::QuotaExceeded("daily".into()).code(),
            "ACCOUNT_QUOTA_EXCEEDED"
        );
    }

    #[test]
    fn an_uncovered_hoster_reports_the_cascade_fall_through_code() {
        assert_eq!(
            PluginError::HosterUnavailable("not supported".into()).code(),
            "HOSTER_NO_FILE"
        );
    }

    #[test]
    fn a_password_protected_link_asks_the_host_for_authentication() {
        assert_eq!(
            PluginError::LinkPasswordRequired("pass".into()).code(),
            "HOSTER_AUTHENTICATION_REQUIRED"
        );
    }
}
