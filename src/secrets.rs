//! Secrets (OAuth tokens, cookies, client secrets, passwords) never sit in streamerz.json as plain
//! text: they're taken out of the config on save and written back as one encrypted `secrets` value.
//! - Windows: DPAPI (only this Windows user can decrypt it).
//! - Linux: a random key kept in the Secret Service (GNOME Keyring, KWallet, KeePassXC); the value
//!   is HMAC-SHA256 in counter mode, encrypt-then-MAC. Without a keyring, new secrets stay in the
//!   file, which is only readable by its owner (0600).
//!
//! When the key can't be read right away (transient D-Bus hiccup or locked keyring), streamerz
//! retries transient D-Bus errors immediately, keeps the encrypted value intact if still locked,
//! and automatically unlocks and reconnects once the keyring becomes available.

use crate::model::Obj;
use base64::Engine as _;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Top-level parts of the config that are the user's own data, never searched for secrets.
const USER_DATA: &[&str] = &[
    "actions",
    "globals",
    "userGlobals",
    "globalWrites",
    "commands",
    "timers",
    "groups",
    "counters",
];

/// Whether a config key holds a secret.
pub fn is_secret(k: &str) -> bool {
    let l = k.to_ascii_lowercase();
    l.ends_with("token") || l.ends_with("secret") || l.ends_with("password") || l.ends_with("cookie")
}

/// Take the non-empty secrets out of `v` (left as ""), by path ("kick/bot/token/accessToken",
/// "obs/connections/0/password").
pub fn split(v: &mut Value) -> Obj {
    fn walk(v: &mut Value, path: &str, out: &mut Obj) {
        match v {
            Value::Object(o) => {
                for (k, x) in o.iter_mut() {
                    if path.is_empty() && USER_DATA.contains(&k.as_str()) {
                        continue;
                    }
                    let p = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}/{k}")
                    };
                    match x {
                        Value::String(s) if is_secret(k) && !s.is_empty() => {
                            out.insert(p, Value::String(std::mem::take(s)));
                        }
                        _ => walk(x, &p, out),
                    }
                }
            }
            Value::Array(a) => {
                for (i, x) in a.iter_mut().enumerate() {
                    walk(x, &format!("{path}/{i}"), out);
                }
            }
            _ => {}
        }
    }
    let mut out = Obj::new();
    walk(v, "", &mut out);
    out
}

/// Put secrets back where `split` took them, unless the file has a newer one there (a login made
/// while the key couldn't be read).
pub fn merge(v: &mut Value, secrets: &Obj) {
    for (p, s) in secrets {
        if let Some(slot) = v.pointer_mut(&format!("/{p}")) {
            if slot.as_str().is_some_and(str::is_empty) {
                *slot = s.clone();
            }
        }
    }
}

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

/// Random bytes from the OS.
pub fn random(n: usize) -> Vec<u8> {
    let mut b = vec![0u8; n];
    #[cfg(windows)]
    {
        use windows_sys::Win32::Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG};
        // SAFETY: the buffer is n bytes long.
        unsafe {
            BCryptGenRandom(
                std::ptr::null_mut(),
                b.as_mut_ptr(),
                n as u32,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            )
        };
    }
    #[cfg(not(windows))]
    {
        use std::io::Read;
        if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
            let _ = f.read_exact(&mut b);
        }
    }
    b
}

/// The encrypted value read at start, kept as is while it can't be decrypted.
static KEPT: Mutex<Option<String>> = Mutex::new(None);
/// Why `KEPT` is currently locked, if it is.
static LOCKED: Mutex<Option<String>> = Mutex::new(None);
static WARNED_NO_KEYRING: AtomicBool = AtomicBool::new(false);

/// Why saved logins are currently locked (keyring locked or unreachable), or `None` when unlocked.
pub fn locked_reason() -> Option<String> {
    LOCKED.lock().ok().and_then(|l| l.clone())
}

/// Whether saved logins are waiting on the keyring to unlock.
pub fn is_locked() -> bool {
    locked_reason().is_some()
}

