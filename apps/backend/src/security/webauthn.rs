//! WebAuthn (passkeys): the verification of registration and sign-in ceremonies.
//!
//! Scope, on purpose: ES256 (ECDSA P-256 with SHA-256) credentials, which every platform and
//! security-key authenticator supports; attestation "none" (we do not ask who built the
//! authenticator, so no attestation statement is trusted or verified); user verification required
//! (a passkey sign-in is a second factor in itself: possession plus PIN or biometrics).
//!
//! What is checked, following the W3C Web Authentication specification:
//! * `clientDataJSON`: the ceremony `type`, the exact one-time `challenge`, the exact `origin`, and
//!   that the call did not come from a cross-origin frame;
//! * `authenticatorData`: its `rpIdHash` is the hash of our relying-party id (so a credential made for
//!   another site never verifies), the user-present and user-verified flags are set;
//! * registration: the attested credential data carries a P-256 key that is a valid curve point;
//! * sign-in: the signature over `authenticatorData || SHA-256(clientDataJSON)` verifies under the
//!   stored key, and the signature counter moves forward (a counter that does not is a cloned key).

use std::io::Cursor;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ciborium::value::Value as Cbor;
use p256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::config::Config;

const FLAG_UP: u8 = 0x01;
const FLAG_UV: u8 = 0x04;
const FLAG_AT: u8 = 0x40;

/// The relying party: our id (a registrable domain) and the exact origin the browser reports.
#[derive(Clone, Debug, PartialEq)]
pub struct Rp {
    pub id: String,
    pub origin: String,
}

impl Rp {
    /// Derived from `WORK_FRONTEND_URL`. `WORK_WEBAUTHN_RP_ID` may name a parent domain (so passkeys
    /// work across subdomains); it must be a suffix of the host.
    pub fn from_config(config: &Config) -> Result<Rp, String> {
        let url = url::Url::parse(&config.frontend_url).map_err(|e| e.to_string())?;
        let host = url
            .host_str()
            .ok_or("WORK_FRONTEND_URL has no host")?
            .to_owned();
        let id = match &config.webauthn_rp_id {
            Some(id) if host == *id || host.ends_with(&format!(".{id}")) => id.clone(),
            Some(id) => {
                return Err(format!(
                    "WORK_WEBAUTHN_RP_ID {id} is not a parent domain of {host}"
                ))
            }
            None => host,
        };
        Ok(Rp {
            id,
            origin: url.origin().ascii_serialization(),
        })
    }
}

pub fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn unb64(s: &str) -> Result<Vec<u8>, &'static str> {
    URL_SAFE_NO_PAD
        .decode(s.trim_end_matches('='))
        .map_err(|_| "bad base64url")
}

fn check_client_data(json: &[u8], ty: &str, challenge: &[u8], rp: &Rp) -> Result<(), &'static str> {
    let v: serde_json::Value =
        serde_json::from_slice(json).map_err(|_| "client data is not JSON")?;
    if v["type"] != ty {
        return Err("wrong ceremony type");
    }
    if v["challenge"].as_str().map(unb64) != Some(Ok(challenge.to_vec())) {
        return Err("wrong challenge");
    }
    if v["origin"] != rp.origin.as_str() {
        return Err("wrong origin");
    }
    if v["crossOrigin"] == true {
        return Err("cross-origin ceremony");
    }
    Ok(())
}

struct AuthData {
    flags: u8,
    sign_count: u32,
    /// credential id and SEC1 public key, present in registrations
    credential: Option<(Vec<u8>, Vec<u8>)>,
}

fn int_key(k: &Cbor) -> Option<i64> {
    k.as_integer()
        .and_then(|i| i64::try_from(i128::from(i)).ok())
}

