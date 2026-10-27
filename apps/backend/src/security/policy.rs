//! Password policy. Length is the main rule (NIST SP 800-63B); on top of it a password must not
//! be a well-known one, must not be built from the account's own e-mail address or name, and must
//! not be one repeated character.

use crate::{
    error::ApiError,
    security::auth::{MAX_PASSWORD_LEN, MIN_PASSWORD_LEN},
};

/// Passwords that are long enough to pass the length rule and still appear in every breach list.
const COMMON: &[&str] = &[
    "123456789012",
    "1234567890123",
    "12345678901234",
    "abcdefghijkl",
    "administrator",
    "adminadminadmin",
    "changemechangeme",
    "changemenow",
    "correcthorsebatterystaple",
    "iloveyou1234",
    "iloveyouiloveyou",
    "letmein123456",
    "passwordpassword",
    "password1234",
    "password12345",
    "password123456",
    "password1234567",
    "passw0rd1234",
    "p@ssw0rd1234",
    "qwertyuiop12",
    "qwertyuiopas",
    "qwertyuiopasdf",
    "qwertyuiop123",
    "qwertyuiopasdfghjkl",
    "welcome12345",
    "welcome123456",
    "welcometowork",
    "trustno1trustno1",
    "monkeymonkey12",
    "1q2w3e4r5t6y",
    "1qaz2wsx3edc",
    "zaq12wsxcde3",
    "asdfghjkl123",
    "asdfghjklqwe",
    "000000000000",
    "111111111111",
    "123123123123",
    "123456123456",
    "987654321098",
];

/// Lower-cases and drops everything that is not a letter or digit, so `Pass-Word 1234!` and
/// `password1234` are the same password for the blocklist.
fn fold(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// `context` are values tied to the account (e-mail, first and last name).
pub fn check_password(password: &str, context: &[&str]) -> Result<(), ApiError> {
    let n = password.chars().count();
    if !(MIN_PASSWORD_LEN..=MAX_PASSWORD_LEN).contains(&n) {
        return Err(ApiError::BadRequest(format!(
            "password must be {MIN_PASSWORD_LEN}-{MAX_PASSWORD_LEN} characters"
        )));
    }
    let folded = fold(password);
    let mut distinct: Vec<char> = password.chars().collect();
    distinct.sort_unstable();
    distinct.dedup();
    if distinct.len() < 5 {
        return Err(ApiError::BadRequest(
            "password is too repetitive, use more different characters".into(),
        ));
    }
    if COMMON.iter().any(|c| fold(c) == folded) {
        return Err(ApiError::BadRequest(
            "password is too common, choose another one".into(),
        ));
    }
    for value in context {
        // Only the part before the `@` of an address is personal; short fragments would reject
        // too many honest passwords.
        let local = value.split('@').next().unwrap_or(value);
        for part in local.split(|c: char| !c.is_alphanumeric()) {
            let part = fold(part);
            if part.chars().count() >= 4 && folded.contains(&part) {
                return Err(ApiError::BadRequest(
                    "password must not contain your name or e-mail address".into(),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bad(p: &str, ctx: &[&str]) -> bool {
        matches!(check_password(p, ctx), Err(ApiError::BadRequest(_)))
    }

    #[test]
    fn enforces_length() {
        assert!(bad("short-one", &[]));
        assert!(bad(&"a1b2".repeat(40), &[]));
        assert!(check_password("correct-horse-battery", &[]).is_ok());
    }

    #[test]
    fn rejects_common_passwords_in_any_spelling() {
        assert!(bad("password1234", &[]));
        assert!(bad("Password-1234", &[]));
        assert!(bad("QWERTYUIOP 12", &[]));
        assert!(bad("1234567890123", &[]));
    }

    #[test]
    fn rejects_repetition() {
        assert!(bad("aaaaaaaaaaaaaa", &[]));
        assert!(bad("abababababab", &[]));
    }

    #[test]
    fn rejects_personal_data_but_not_short_fragments() {
        let ctx = ["jane.doe@example.com", "Jane", "Doe"];
        assert!(bad("my-jane-secret-99", &ctx));
        assert!(bad("xx-JANE.doe-xx-yy", &ctx));
        assert!(!bad("MyDoe!Secret!42", &[]));
        // "doe" is only three letters: not enough to be a giveaway
        assert!(check_password("unrelated-doe-phrase-7", &["Doe"]).is_ok());
    }
}
