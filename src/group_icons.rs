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
        json!({"sha256": Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect::<String>(), "format": kind,
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
        assert!(super::decode(&icon).is_some());
        let mut tampered = icon;
        tampered["sha256"] = "wrong".into();
        assert!(super::decode(&tampered).is_none());
        assert!(super::encode(b"not an image").is_err());
    }
}