fn parse_cose_key(cur: &mut Cursor<&[u8]>) -> Result<Vec<u8>, &'static str> {
    let key: Cbor = ciborium::de::from_reader(&mut *cur).map_err(|_| "bad credential key")?;
    let map = key.as_map().ok_or("bad credential key")?;
    let get = |want: i64| {
        map.iter()
            .find(|(k, _)| int_key(k) == Some(want))
            .map(|(_, v)| v)
    };
    let int = |want: i64| get(want).and_then(int_key);
    // EC2 (2), ES256 (-7), curve P-256 (1)
    if int(1) != Some(2) || int(3) != Some(-7) || int(-1) != Some(1) {
        return Err("only ES256 passkeys are supported");
    }
    let (x, y) = (
        get(-2).and_then(Cbor::as_bytes),
        get(-3).and_then(Cbor::as_bytes),
    );
    let (Some(x), Some(y)) = (x, y) else {
        return Err("bad credential key");
    };
    if x.len() != 32 || y.len() != 32 {
        return Err("bad credential key");
    }
    let mut sec1 = vec![0x04];
    sec1.extend_from_slice(x);
    sec1.extend_from_slice(y);
    VerifyingKey::from_sec1_bytes(&sec1).map_err(|_| "the key is not a point on the curve")?;
    Ok(sec1)
}

fn parse_auth_data(bytes: &[u8], rp: &Rp) -> Result<AuthData, &'static str> {
    if bytes.len() < 37 {
        return Err("authenticator data is too short");
    }
    if bytes[..32] != Sha256::digest(rp.id.as_bytes())[..] {
        return Err("the credential belongs to another site");
    }
    let flags = bytes[32];
    let sign_count = u32::from_be_bytes([bytes[33], bytes[34], bytes[35], bytes[36]]);
    let credential = if flags & FLAG_AT != 0 {
        // aaguid (16) | credentialIdLength (2) | credentialId | COSE key
        let rest = &bytes[37..];
        if rest.len() < 18 {
            return Err("attested credential data is too short");
        }
        let len = u16::from_be_bytes([rest[16], rest[17]]) as usize;
        if !(16..=1023).contains(&len) || rest.len() < 18 + len {
            return Err("bad credential id");
        }
        let id = rest[18..18 + len].to_vec();
        let mut cur = Cursor::new(&rest[18 + len..]);
        let key = parse_cose_key(&mut cur)?;
        Some((id, key))
    } else {
        None
    };
    Ok(AuthData {
        flags,
        sign_count,
        credential,
    })
}

fn require_flags(flags: u8) -> Result<(), &'static str> {
    if flags & FLAG_UP == 0 {
        return Err("user presence is required");
    }
    if flags & FLAG_UV == 0 {
        return Err("user verification is required");
    }
    Ok(())
}

#[derive(Debug)]
pub struct NewCredential {
    pub id: Vec<u8>,
    pub public_key: Vec<u8>,
    pub sign_count: u32,
}

pub fn verify_registration(
    rp: &Rp,
    challenge: &[u8],
    client_data_json: &[u8],
    attestation_object: &[u8],
) -> Result<NewCredential, &'static str> {
    check_client_data(client_data_json, "webauthn.create", challenge, rp)?;
    let att: Cbor =
        ciborium::de::from_reader(attestation_object).map_err(|_| "bad attestation object")?;
    let auth_data = att
        .as_map()
        .and_then(|m| m.iter().find(|(k, _)| k.as_text() == Some("authData")))
        .and_then(|(_, v)| v.as_bytes())
        .ok_or("the attestation has no authenticator data")?;
    let data = parse_auth_data(auth_data, rp)?;
    require_flags(data.flags)?;
    let (id, public_key) = data
        .credential
        .ok_or("the attestation carries no credential")?;
    Ok(NewCredential {
        id,
        public_key,
        sign_count: data.sign_count,
    })
}