/// Before saving: move the secrets of `v` into its `secrets` value, if they can be protected.
pub fn seal(v: &mut Value) {
    let Value::Object(_) = v else { return };
    let mut secrets = split(v);
    if let Some(old) = KEPT.lock().ok().and_then(|k| k.clone()) {
        // Undecryptable right now: keep the encrypted blob intact, and any new login stays in 0600 file until merged.
        if let Some(o) = v.as_object_mut() {
            o.insert("secrets".into(), old.into());
        }
        restore(v, &secrets);
        return;
    }
    if secrets.is_empty() {
        return;
    }
    let plain = serde_json::to_vec(&secrets).unwrap_or_default();
    match protect(&plain) {
        Ok(blob) => {
            if let Some(o) = v.as_object_mut() {
                o.insert("secrets".into(), blob.into());
            }
        }
        Err(e) => {
            if !WARNED_NO_KEYRING.swap(true, Ordering::Relaxed) {
                crate::say(format!(
                    "Keyring unavailable ({e}): keeping new logins in streamerz.json (0600)"
                ));
            }
            restore(v, &std::mem::take(&mut secrets));
        }
    }
}

fn restore(v: &mut Value, secrets: &Obj) {
    for (p, s) in secrets {
        if let Some(slot) = v.pointer_mut(&format!("/{p}")) {
            *slot = s.clone();
        }
    }
}

/// After reading: put the secrets back from the `secrets` value, retrying transient keyring errors.
pub fn open(v: &mut Value) {
    let Some(blob) = v.as_object_mut().and_then(|o| o.remove("secrets")) else {
        return;
    };
    let Some(blob) = blob.as_str().map(String::from) else {
        return;
    };
    match unprotect(&blob, true, 3).and_then(|p| serde_json::from_slice::<Obj>(&p).map_err(|e| e.to_string())) {
        Ok(secrets) => {
            if let Ok(mut k) = KEPT.lock() {
                *k = None;
            }
            if let Ok(mut l) = LOCKED.lock() {
                *l = None;
            }
            merge(v, &secrets);
        }
        Err(why) => {
            crate::say(format!(
                "Keyring: saved logins are locked ({why}); keeping them safe and retrying automatically"
            ));
            if let Ok(mut k) = KEPT.lock() {
                *k = Some(blob);
            }
            if let Ok(mut l) = LOCKED.lock() {
                *l = Some(why);
            }
        }
    }
}

/// Try to unlock previously undecrypted `KEPT` secrets and merge them into `v`.
/// When `interactive` is false (background polling), never opens a password prompt dialog.
pub fn retry_unlock(v: &mut Value, interactive: bool) -> Result<bool, String> {
    let Some(blob) = KEPT.lock().ok().and_then(|k| k.clone()) else {
        if let Ok(mut l) = LOCKED.lock() {
            *l = None;
        }
        return Ok(false);
    };
    let retries = if interactive { 2 } else { 0 };
    match unprotect(&blob, interactive, retries)
        .and_then(|p| serde_json::from_slice::<Obj>(&p).map_err(|e| e.to_string()))
    {
        Ok(secrets) => {
            if let Ok(mut k) = KEPT.lock() {
                *k = None;
            }
            if let Ok(mut l) = LOCKED.lock() {
                *l = None;
            }
            merge(v, &secrets);
            crate::say("Keyring: saved logins unlocked and restored".into());
            Ok(true)
        }
        Err(why) => {
            if interactive {
                if let Ok(mut l) = LOCKED.lock() {
                    *l = Some(why.clone());
                }
            }
            Err(why)
        }
    }
}

#[cfg(windows)]
fn dpapi(data: &[u8], protect: bool) -> Option<Vec<u8>> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB};
    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut out = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };

    // SAFETY: input points to `data`; on success Windows allocates out, freed below with LocalFree.
    let ok = unsafe {
        if protect {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                &mut out,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                &mut out,
            )
        }
    };
    if ok == 0 || out.pbData.is_null() {
        return None;
    }
    // SAFETY: out is cbData bytes allocated by Windows.
    let v = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
    unsafe { LocalFree(out.pbData.cast()) };
    Some(v)
}

#[cfg(windows)]
fn protect(plain: &[u8]) -> Result<String, String> {
    dpapi(plain, true)
        .map(|c| format!("dpapi:{}", b64().encode(c)))
        .ok_or_else(|| "DPAPI protect failed".into())
}

