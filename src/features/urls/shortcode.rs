// 🔑 HASHING — turning a long URL into a short code.  (architecture.md §3)
//
//   digest = SHA-256(url)          or SHA-256(url + "#" + attempt) on retries
//   number = first 8 bytes of digest as a u64
//   code   = 7 base62 digits of number            → e.g. "aZ3kP9x"
//
// ≈ in Node:
//   const digest = crypto.createHash("sha256").update(input).digest();
//   const n = digest.readBigUInt64BE(0);

use sha2::{Digest, Sha256};
use url::Url;

pub const CODE_LENGTH: usize = 7;
const BASE62: &[u8; 62] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
const MAX_URL_LENGTH: usize = 2048;
/// Paths used by the API itself can't be aliases, or they'd shadow routes.
const RESERVED: &[&str] = &["api", "health"];

/// SHA-256 → 64-char lowercase hex. Stored as `url_hash` for dedupe lookups.
pub fn sha256_hex(input: &str) -> String {
    let digest = Sha256::digest(input.as_bytes()); // 32 bytes
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Deterministic short code for `long_url`. `attempt` salts the input so a
/// collision can be retried with a different (but still deterministic) code.
pub fn generate_code(long_url: &str, attempt: u32) -> String {
    let input = if attempt == 0 {
        long_url.to_string()
    } else {
        format!("{long_url}#{attempt}")
    };
    let digest = Sha256::digest(input.as_bytes());

    // First 8 bytes → u64 (big-endian). `try_into` converts a slice to a fixed array.
    let first8: [u8; 8] = digest[..8].try_into().expect("sha256 is 32 bytes");
    let number = u64::from_be_bytes(first8);

    to_base62(number, CODE_LENGTH)
}

/// Encode `n` as exactly `len` base62 digits (keeps the lowest `len` digits).
fn to_base62(mut n: u64, len: usize) -> String {
    let mut out = Vec::with_capacity(len);
    for _ in 0..len {
        out.push(BASE62[(n % 62) as usize]);
        n /= 62;
    }
    out.reverse();
    String::from_utf8(out).expect("base62 alphabet is ASCII")
}

/// Validates and normalises a URL. Normalising (lowercase scheme/host, etc.)
/// means "HTTPS://Example.com" and "https://example.com" hash to the same code.
pub fn normalize_url(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    if raw.len() > MAX_URL_LENGTH {
        return Err(format!("url must be at most {MAX_URL_LENGTH} characters"));
    }
    let parsed = Url::parse(raw).map_err(|e| format!("invalid url: {e}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("only http and https urls are allowed".into());
    }
    if parsed.host_str().is_none_or(|h| h.is_empty()) {
        return Err("url must have a host".into());
    }
    Ok(parsed.to_string())
}

/// Custom alias rules: 3–32 chars of [A-Za-z0-9_-], not a reserved word.
pub fn validate_alias(alias: &str) -> Result<(), String> {
    if !(3..=32).contains(&alias.len()) {
        return Err("custom_alias must be 3-32 characters".into());
    }
    if !alias.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("custom_alias may only contain letters, digits, '-' and '_'".into());
    }
    if RESERVED.contains(&alias.to_ascii_lowercase().as_str()) {
        return Err(format!("custom_alias '{alias}' is reserved"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_known_value() {
        // Well-known test vector: sha256("abc")
        assert_eq!(
            sha256_hex("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn code_is_deterministic_and_7_base62_chars() {
        let a = generate_code("https://example.com/", 0);
        let b = generate_code("https://example.com/", 0);
        assert_eq!(a, b, "same input must give same code");
        assert_eq!(a.len(), CODE_LENGTH);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn retry_attempt_changes_code() {
        let url = "https://example.com/";
        assert_ne!(generate_code(url, 0), generate_code(url, 1));
    }

    #[test]
    fn different_urls_give_different_codes() {
        assert_ne!(
            generate_code("https://example.com/a", 0),
            generate_code("https://example.com/b", 0)
        );
    }

    #[test]
    fn base62_encoding() {
        assert_eq!(to_base62(0, 3), "000");
        assert_eq!(to_base62(61, 2), "0Z");
        assert_eq!(to_base62(62, 2), "10");
    }

    #[test]
    fn normalize_lowercases_scheme_and_host() {
        assert_eq!(
            normalize_url("  HTTPS://Example.COM/Path?q=1 ").unwrap(),
            "https://example.com/Path?q=1" // path stays case-sensitive
        );
    }

    #[test]
    fn normalize_rejects_bad_urls() {
        assert!(normalize_url("not a url").is_err());
        assert!(normalize_url("ftp://example.com").is_err());
        assert!(normalize_url("javascript:alert(1)").is_err());
        assert!(normalize_url(&format!("https://e.com/{}", "a".repeat(3000))).is_err());
    }

    #[test]
    fn alias_validation() {
        assert!(validate_alias("my-blog_2").is_ok());
        assert!(validate_alias("ab").is_err());
        assert!(validate_alias("has space").is_err());
        assert!(validate_alias("API").is_err());
    }
}
