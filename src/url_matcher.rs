//! Which URLs this debrid claims.
//!
//! A debrid plugin claims any link on a hoster it can unrestrict. The real
//! answer lives behind `GET /v4/hosts/domains`, but `can_handle` runs on
//! every pasted URL and must stay synchronous and offline, so the list is
//! static.
//!
//! ponytail: static domain list, refresh from `/v4/hosts/domains` cached via
//! `set_state` if it drifts. Being wrong is cheap in both directions — a
//! missing domain just skips the debrid tier, and a stale one gets
//! `LINK_HOST_NOT_SUPPORTED` at unlock time, which maps to `HOSTER_NO_FILE`
//! and lets the cascade fall through with a real reason (R-04).

/// Hosters AllDebrid unrestricts, matched on the registrable domain.
///
/// Deliberately file hosters only: streaming and media sites have their own
/// crawler plugins in Vortex and should not be diverted through a debrid.
const SUPPORTED_HOSTS: &[&str] = &[
    "1fichier.com",
    "alfafile.net",
    "clicknupload.to",
    "ddownload.com",
    "depositfiles.com",
    "drop.download",
    "easybytez.com",
    "fastfile.cc",
    "fikper.com",
    "filefactory.com",
    "filespace.com",
    "filestore.to",
    "flashbit.cc",
    "gigapeta.com",
    "gofile.io",
    "hexload.com",
    "hitfile.net",
    "isra.cloud",
    "k2s.cc",
    "katfile.com",
    "keep2share.cc",
    "mediafire.com",
    "mega.nz",
    "modsbase.com",
    "nitroflare.com",
    "pixeldrain.com",
    "rapidgator.net",
    "rg.to",
    "scribd.com",
    "sendspace.com",
    "turbobit.net",
    "uploady.io",
    "uptobox.com",
    "usersdrive.com",
    "worldbytez.com",
];

/// True when AllDebrid covers the hoster this URL points at.
pub fn is_supported(url: &str) -> bool {
    match registrable_host(url) {
        Some(host) => SUPPORTED_HOSTS
            .iter()
            .any(|supported| host == *supported || host.ends_with(&format!(".{supported}"))),
        None => false,
    }
}

/// Lowercased host of an http(s) URL, with any `www.` prefix removed.
fn registrable_host(url: &str) -> Option<String> {
    let (scheme, rest) = url.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let authority = match rest.find(['/', '?', '#']) {
        Some(index) => &rest[..index],
        None => rest,
    };
    // Strip any userinfo, then the port. IPv6 literals have no registrable
    // domain, so rejecting them here is correct rather than lossy.
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    if authority.starts_with('[') {
        return None;
    }
    let host = authority.split(':').next().unwrap_or(authority);
    if host.is_empty() {
        return None;
    }
    let host = host.to_ascii_lowercase();
    Some(host.strip_prefix("www.").unwrap_or(&host).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("https://www.mediafire.com/file/abc/x.zip/file", true)]
    #[case("https://rapidgator.net/file/abcdef", true)]
    #[case("http://1fichier.com/?abc123def", true)]
    #[case("https://download.uptobox.com/x", true)] // subdomain
    #[case("https://MEDIAFIRE.COM/file/a", true)] // case-insensitive
    #[case("https://mega.nz:443/file/a", true)] // explicit port
    #[case("https://example.com/file/a", false)]
    #[case("https://notmediafire.com/file/a", false)] // suffix must be a label
    #[case("https://www.youtube.com/watch?v=x", false)] // crawler plugin owns it
    #[case("ftp://mediafire.com/x", false)]
    #[case("https://[::1]/x", false)]
    #[case("not a url", false)]
    fn is_supported_recognises_covered_hosters(#[case] url: &str, #[case] expected: bool) {
        assert_eq!(is_supported(url), expected);
    }
}