#[cfg(windows)]
fn unprotect(blob: &str, _interactive: bool, _retries: usize) -> Result<Vec<u8>, String> {
    let raw = blob
        .strip_prefix("dpapi:")
        .ok_or_else(|| "not a DPAPI secret".to_string())?;
    let bytes = b64().decode(raw).map_err(|e| e.to_string())?;
    dpapi(&bytes, false).ok_or_else(|| "DPAPI unprotect failed".into())
}

/// Cached 32-byte key from the Secret Service. Only populated on success so a transient D-Bus
/// error never poisons the process for its whole lifetime.
#[cfg(not(windows))]
static KEY: std::sync::OnceLock<[u8; 32]> = std::sync::OnceLock::new();

/// The config file the key belongs to (its keyring attribute).
static CONFIG: std::sync::OnceLock<String> = std::sync::OnceLock::new();

pub fn init(config: &std::path::Path) {
    let _ = CONFIG.set(config.display().to_string());
}

/// Whether a D-Bus / keyring error is a transient daemon wake-up or disconnect hiccup worth retrying.
#[cfg(not(windows))]
fn is_transient(err: &str) -> bool {
    const TRANSIENT: &[&str] = &[
        "NoReply",
        "Remote peer disconnected",
        "ServiceUnknown",
        "NameHasNoOwner",
        "TimedOut",
        "ActivateServiceByName",
        "Connection reset",
        "Broken pipe",
        "I/O error",
        "EOF",
    ];
    TRANSIENT.iter().any(|m| err.contains(m))
}

/// The keyring call runs on its own thread with its own small runtime: this may be called from the
/// window or from inside the engine's runtime, and an unlock prompt may wait for the user.
#[cfg(not(windows))]
fn key(interactive: bool, retries: usize) -> Result<[u8; 32], String> {
    if let Some(k) = KEY.get() {
        return Ok(*k);
    }
    let path = CONFIG
        .get()
        .cloned()
        .ok_or_else(|| "config path not initialized".to_string())?;
    let got = std::thread::spawn(move || -> Result<[u8; 32], String> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        rt.block_on(async move {
            let wait = std::time::Duration::from_secs(if interactive { 120 } else { 8 });
            let delays = [250u64, 500, 1000, 1500];
            let mut last = String::new();
            for attempt in 0..=retries {
                if attempt > 0 {
                    let ms = delays[(attempt - 1).min(delays.len() - 1)];
                    tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                }
                match tokio::time::timeout(wait, keyring::key(&path, interactive)).await {
                    Ok(Ok(k)) => return Ok(k),
                    Ok(Err(e)) => {
                        last = e;
                        if !is_transient(&last) {
                            break;
                        }
                    }
                    Err(_) => {
                        last = "the keyring timed out".into();
                        if !interactive {
                            break;
                        }
                    }
                }
            }
            Err(last)
        })
    })
    .join()
    .map_err(|_| "keyring thread panicked".to_string())?;

    if let Ok(k) = got {
        let _ = KEY.set(k);
    }
    got
}

#[cfg(not(windows))]
fn hmac(key: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut k = [0u8; 64];
    k[..key.len().min(64)].copy_from_slice(&key[..key.len().min(64)]);
    let pad = |x: u8| k.iter().map(|b| b ^ x).collect::<Vec<u8>>();
    let mut inner = Sha256::new();
    inner.update(pad(0x36));
    for p in parts {
        inner.update(p);
    }
    let mut outer = Sha256::new();
    outer.update(pad(0x5c));
    outer.update(inner.finalize());
    outer.finalize().into()
}

/// Encrypt-then-MAC: keystream HMAC(ke, nonce || counter), tag HMAC(km, nonce || ciphertext).
#[cfg(not(windows))]
fn seal_with(key: &[u8; 32], nonce: &[u8], plain: &[u8]) -> Vec<u8> {
    let (ke, km) = (hmac(key, &[b"enc"]), hmac(key, &[b"mac"]));
    let mut out = nonce.to_vec();
    for (i, chunk) in plain.chunks(32).enumerate() {
        let ks = hmac(&ke, &[nonce, &(i as u64).to_be_bytes()]);
        out.extend(chunk.iter().zip(ks).map(|(a, b)| a ^ b));
    }
    let tag = hmac(&km, &[&out]);
    out.extend(tag);
    out
}

