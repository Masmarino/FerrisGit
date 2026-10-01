use axum::http::HeaderMap;
use base64::Engine;

/// Git's HTTP client sends the personal access token as the Basic password.
pub fn parse_basic_auth(headers: &HeaderMap) -> Option<(String, String)> {
    let header_value = headers
        .get(axum::http::header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    let encoded = header_value.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let decoded = String::from_utf8(decoded).ok()?;
    let (username, password) = decoded.split_once(':')?;
    Some((username.to_string(), password.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderValue};
    use base64::Engine;

    #[test]
    fn parses_a_valid_basic_auth_header() {
        let mut headers = HeaderMap::new();
        let encoded = base64::engine::general_purpose::STANDARD.encode("florian:fg_abc");
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Basic {encoded}")).unwrap(),
        );

        assert_eq!(
            parse_basic_auth(&headers),
            Some(("florian".to_string(), "fg_abc".to_string()))
        );
    }

    #[test]
    fn returns_none_when_the_header_is_missing() {
        assert_eq!(parse_basic_auth(&HeaderMap::new()), None);
    }

    #[test]
    fn returns_none_for_a_non_basic_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_static("Bearer sometoken"),
        );
        assert_eq!(parse_basic_auth(&headers), None);
    }
}
