//! Time-based one-time passwords (RFC 6238: HMAC-SHA1, 6 digits, 30 s) for authenticator apps,
//! plus the single-use recovery codes that go with them.

use hmac::{Hmac, Mac};
use rand::RngCore;
use sha1::Sha1;
use subtle::ConstantTimeEq;

pub const STEP_SECS: i64 = 30;
const DIGITS: u32 = 6;
const SECRET_BYTES: usize = 20;
const B32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

pub fn generate_secret() -> Vec<u8> {
    let mut secret = vec![0u8; SECRET_BYTES];
    rand::rngs::OsRng.fill_bytes(&mut secret);
    secret
}

/// RFC 4648 base32 without padding, the format authenticator apps expect.
pub fn base32(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() * 8).div_ceil(5));
    let (mut buffer, mut bits) = (0u32, 0u32);
    for &byte in data {
        buffer = (buffer << 8) | byte as u32;
        bits += 8;
        while bits >= 5 {
            out.push(B32[((buffer >> (bits - 5)) & 31) as usize] as char);
            bits -= 5;
        }
        buffer &= (1 << bits) - 1;
    }
    if bits > 0 {
        out.push(B32[((buffer << (5 - bits)) & 31) as usize] as char);
    }
    out
}

fn code_for_step(secret: &[u8], step: i64) -> u32 {
    let mut mac = Hmac::<Sha1>::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(&(step as u64).to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = (digest[19] & 0x0f) as usize;
    let bin = u32::from_be_bytes([
        digest[offset] & 0x7f,
        digest[offset + 1],
        digest[offset + 2],
        digest[offset + 3],
    ]);
    bin % 10u32.pow(DIGITS)
}

pub fn step_at(unix_secs: i64) -> i64 {
    unix_secs.div_euclid(STEP_SECS)
}

/// The code an authenticator would show at `unix_secs`. For tests only.
#[cfg(test)]
pub fn code_at(secret: &[u8], unix_secs: i64) -> String {
    format!(
        "{:0width$}",
        code_for_step(secret, step_at(unix_secs)),
        width = DIGITS as usize
    )
}

/// Accepts the current step and one on either side (clock drift). Returns the matched step, which
/// the caller stores so the same code cannot be used twice: steps at or below `last_step` are
/// refused.
pub fn verify(secret: &[u8], code: &str, unix_secs: i64, last_step: Option<i64>) -> Option<i64> {
    let code = code.trim().replace(' ', "");
    if code.len() != DIGITS as usize || !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let now = step_at(unix_secs);
    let mut matched = None;
    // Check every candidate (no early exit) so timing does not reveal which step matched.
    for step in now - 1..=now + 1 {
        let expected = format!(
            "{:0width$}",
            code_for_step(secret, step),
            width = DIGITS as usize
        );
        if bool::from(expected.as_bytes().ct_eq(code.as_bytes()))
            && last_step.is_none_or(|last| step > last)
        {
            matched = Some(step);
        }
    }
    matched
}

fn pct(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// `otpauth://` URI that authenticator apps read from a QR code.
pub fn otpauth_uri(issuer: &str, account: &str, secret: &[u8]) -> String {
    format!(
        "otpauth://totp/{}:{}?secret={}&issuer={}&algorithm=SHA1&digits={DIGITS}&period={STEP_SECS}",
        pct(issuer),
        pct(account),
        base32(secret),
        pct(issuer)
    )
}

const RECOVERY_ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789"; // no 0/o/1/l/i
pub const RECOVERY_CODES: usize = 10;

/// `xxxxx-xxxxx`, about 50 bits each. Shown once; only a hash is stored.
pub fn generate_recovery_codes() -> Vec<String> {
    let mut rng = rand::rngs::OsRng;
    let limit = 256 - (256 % RECOVERY_ALPHABET.len());
    let mut pick = |n: usize| -> String {
        (0..n)
            .map(|_| loop {
                // Rejection sampling keeps the choice uniform.
                let b = (rng.next_u32() & 0xff) as usize;
                if b < limit {
                    break RECOVERY_ALPHABET[b % RECOVERY_ALPHABET.len()] as char;
                }
            })
            .collect()
    };
    (0..RECOVERY_CODES)
        .map(|_| format!("{}-{}", pick(5), pick(5)))
        .collect()
}

/// Case, spaces and the dash do not matter when typing a recovery code.
pub fn normalize_recovery_code(code: &str) -> String {
    code.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RFC_SECRET: &[u8] = b"12345678901234567890";

    #[test]
    fn matches_rfc6238_vectors() {
        // RFC 6238 appendix B (SHA-1), truncated to six digits.
        for (t, code) in [
            (59, "287082"),
            (1_111_111_109, "081804"),
            (1_234_567_890, "005924"),
            (2_000_000_000, "279037"),
        ] {
            assert_eq!(verify(RFC_SECRET, code, t, None), Some(step_at(t)), "t={t}");
        }
    }

    #[test]
    fn tolerates_one_step_of_drift_but_not_more() {
        let t = 1_111_111_109;
        assert!(verify(RFC_SECRET, "081804", t + 30, None).is_some());
        assert!(verify(RFC_SECRET, "081804", t - 30, None).is_some());
        assert!(verify(RFC_SECRET, "081804", t + 90, None).is_none());
    }

    #[test]
    fn a_code_works_only_once() {
        let t = 1_111_111_109;
        let step = verify(RFC_SECRET, "081804", t, None).unwrap();
        assert!(verify(RFC_SECRET, "081804", t, Some(step)).is_none());
    }

    #[test]
    fn rejects_malformed_codes() {
        for c in ["", "12345", "1234567", "abcdef", "28708 2x"] {
            assert!(verify(RFC_SECRET, c, 59, None).is_none(), "{c}");
        }
        assert!(verify(RFC_SECRET, "287 082", 59, None).is_some());
    }

    #[test]
    fn base32_matches_rfc4648() {
        assert_eq!(base32(b"foobar"), "MZXW6YTBOI");
        assert_eq!(base32(b"fooba"), "MZXW6YTB");
        assert_eq!(base32(b"f"), "MY");
        assert_eq!(base32(RFC_SECRET), "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
    }

    #[test]
    fn otpauth_uri_is_escaped() {
        let uri = otpauth_uri("work", "a b@x.test", RFC_SECRET);
        assert!(uri.starts_with("otpauth://totp/work:a%20b%40x.test?secret=GEZD"));
        assert!(uri.contains("&issuer=work&"));
    }

    #[test]
    fn recovery_codes_are_unique_and_normalize() {
        let codes = generate_recovery_codes();
        assert_eq!(codes.len(), RECOVERY_CODES);
        let mut sorted = codes.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), RECOVERY_CODES);
        assert!(codes
            .iter()
            .all(|c| c.len() == 11 && c.as_bytes()[5] == b'-'));
        assert_eq!(normalize_recovery_code(" ABCDE-fghjk "), "abcdefghjk");
    }
}