#[cfg(not(windows))]
fn open_with(key: &[u8; 32], sealed: &[u8]) -> Option<Vec<u8>> {
    if sealed.len() < 48 {
        return None;
    }
    let (body, tag) = sealed.split_at(sealed.len() - 32);
    let (ke, km) = (hmac(key, &[b"enc"]), hmac(key, &[b"mac"]));
    // Constant time: a timing difference would tell how much of the tag matched.
    if hmac(&km, &[body]).iter().zip(tag).fold(0, |d, (a, b)| d | (a ^ b)) != 0 {
        return None;
    }
    let (nonce, cipher) = body.split_at(16);
    let mut out = Vec::with_capacity(cipher.len());
    for (i, chunk) in cipher.chunks(32).enumerate() {
        let ks = hmac(&ke, &[nonce, &(i as u64).to_be_bytes()]);
        out.extend(chunk.iter().zip(ks).map(|(a, b)| a ^ b));
    }
    Some(out)
}

#[cfg(not(windows))]
fn protect(plain: &[u8]) -> Result<String, String> {
    let k = key(true, 2)?;
    Ok(format!("keyring:{}", b64().encode(seal_with(&k, &random(16), plain))))
}

#[cfg(not(windows))]
fn unprotect(blob: &str, interactive: bool, retries: usize) -> Result<Vec<u8>, String> {
    let raw = blob
        .strip_prefix("keyring:")
        .ok_or_else(|| "unknown secret format".to_string())?;
    let bytes = b64().decode(raw).map_err(|e| e.to_string())?;
    let k = key(interactive, retries)?;
    open_with(&k, &bytes).ok_or_else(|| "keyring key mismatch".into())
}

/// The freedesktop Secret Service over D-Bus (zbus, which the tray already uses).
#[cfg(not(windows))]
mod keyring {
    use std::collections::HashMap;
    use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
    use zbus::{Connection, Proxy};

    const DEST: &str = "org.freedesktop.secrets";

