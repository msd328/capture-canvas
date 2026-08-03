//! Native RS256/ES256 ID-token signature verification and identity promotion.
//!
//! The verifier accepts only a key already selected by the bounded JWKS resolver,
//! verifies the exact compact-JWS `header.payload` bytes, and validates the pinned
//! issuer, audience, subject, timestamps and authorization nonce before returning a
//! native identity. It does not persist tokens, establish a session, authorize cloud
//! access, or expose identity data to the WebView.

use crate::oidc_claims::{inspect_unverified_id_token, UnverifiedIdTokenClaims};
use crate::oidc_jwks::{resolve_id_token_key, ValidatedJwk};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ring::signature;
use uuid::Uuid;

const MAX_ID_TOKEN_BYTES: usize = 16 * 1024;
const MAX_SIGNATURE_SEGMENT_BYTES: usize = 1024;
const MAX_SIGNATURE_BYTES: usize = 512;
const ES256_SIGNATURE_BYTES: usize = 64;
const P256_UNCOMPRESSED_PUBLIC_KEY_BYTES: usize = 65;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifiedNativeIdentity {
    subject: Uuid,
    issued_at: i64,
    expires_at: i64,
    authenticated_at: i64,
}

#[allow(dead_code)]
impl VerifiedNativeIdentity {
    pub(crate) const fn subject(&self) -> Uuid {
        self.subject
    }

    pub(crate) const fn issued_at(&self) -> i64 {
        self.issued_at
    }

    pub(crate) const fn expires_at(&self) -> i64 {
        self.expires_at
    }

    pub(crate) const fn authenticated_at(&self) -> i64 {
        self.authenticated_at
    }
}

/// Verify a compact ID token and promote its claims to a native identity.
///
/// The JWKS resolver performs strict header parsing and exact `kid`/algorithm key
/// selection. Claims are inspected only after the signature succeeds. The returned
/// identity contains no access, refresh or ID-token bytes.
#[allow(dead_code)]
pub(crate) async fn verify_id_token(
    id_token: &[u8],
    expected_nonce: &[u8],
) -> Result<VerifiedNativeIdentity, String> {
    let key = resolve_id_token_key(id_token).await?;
    verify_signature_with_key(id_token, &key).map_err(|code| {
        eprintln!("[Recorder][AuthHealth] stage=oidc_id_signature ok=false code={code}");
        "OIDC ID token signature is invalid".to_string()
    })?;

    let claims = inspect_unverified_id_token(id_token, expected_nonce)?;
    let identity = promote_claims(claims);
    eprintln!(
        "[Recorder][AuthHealth] stage=oidc_id_verification ok=true signature_valid=true claims_valid=true identity_native_only=true"
    );
    Ok(identity)
}

fn promote_claims(claims: UnverifiedIdTokenClaims) -> VerifiedNativeIdentity {
    VerifiedNativeIdentity {
        subject: claims.subject(),
        issued_at: claims.issued_at(),
        expires_at: claims.expires_at(),
        authenticated_at: claims.authenticated_at(),
    }
}

fn verify_signature_with_key(
    id_token: &[u8],
    key: &ValidatedJwk,
) -> Result<(), &'static str> {
    let (signing_input, signature_bytes) = split_compact_signature(id_token)?;
    match key {
        ValidatedJwk::Rsa {
            modulus, exponent, ..
        } => {
            if signature_bytes.len() != modulus.len() {
                return Err("rsa_signature_size_invalid");
            }
            signature::RsaPublicKeyComponents {
                n: modulus.as_slice(),
                e: exponent.as_slice(),
            }
            .verify(
                &signature::RSA_PKCS1_2048_8192_SHA256,
                signing_input,
                &signature_bytes,
            )
            .map_err(|_| "signature_mismatch")
        }
        ValidatedJwk::EcP256 { x, y, .. } => {
            if signature_bytes.len() != ES256_SIGNATURE_BYTES {
                return Err("es256_signature_size_invalid");
            }
            let mut public_key = [0_u8; P256_UNCOMPRESSED_PUBLIC_KEY_BYTES];
            public_key[0] = 0x04;
            public_key[1..33].copy_from_slice(x);
            public_key[33..].copy_from_slice(y);
            signature::UnparsedPublicKey::new(
                &signature::ECDSA_P256_SHA256_FIXED,
                &public_key,
            )
            .verify(signing_input, &signature_bytes)
            .map_err(|_| "signature_mismatch")
        }
    }
}

