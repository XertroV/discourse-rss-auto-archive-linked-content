use url::Url;

/// Tracking parameters to strip from URLs.
const TRACKING_PARAMS: &[&str] = &[
    "utm_source",
    "utm_medium",
    "utm_campaign",
    "utm_term",
    "utm_content",
    "utm_cid",
    "utm_reader",
    "utm_name",
    "fbclid",
    "gclid",
    "gclsrc",
    "dclid",
    "zanpid",
    "ref",
    "ref_src",
    "ref_url",
    "referer",
    "referrer",
    "source",
    "share",
    "igshid",
    "s", // Twitter share param
    "t", // Twitter timestamp param (sometimes tracking)
    "feature",
    "context",
];

/// Normalize a URL by applying common transformations.
#[must_use]
pub fn normalize_url(url: &str) -> String {
    let parsed = match Url::parse(url) {
        Ok(u) => u,
        Err(_) => return url.to_string(),
    };

    // Skip non-HTTP URLs
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return url.to_string();
    }

    let mut normalized = parsed.clone();

    // Force HTTPS
    if normalized.scheme() == "http" {
        let _ = normalized.set_scheme("https");
    }

    // Lowercase the host
    if let Some(host) = normalized.host_str() {
        let lower_host = host.to_lowercase();
        if host != lower_host {
            let _ = normalized.set_host(Some(&lower_host));
        }
    }

    // Remove default ports
    if normalized.port() == Some(443) || normalized.port() == Some(80) {
        let _ = normalized.set_port(None);
    }

    // Remove tracking parameters
    let filtered_params: Vec<(String, String)> = normalized
        .query_pairs()
        .filter(|(key, _)| !is_tracking_param(key))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    if filtered_params.is_empty() {
        normalized.set_query(None);
    } else {
        let new_query: String = filtered_params
            .iter()
            .map(|(k, v)| {
                if v.is_empty() {
                    k.clone()
                } else {
                    format!("{k}={v}")
                }
            })
            .collect::<Vec<_>>()
            .join("&");
        normalized.set_query(Some(&new_query));
    }

    // Remove fragment
    normalized.set_fragment(None);

    // Normalize trailing slash for paths
    let path = normalized.path().to_string();
    if path.ends_with('/') && path.len() > 1 {
        normalized.set_path(path.trim_end_matches('/'));
    }

    normalized.to_string()
}

/// Check if a query parameter is a tracking parameter.
fn is_tracking_param(key: &str) -> bool {
    let lower = key.to_lowercase();
    TRACKING_PARAMS.contains(&lower.as_str()) || lower.starts_with("utm_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_force_https() {
        assert_eq!(
            normalize_url("http://example.com/path"),
            "https://example.com/path"
        );
    }

    #[test]
    fn test_lowercase_host() {
        assert_eq!(
            normalize_url("https://EXAMPLE.COM/path"),
            "https://example.com/path"
        );
    }

    #[test]
    fn test_remove_tracking_params() {
        assert_eq!(
            normalize_url("https://example.com/path?utm_source=test&id=123"),
            "https://example.com/path?id=123"
        );
    }

    #[test]
    fn test_remove_all_tracking_params() {
        assert_eq!(
            normalize_url("https://example.com/path?utm_source=test&utm_medium=web"),
            "https://example.com/path"
        );
    }

    #[test]
    fn test_remove_fragment() {
        assert_eq!(
            normalize_url("https://example.com/path#section"),
            "https://example.com/path"
        );
    }

    #[test]
    fn test_remove_trailing_slash() {
        assert_eq!(
            normalize_url("https://example.com/path/"),
            "https://example.com/path"
        );
    }

    #[test]
    fn test_keep_root_slash() {
        assert_eq!(
            normalize_url("https://example.com/"),
            "https://example.com/"
        );
    }

    #[test]
    fn test_remove_default_port() {
        assert_eq!(
            normalize_url("https://example.com:443/path"),
            "https://example.com/path"
        );
    }

    #[test]
    fn test_preserve_non_tracking_params() {
        assert_eq!(
            normalize_url("https://example.com/path?page=2&sort=new"),
            "https://example.com/path?page=2&sort=new"
        );
    }

    #[test]
    fn test_invalid_url_passthrough() {
        assert_eq!(normalize_url("not a url"), "not a url");
    }

    #[test]
    fn test_non_http_passthrough() {
        assert_eq!(
            normalize_url("mailto:test@example.com"),
            "mailto:test@example.com"
        );
    }
}