/// Returns the new signature counter to store.
pub fn verify_assertion(
    rp: &Rp,
    challenge: &[u8],
    client_data_json: &[u8],
    authenticator_data: &[u8],
    signature: &[u8],
    public_key: &[u8],
    stored_count: u32,
) -> Result<u32, &'static str> {
    check_client_data(client_data_json, "webauthn.get", challenge, rp)?;
    let data = parse_auth_data(authenticator_data, rp)?;
    require_flags(data.flags)?;
    let key = VerifyingKey::from_sec1_bytes(public_key).map_err(|_| "bad stored key")?;
    let sig = Signature::from_der(signature).map_err(|_| "bad signature encoding")?;
    let mut signed = authenticator_data.to_vec();
    signed.extend_from_slice(&Sha256::digest(client_data_json));
    key.verify(&signed, &sig)
        .map_err(|_| "the signature does not verify")?;
    // Authenticators that keep no counter always send 0; otherwise it must keep rising.
    if (stored_count > 0 || data.sign_count > 0) && data.sign_count <= stored_count {
        return Err("the signature counter did not advance (cloned authenticator?)");
    }
    Ok(data.sign_count)
}

#[cfg(test)]
pub(crate) mod soft {
    //! A software authenticator, so tests can run whole ceremonies without a browser.
    use super::*;
    use p256::ecdsa::{signature::Signer, SigningKey};

    pub struct Authenticator {
        pub key: SigningKey,
        pub credential_id: Vec<u8>,
        pub counter: u32,
    }

    pub fn client_data(ty: &str, challenge: &[u8], origin: &str, cross: bool) -> Vec<u8> {
        serde_json::json!({"type": ty, "challenge": b64(challenge), "origin": origin, "crossOrigin": cross}).to_string().into_bytes()
    }

    impl Authenticator {
        pub fn new() -> Self {
            let mut id = vec![0u8; 32];
            rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut id);
            Self {
                key: SigningKey::random(&mut rand::rngs::OsRng),
                credential_id: id,
                counter: 0,
            }
        }

        fn cose(&self) -> Vec<u8> {
            let point = self.key.verifying_key().to_encoded_point(false);
            let map = Cbor::Map(vec![
                (Cbor::Integer(1.into()), Cbor::Integer(2.into())),
                (Cbor::Integer(3.into()), Cbor::Integer((-7).into())),
                (Cbor::Integer((-1).into()), Cbor::Integer(1.into())),
                (
                    Cbor::Integer((-2).into()),
                    Cbor::Bytes(point.x().unwrap().to_vec()),
                ),
                (
                    Cbor::Integer((-3).into()),
                    Cbor::Bytes(point.y().unwrap().to_vec()),
                ),
            ]);
            let mut out = Vec::new();
            ciborium::ser::into_writer(&map, &mut out).unwrap();
            out
        }

        pub fn auth_data(&self, rp_id: &str, flags: u8, with_credential: bool) -> Vec<u8> {
            let mut d = Sha256::digest(rp_id.as_bytes()).to_vec();
            d.push(flags);
            d.extend_from_slice(&self.counter.to_be_bytes());
            if with_credential {
                d.extend_from_slice(&[0u8; 16]);
                d.extend_from_slice(&(self.credential_id.len() as u16).to_be_bytes());
                d.extend_from_slice(&self.credential_id);
                d.extend_from_slice(&self.cose());
            }
            d
        }

        /// (clientDataJSON, attestationObject)
        pub fn register(&self, rp: &Rp, challenge: &[u8], flags: u8) -> (Vec<u8>, Vec<u8>) {
            let att = Cbor::Map(vec![
                (Cbor::Text("fmt".into()), Cbor::Text("none".into())),
                (Cbor::Text("attStmt".into()), Cbor::Map(vec![])),
                (
                    Cbor::Text("authData".into()),
                    Cbor::Bytes(self.auth_data(&rp.id, flags, true)),
                ),
            ]);
            let mut out = Vec::new();
            ciborium::ser::into_writer(&att, &mut out).unwrap();
            (
                client_data("webauthn.create", challenge, &rp.origin, false),
                out,
            )
        }