fn split_compact_signature(id_token: &[u8]) -> Result<(&[u8], Vec<u8>), &'static str> {
    if id_token.is_empty() || id_token.len() > MAX_ID_TOKEN_BYTES {
        return Err("token_size_invalid");
    }
    if id_token
        .iter()
        .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err("token_text_invalid");
    }

    let first_dot = id_token
        .iter()
        .position(|byte| *byte == b'.')
        .ok_or("segment_count_invalid")?;
    let second_dot = id_token[first_dot + 1..]
        .iter()
        .position(|byte| *byte == b'.')
        .map(|position| first_dot + 1 + position)
        .ok_or("segment_count_invalid")?;
    if first_dot == 0
        || second_dot == first_dot + 1
        || second_dot + 1 >= id_token.len()
        || id_token[second_dot + 1..].contains(&b'.')
    {
        return Err("segment_count_invalid");
    }

    let encoded_signature = &id_token[second_dot + 1..];
    if encoded_signature.len() > MAX_SIGNATURE_SEGMENT_BYTES
        || encoded_signature.contains(&b'=')
        || !encoded_signature
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-' || *byte == b'_')
    {
        return Err("signature_encoding_invalid");
    }
    let signature = URL_SAFE_NO_PAD
        .decode(encoded_signature)
        .map_err(|_| "signature_encoding_invalid")?;
    if signature.is_empty() || signature.len() > MAX_SIGNATURE_BYTES {
        return Err("signature_size_invalid");
    }
    Ok((&id_token[..second_dot], signature))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSA_SIGNING_INPUT: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6InJzYS10ZXN0IiwidHlwIjoiSldUIn0.eyJzdWIiOiIxMTExMTExMS0xMTExLTQxMTEtODExMS0xMTExMTExMTExMTEifQ";
    const RSA_MODULUS: &str = "0JLkQYVvxq6-XcF7X8YQaIbcWsvZc-lrTq9iR6B4nEoXemrNFCTwTtbVZF5YKjz8fG907n4N3zt2O55JVvkWuiFDMCA8JD_Q9QGbXlZ3Y2eGxrr1fxA0uzQ4OiEGcPkjAMiSR8XxG7fopIpbIOeUro5WjrMJkzQJ9rst8NWJbksbXnEFjsswWrVGaQhwkDWXazmgGKnJFeJGrfkcSGf1NrjyR8sRfomRf-id4t0XzX9SBNjG-1OnKKiUTj24FyHPXMGED6A58kAb1QTp8kVKNyDE-ui0rs-qe4RliYZ6iKVv0Y3DGKvHlDEQh8mjNN9dC813QzV_4_cggfbyloaYMQ";
    const RSA_SIGNATURE: &str = "i-ZkFu2QvWBNRP8u_O0BVx-KiynAbnORB8LBbnRWkoEsCQY6fm4MQFZiIVealwlOgHTqqJjuYuQlo6A1X6_fEi4Ut4pbRnhzhdlnKhXoZ7VLWJnIWLre3VaiGZ1RfTgJQJCGE0ojR2gl3Y0j3S6Krwe087L2gigNNeyiPkd1Q4iLIjzexQZNJquEmSCZhXh6wWux_L4uatAcdL7SL6WGveaoAIE4K221vcRGoqOd8bsDTH0crXoiV0AFDPm9Kmqhxkqaf0wvNSB0ScVtVmLgJGfxy4TyIFb1HVu5yWhPPRKiv6xpiERjFHBxGJYt8n36bMnbnO3fiNmdU5sWvywsmw";

    const ES_SIGNING_INPUT: &str = "eyJhbGciOiJFUzI1NiIsImtpZCI6ImVjLXRlc3QiLCJ0eXAiOiJKV1QifQ.eyJzdWIiOiIxMTExMTExMS0xMTExLTQxMTEtODExMS0xMTExMTExMTExMTEifQ";
    const ES_X: &str = "kt5PQFpkkoXsOwE6zgA79hbZpTgFywqZcnF2FSKjA5Y";
    const ES_Y: &str = "V6Hk-QUKm3lG6ERolavWzbtC3gdQ9jjgeys9Pc8rYiU";
    const ES_SIGNATURE: &str = "GwCP0ZUh2jmC876Yz_eVg6ZEOFXibi-DnaGT3kHEkqgAv702PKEGae89s-QESK5H3pbJsNdAXRWQgfx1f-4WGw";

    fn compact(input: &str, signature: &str) -> Vec<u8> {
        format!("{input}.{signature}").into_bytes()
    }

    #[test]
    fn rs256_signature_vector_is_accepted_and_tampering_is_rejected() {
        let key = ValidatedJwk::Rsa {
            kid: "rsa-test".to_string(),
            modulus: URL_SAFE_NO_PAD.decode(RSA_MODULUS).unwrap(),
            exponent: URL_SAFE_NO_PAD.decode("AQAB").unwrap(),
        };
        let token = compact(RSA_SIGNING_INPUT, RSA_SIGNATURE);
        assert_eq!(verify_signature_with_key(&token, &key), Ok(()));

        let mut tampered = token;
        let payload_offset = tampered.iter().position(|byte| *byte == b'.').unwrap() + 1;
        tampered[payload_offset] = if tampered[payload_offset] == b'e' {
            b'f'
        } else {
            b'e'
        };
        assert_eq!(
            verify_signature_with_key(&tampered, &key),
            Err("signature_mismatch")
        );
    }

    #[test]
    fn es256_fixed_signature_vector_is_accepted_and_wrong_key_is_rejected() {
        let x: [u8; 32] = URL_SAFE_NO_PAD.decode(ES_X).unwrap().try_into().unwrap();
        let y: [u8; 32] = URL_SAFE_NO_PAD.decode(ES_Y).unwrap().try_into().unwrap();
        let key = ValidatedJwk::EcP256 {
            kid: "ec-test".to_string(),
            x,
            y,
        };
        let token = compact(ES_SIGNING_INPUT, ES_SIGNATURE);
        assert_eq!(verify_signature_with_key(&token, &key), Ok(()));

        let wrong_key = ValidatedJwk::EcP256 {
            kid: "ec-test".to_string(),
            x: [0x11; 32],
            y: [0x22; 32],
        };
        assert_eq!(
            verify_signature_with_key(&token, &wrong_key),
            Err("signature_mismatch")
        );
    }

    #[test]
    fn malformed_signature_segments_and_sizes_are_rejected() {
        assert_eq!(
            split_compact_signature(b"header.payload."),
            Err("segment_count_invalid")
        );
        assert_eq!(
            split_compact_signature(b"header.payload.ab=c"),
            Err("signature_encoding_invalid")
        );

        let key = ValidatedJwk::EcP256 {
            kid: "ec-test".to_string(),
            x: URL_SAFE_NO_PAD.decode(ES_X).unwrap().try_into().unwrap(),
            y: URL_SAFE_NO_PAD.decode(ES_Y).unwrap().try_into().unwrap(),
        };
        let short_signature = compact(ES_SIGNING_INPUT, "AQ");
        assert_eq!(
            verify_signature_with_key(&short_signature, &key),
            Err("es256_signature_size_invalid")
        );
    }
}
