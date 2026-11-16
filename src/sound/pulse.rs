//! Linux output: PulseAudio's simple API (PipeWire answers it too), loaded with dlopen on the first
//! sound, so a computer without it still starts and nothing is linked for those who never play one.
//! A write blocks until the server takes the frames: that is the pacing, with no timer.

use super::{Output, CHANNELS, RATE};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::sync::OnceLock;

#[repr(C)]
struct SampleSpec {
    format: c_int,
    rate: u32,
    channels: u8,
}

#[repr(C)]
struct BufferAttr {
    maxlength: u32,
    tlength: u32,
    prebuf: u32,
    minreq: u32,
    fragsize: u32,
}

const FLOAT32LE: c_int = 5;
const PLAYBACK: c_int = 1;
/// What the server keeps ahead: 60 ms, so a stop or a volume change is heard at once.
const AHEAD_MS: u32 = 60;

type New = unsafe extern "C" fn(
    *const c_char,
    *const c_char,
    c_int,
    *const c_char,
    *const c_char,
    *const SampleSpec,
    *const c_void,
    *const BufferAttr,
    *mut c_int,
) -> *mut c_void;
type Write = unsafe extern "C" fn(*mut c_void, *const c_void, usize, *mut c_int) -> c_int;
type Drain = unsafe extern "C" fn(*mut c_void, *mut c_int) -> c_int;
type Free = unsafe extern "C" fn(*mut c_void);
type StrError = unsafe extern "C" fn(c_int) -> *const c_char;

struct Api {
    new: New,
    write: Write,
    drain: Drain,
    free: Free,
    strerror: Option<StrError>,
}

fn api() -> Result<&'static Api, String> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(|| {
        // SAFETY: dlopen and dlsym with NUL-terminated names; each symbol is cast to the C signature
        // pulse/simple.h and pulse/error.h give it. The library stays loaded for the process's life.
        unsafe {
            let lib = libc::dlopen(c"libpulse-simple.so.0".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL);
            if lib.is_null() {
                return None;
            }
            let sym = |name: &CStr| {
                let p = libc::dlsym(lib, name.as_ptr());
                (!p.is_null()).then_some(p)
            };
            Some(Api {
                new: std::mem::transmute::<*mut c_void, New>(sym(c"pa_simple_new")?),
                write: std::mem::transmute::<*mut c_void, Write>(sym(c"pa_simple_write")?),
                drain: std::mem::transmute::<*mut c_void, Drain>(sym(c"pa_simple_drain")?),
                free: std::mem::transmute::<*mut c_void, Free>(sym(c"pa_simple_free")?),
                strerror: sym(c"pa_strerror").map(|p| std::mem::transmute::<*mut c_void, StrError>(p)),
            })
        }
    })
    .as_ref()
    .ok_or_else(|| "sounds need PulseAudio or PipeWire (libpulse-simple.so.0 was not found)".to_string())
}

fn said(api: &Api, error: c_int) -> String {
    match api.strerror {
        // SAFETY: pa_strerror returns a static NUL-terminated string (or null).
        Some(f) => unsafe {
            let p = f(error);
            if p.is_null() {
                format!("error {error}")
            } else {
                CStr::from_ptr(p).to_string_lossy().into_owned()
            }
        },
        None => format!("error {error}"),
    }
}

pub struct Pulse {
    api: &'static Api,
    stream: *mut c_void,
}

pub fn open(device: &str) -> Result<Box<dyn Output>, String> {
    let api = api()?;
    let spec = SampleSpec {
        format: FLOAT32LE,
        rate: RATE,
        channels: CHANNELS as u8,
    };
    let ahead = RATE * CHANNELS as u32 * 4 * AHEAD_MS / 1000;
    let attr = BufferAttr {
        maxlength: u32::MAX,
        tlength: ahead,
        prebuf: u32::MAX,
        minreq: u32::MAX,
        fragsize: u32::MAX,
    };
    let device = (!device.is_empty())
        .then(|| CString::new(device).map_err(|_| "a device name can't hold a NUL".to_string()))
        .transpose()?;
    let mut error = 0;
    // SAFETY: every pointer is valid for the call (or null where the API allows it).
    let stream = unsafe {
        (api.new)(
            std::ptr::null(),
            c"Streamerz".as_ptr(),
            PLAYBACK,
            device.as_ref().map_or(std::ptr::null(), |d| d.as_ptr()),
            c"Sounds".as_ptr(),
            &spec,
            std::ptr::null(),
            &attr,
            &mut error,
        )
    };
    if stream.is_null() {
        return Err(format!("cannot open the sound output: {}", said(api, error)));
    }
    Ok(Box::new(Pulse { api, stream }))
}

impl Output for Pulse {
    fn write(&mut self, frames: &[f32]) -> Result<(), String> {
        let mut error = 0;
        // SAFETY: the stream is open; the slice is valid for its length in bytes.
        let r = unsafe {
            (self.api.write)(
                self.stream,
                frames.as_ptr().cast(),
                std::mem::size_of_val(frames),
                &mut error,
            )
        };
        if r < 0 {
            return Err(said(self.api, error));
        }
        Ok(())
    }

    fn drain(&mut self) {
        let mut error = 0;
        // SAFETY: the stream is open.
        unsafe { (self.api.drain)(self.stream, &mut error) };
    }
}

impl Drop for Pulse {
    fn drop(&mut self) {
        // SAFETY: the stream is open and freed once.
        unsafe { (self.api.free)(self.stream) };
    }
}