    async fn proxy<'a>(c: &'a Connection, path: &'a str, iface: &'a str) -> zbus::Result<Proxy<'a>> {
        Proxy::new(c, DEST, path.to_string(), iface).await
    }

    /// Run a prompt ("/" is none) and wait for it: unlocking may ask for the keyring password.
    async fn prompt(c: &Connection, path: &OwnedObjectPath, interactive: bool) -> zbus::Result<()> {
        use futures_util::StreamExt;
        if path.as_str() == "/" {
            return Ok(());
        }
        if !interactive {
            return Err(zbus::Error::Failure("keyring is locked".into()));
        }
        let p = proxy(c, path.as_str(), "org.freedesktop.Secret.Prompt").await?;
        let mut done = p.receive_signal("Completed").await?;
        p.call_method("Prompt", &("",)).await?;
        let m = done
            .next()
            .await
            .ok_or_else(|| zbus::Error::Failure("prompt gone".into()))?;
        let (dismissed, _): (bool, OwnedValue) = m.body().deserialize()?;
        if dismissed {
            return Err(zbus::Error::Failure("keyring unlock dismissed".into()));
        }
        Ok(())
    }

    async fn unlock(
        c: &Connection,
        svc: &Proxy<'_>,
        paths: Vec<OwnedObjectPath>,
        interactive: bool,
    ) -> zbus::Result<()> {
        let (_, p): (Vec<OwnedObjectPath>, OwnedObjectPath) = svc.call("Unlock", &(paths,)).await?;
        prompt(c, &p, interactive).await
    }

    /// streamerz's key for this config file: read, or made and stored.
    pub async fn key(config: &str, interactive: bool) -> Result<[u8; 32], String> {
        let e = |x: zbus::Error| x.to_string();
        let c = Connection::session().await.map_err(e)?;
        let svc = proxy(&c, "/org/freedesktop/secrets", "org.freedesktop.Secret.Service")
            .await
            .map_err(e)?;
        let (_, session): (OwnedValue, OwnedObjectPath) =
            svc.call("OpenSession", &("plain", Value::from(""))).await.map_err(e)?;
        let attrs = HashMap::from([("application", "streamerz"), ("config", config)]);
        let (mut found, locked): (Vec<OwnedObjectPath>, Vec<OwnedObjectPath>) =
            svc.call("SearchItems", &(&attrs,)).await.map_err(e)?;
        if found.is_empty() && !locked.is_empty() {
            unlock(&c, &svc, locked.clone(), interactive).await.map_err(e)?;
            found = locked;
        }
        let parse = |v: &[u8]| -> Option<[u8; 32]> {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD
                .decode(v)
                .ok()?
                .try_into()
                .ok()
        };
        if let Some(item) = found.first() {
            let item = proxy(&c, item.as_str(), "org.freedesktop.Secret.Item")
                .await
                .map_err(e)?;
            let (_, _, value, _): (OwnedObjectPath, Vec<u8>, Vec<u8>, String) =
                item.call("GetSecret", &(&session,)).await.map_err(e)?;
            return parse(&value).ok_or_else(|| "the keyring's streamerz key is damaged".into());
        }
        if !interactive {
            return Err("keyring item not available yet".into());
        }
        // None yet: make one in the default collection.
        let key: [u8; 32] = super::random(32).try_into().map_err(|_| "no random".to_string())?;
        let mut coll: OwnedObjectPath = svc.call("ReadAlias", &("default",)).await.map_err(e)?;
        if coll.as_str() == "/" {
            coll = OwnedObjectPath::try_from("/org/freedesktop/secrets/collection/login").map_err(|x| x.to_string())?;
        }
        unlock(&c, &svc, vec![coll.clone()], interactive).await.map_err(e)?;
        let col = proxy(&c, coll.as_str(), "org.freedesktop.Secret.Collection")
            .await
            .map_err(e)?;
        let props: HashMap<&str, Value> = HashMap::from([
            ("org.freedesktop.Secret.Item.Label", Value::from("streamerz logins")),
            ("org.freedesktop.Secret.Item.Attributes", Value::from(attrs.clone())),
        ]);
        use base64::Engine as _;
        let value = base64::engine::general_purpose::STANDARD.encode(key).into_bytes();
        let secret = (&session, Vec::<u8>::new(), value, "text/plain");
        let (_, p): (OwnedObjectPath, OwnedObjectPath) =
            col.call("CreateItem", &(props, secret, true)).await.map_err(e)?;
        prompt(&c, &p, interactive).await.map_err(e)?;
        Ok(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn split_and_merge() {
        let mut v = json!({
            "kick": {"clientSecret": "s", "bot": {"token": {"accessToken": "a", "refreshToken": "", "expiresAt": 5}}},
            "obs": {"connections": [{"name": "x", "password": "p"}]},
            "tiktok": {"broadcaster": {"sessionCookie": "c"}},
            "twitch": {"broadcaster": {"token": {"tokenType": "bearer"}}},
            "globals": {"myToken": "user data"}
        });
        let orig = v.clone();
        let s = split(&mut v);
        assert_eq!(s.len(), 4, "{s:?}");
        assert_eq!(s["obs/connections/0/password"], "p");
        assert_eq!(v["kick"]["bot"]["token"]["accessToken"], "");
        assert_eq!(v["globals"]["myToken"], "user data");
        merge(&mut v, &s);
        assert_eq!(v, orig);
        // A newer secret in the file wins.
        let mut w = orig.clone();
        w["kick"]["clientSecret"] = "new".into();
        merge(&mut w, &s);
        assert_eq!(w["kick"]["clientSecret"], "new");
    }

    #[cfg(not(windows))]
    #[test]
    fn cipher_and_transient_errors() {
        let k = [7u8; 32];
        let plain = b"{\"a\":\"some secret longer than one 32 byte block\"}";
        let mut sealed = seal_with(&k, &random(16), plain);
        assert_eq!(open_with(&k, &sealed).unwrap(), plain);
        assert!(open_with(&[8u8; 32], &sealed).is_none());
        sealed[20] ^= 1;
        assert!(open_with(&k, &sealed).is_none());
        assert_ne!(random(16), random(16));
        assert!(is_transient(
            "org.freedesktop.DBus.Error.NoReply: Remote peer disconnected"
        ));
        assert!(is_transient(
            "org.freedesktop.DBus.Error.ServiceUnknown: The name was not provided"
        ));
        assert!(!is_transient("keyring unlock dismissed"));
    }
}