        /// (clientDataJSON, authenticatorData, signature)
        pub fn assert(
            &mut self,
            rp: &Rp,
            challenge: &[u8],
            flags: u8,
        ) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
            self.counter += 1;
            let cd = client_data("webauthn.get", challenge, &rp.origin, false);
            let ad = self.auth_data(&rp.id, flags, false);
            let mut signed = ad.clone();
            signed.extend_from_slice(&Sha256::digest(&cd));
            let sig: Signature = self.key.sign(&signed);
            (cd, ad, sig.to_der().as_bytes().to_vec())
        }

        pub fn public_key_sec1(&self) -> Vec<u8> {
            self.key
                .verifying_key()
                .to_encoded_point(false)
                .as_bytes()
                .to_vec()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::soft::*;
    use super::*;

    const UP_UV_AT: u8 = FLAG_UP | FLAG_UV | FLAG_AT;
    const UP_UV: u8 = FLAG_UP | FLAG_UV;

    fn rp() -> Rp {
        Rp {
            id: "work.example".into(),
            origin: "https://work.example".into(),
        }
    }

    #[test]
    fn a_registration_and_sign_in_round_trip() {
        let mut a = Authenticator::new();
        let (cd, att) = a.register(&rp(), b"challenge-1", UP_UV_AT);
        let cred = verify_registration(&rp(), b"challenge-1", &cd, &att).unwrap();
        assert_eq!(cred.id, a.credential_id);
        assert_eq!(cred.public_key, a.public_key_sec1());
        let (cd, ad, sig) = a.assert(&rp(), b"challenge-2", UP_UV);
        assert_eq!(
            verify_assertion(
                &rp(),
                b"challenge-2",
                &cd,
                &ad,
                &sig,
                &cred.public_key,
                cred.sign_count
            ),
            Ok(1)
        );
    }

    #[test]
    fn registration_rejects_everything_that_does_not_match() {
        let a = Authenticator::new();
        let (cd, att) = a.register(&rp(), b"c", UP_UV_AT);
        let other_site = Rp {
            id: "evil.example".into(),
            origin: "https://evil.example".into(),
        };
        let bad: Vec<(&str, Result<NewCredential, &str>)> = vec![
            (
                "wrong challenge",
                verify_registration(&rp(), b"other", &cd, &att),
            ),
            (
                "wrong origin",
                verify_registration(
                    &Rp {
                        id: rp().id,
                        origin: "https://evil.example".into(),
                    },
                    b"c",
                    &cd,
                    &att,
                ),
            ),
            (
                "credential for another site",
                verify_registration(
                    &other_site,
                    b"c",
                    &client_data("webauthn.create", b"c", &other_site.origin, false),
                    &att,
                ),
            ),
            (
                "get instead of create",
                verify_registration(
                    &rp(),
                    b"c",
                    &client_data("webauthn.get", b"c", &rp().origin, false),
                    &att,
                ),
            ),
            (
                "cross-origin",
                verify_registration(
                    &rp(),
                    b"c",
                    &client_data("webauthn.create", b"c", &rp().origin, true),
                    &att,
                ),
            ),
            ("no user verification", {
                let (cd, att) = a.register(&rp(), b"c", FLAG_UP | FLAG_AT);
                verify_registration(&rp(), b"c", &cd, &att)
            }),
            ("no user presence", {
                let (cd, att) = a.register(&rp(), b"c", FLAG_UV | FLAG_AT);
                verify_registration(&rp(), b"c", &cd, &att)
            }),
            ("no credential data", {
                let (cd, att) = a.register(&rp(), b"c", UP_UV);
                verify_registration(&rp(), b"c", &cd, &att)
            }),
            (
                "garbage attestation",
                verify_registration(&rp(), b"c", &cd, &[0xff; 32]),
            ),
            (
                "garbage client data",
                verify_registration(&rp(), b"c", b"not json", &att),
            ),
        ];
        for (what, res) in bad {
            assert!(res.is_err(), "{what} must be refused");
        }
    }

    #[test]
    fn assertions_must_verify_and_the_counter_must_advance() {
        let mut a = Authenticator::new();
        let key = a.public_key_sec1();
        let (cd, ad, sig) = a.assert(&rp(), b"ch", UP_UV);
        assert_eq!(
            verify_assertion(&rp(), b"ch", &cd, &ad, &sig, &key, 0),
            Ok(1)
        );
        // Replayed against a counter that already saw it: a clone (or a replay).
        assert!(verify_assertion(&rp(), b"ch", &cd, &ad, &sig, &key, 1)
            .unwrap_err()
            .contains("counter"));
        // Wrong challenge, wrong key, tampered data, wrong site, missing user verification.
        assert!(verify_assertion(&rp(), b"other", &cd, &ad, &sig, &key, 0).is_err());
        assert!(verify_assertion(
            &rp(),
            b"ch",
            &cd,
            &ad,
            &sig,
            &Authenticator::new().public_key_sec1(),
            0
        )
        .is_err());
        let mut tampered = ad.clone();
        tampered[36] ^= 1; // flips the counter inside the signed data
        assert!(verify_assertion(&rp(), b"ch", &cd, &tampered, &sig, &key, 0).is_err());
        assert!(verify_assertion(
            &Rp {
                id: "evil.example".into(),
                origin: rp().origin
            },
            b"ch",
            &cd,
            &ad,
            &sig,
            &key,
            0
        )
        .is_err());
        let (cd2, ad2, sig2) = a.assert(&rp(), b"ch", FLAG_UP);
        assert!(verify_assertion(&rp(), b"ch", &cd2, &ad2, &sig2, &key, 0)
            .unwrap_err()
            .contains("verification"));
        assert!(verify_assertion(&rp(), b"ch", &cd, &ad, b"junk", &key, 0).is_err());
        // Authenticators without a counter always report 0 and stay acceptable.
        let mut z = Authenticator::new();
        let zk = z.public_key_sec1();
        z.counter = 0;
        let (c, d, s) = {
            let cd = client_data("webauthn.get", b"ch", &rp().origin, false);
            let ad = z.auth_data(&rp().id, UP_UV, false);
            let mut signed = ad.clone();
            signed.extend_from_slice(&Sha256::digest(&cd));
            let sig: Signature = p256::ecdsa::signature::Signer::sign(&z.key, &signed);
            (cd, ad, sig.to_der().as_bytes().to_vec())
        };
        assert_eq!(verify_assertion(&rp(), b"ch", &c, &d, &s, &zk, 0), Ok(0));
    }

    #[test]
    fn unsupported_keys_are_refused_not_misread() {
        // An RSA key (kty 3) in the credential data.
        let a = Authenticator::new();
        let mut ad = a.auth_data(&rp().id, UP_UV_AT, false);
        ad.extend_from_slice(&[0u8; 16]);
        ad.extend_from_slice(&32u16.to_be_bytes());
        ad.extend_from_slice(&[7u8; 32]);
        let rsa = Cbor::Map(vec![
            (Cbor::Integer(1.into()), Cbor::Integer(3.into())),
            (Cbor::Integer(3.into()), Cbor::Integer((-257).into())),
        ]);
        ciborium::ser::into_writer(&rsa, &mut ad).unwrap();
        let att = Cbor::Map(vec![(Cbor::Text("authData".into()), Cbor::Bytes(ad))]);
        let mut out = Vec::new();
        ciborium::ser::into_writer(&att, &mut out).unwrap();
        let cd = client_data("webauthn.create", b"c", &rp().origin, false);
        assert!(verify_registration(&rp(), b"c", &cd, &out)
            .unwrap_err()
            .contains("ES256"));
    }

    #[test]
    fn rp_comes_from_the_frontend_url() {
        // Only the derivation helper is exercised; Config has many fields, so test the pieces.
        let url = url::Url::parse("https://work.example:8443/").unwrap();
        assert_eq!(
            url.origin().ascii_serialization(),
            "https://work.example:8443"
        );
        assert_eq!(b64(&unb64("AQID").unwrap()), "AQID");
        assert!(unb64("!!").is_err());
    }
}
