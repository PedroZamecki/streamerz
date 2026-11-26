//! Portable action group icons, addressed by the SHA-256 of their image bytes.
use base64::Engine as _;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const MAX_BYTES: usize = 1024 * 1024;

pub fn encode(bytes: &[u8]) -> Result<Value, &'static str> {
    if bytes.len() > MAX_BYTES {
        return Err("Image is larger than 1 MB");
    }
    let kind = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "jpeg"
    } else {
        return Err("Choose a PNG or JPEG image");
    };
    Ok(
        json!({"sha256": crate::fmt::hex(&Sha256::digest(bytes)), "format": kind,
        "data": base64::engine::general_purpose::STANDARD.encode(bytes)}),
    )
}

pub fn decode(value: &Value) -> Option<Vec<u8>> {
    let data = value.get("data")?.as_str()?;
    if data.len() > MAX_BYTES.div_ceil(3) * 4 {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD.decode(data).ok()?;
    let expected = encode(&bytes).ok()?;
    (expected == *value).then_some(bytes)
}

#[cfg(test)]
mod tests {
    #[test]
    fn verifies_content_and_digest() {
        let icon = super::encode(b"\x89PNG\r\n\x1a\ncontent").unwrap();
        assert_eq!(super::decode(&icon).as_deref(), Some(&b"\x89PNG\r\n\x1a\ncontent"[..]));
        let mut tampered = icon.clone();
        tampered["sha256"] = "wrong".into();
        assert!(super::decode(&tampered).is_none());
        let mut wrong_fmt = icon.clone();
        wrong_fmt["format"] = "jpeg".into();
        assert!(super::decode(&wrong_fmt).is_none());
        let mut extra_field = icon;
        extra_field["extra"] = true.into();
        assert!(super::decode(&extra_field).is_none());
        assert!(super::encode(b"not an image").is_err());

        let jpg = super::encode(b"\xff\xd8\xff\xe0jpeg").unwrap();
        assert_eq!(jpg["format"], "jpeg");
        assert_eq!(super::decode(&jpg).as_deref(), Some(&b"\xff\xd8\xff\xe0jpeg"[..]));

        let mut too_big = b"\x89PNG\r\n\x1a\n".to_vec();
        too_big.resize(super::MAX_BYTES + 1, 0);
        assert!(super::encode(&too_big).is_err());
        let huge_b64 = serde_json::json!({"sha256": "x", "format": "png", "data": "A".repeat(super::MAX_BYTES * 2)});
        assert!(super::decode(&huge_b64).is_none());
    }
}
