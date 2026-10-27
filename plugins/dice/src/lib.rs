//! An example streamerz plugin: a "Roll dice" sub-action, a `!roll` answer in chat, a "Rolled a six"
//! trigger, and a setting for the number of sides. Build it with
//! `cargo build --release --target wasm32-unknown-unknown` and copy
//! `target/wasm32-unknown-unknown/release/dice.wasm` into streamerz's `plugins` folder.

use serde_json::{json, Value};

#[link(wasm_import_module = "streamerz")]
extern "C" {
    fn call(ptr: *const u8, len: usize) -> u64;
}

thread_local! {
    /// The last answer handed to streamerz: kept until the next one (it reads it before then).
    static LAST: std::cell::RefCell<Vec<u8>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Hands streamerz JSON it reads from our memory: ptr << 32 | len.
fn out(v: &Value) -> u64 {
    LAST.with_borrow_mut(|b| {
        *b = v.to_string().into_bytes();
        ((b.as_ptr() as u64) << 32) | b.len() as u64
    })
}

/// JSON streamerz wrote at `packed`.
fn read(packed: u64) -> Value {
    if packed == 0 {
        return Value::Null;
    }
    let (ptr, len) = ((packed >> 32) as usize, (packed & 0xffff_ffff) as usize);
    // SAFETY: streamerz wrote `len` bytes there, into memory it got from `alloc`.
    let b = unsafe { Vec::from_raw_parts(ptr as *mut u8, len, len) };
    serde_json::from_slice(&b).unwrap_or(Value::Null)
}

/// A request to streamerz, and its answer (only the ones read at once have one).
fn ask(v: Value) -> Value {
    let s = v.to_string();
    // SAFETY: s lives through the call.
    read(unsafe { call(s.as_ptr(), s.len()) })
}

#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len);
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

#[no_mangle]
pub extern "C" fn manifest() -> u64 {
    out(&json!({
        "name": "Dice",
        "version": "1.0.0",
        "about": "Rolls dice: the Roll dice sub-action, and !roll in Twitch chat.",
        "settings": [{"key": "sides", "label": "Sides", "kind": "number", "default": 6}],
        "subactions": [{"name": "Roll dice", "about": "Sets %roll% to a random number from 1 to the sides.",
            "fields": [{"key": "count", "label": "How many dice", "kind": "number", "default": 1}]}],
        "triggers": [{"name": "Rolled a six", "about": "Someone rolled the highest number."}],
        "events": ["TwitchChatMessage"]
    }))
}

/// A number from 1 to `sides`, from the clock (xorshift).
fn roll(sides: u64) -> u64 {
    let now = ask(json!({"op": "now"}))["unix"].as_f64().unwrap_or(0.0);
    let mut x = (now * 1e6) as u64 | 1;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    1 + x % sides.max(1)
}

fn sides() -> u64 {
    ask(json!({"op": "settings"}))["sides"].as_u64().unwrap_or(6)
}

#[no_mangle]
pub extern "C" fn on_event(ptr: *mut u8, len: usize) -> u64 {
    // SAFETY: streamerz wrote `len` bytes at `ptr`, which it got from `alloc`.
    let ev: Value = serde_json::from_slice(&unsafe { Vec::from_raw_parts(ptr, len, len) }).unwrap_or(Value::Null);
    let sides = sides();
    match ev["kind"].as_str().unwrap_or("") {
        "start" => {
            ask(json!({"op": "log", "text": format!("ready, {sides} sides")}));
            0
        }
        "subaction" => {
            let n = ev["fields"]["count"].as_u64().unwrap_or(1).max(1);
            let rolls: Vec<u64> = (0..n).map(|_| roll(sides)).collect();
            if rolls.contains(&sides) {
                ask(json!({"op": "trigger", "event": "Rolled a six", "args": {"roll": sides}}));
            }
            out(&json!({"args": {"roll": rolls.iter().sum::<u64>(), "rolls": format!("{rolls:?}")}}))
        }
        "trigger" if ev["args"]["message"].as_str().is_some_and(|m| m.trim() == "!roll") => {
            let user = ev["args"]["user"].as_str().unwrap_or("someone");
            let r = roll(sides);
            ask(json!({"op": "say", "platform": "twitch", "text": format!("{user} rolled {r}")}));
            0
        }
        _ => 0,
    }
}
