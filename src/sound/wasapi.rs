//! Windows output: WASAPI in shared mode, told when it wants more (an event), so the thread waits on
//! the system and not on a timer. Windows converts the mix (48 kHz stereo float) to the device's own
//! format.

use super::{Output, CHANNELS, RATE};
use windows::core::PCWSTR;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioClient, IAudioRenderClient, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
    AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, DEVICE_STATE_ACTIVE, WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
    WAVEFORMATEXTENSIBLE_0,
};
use windows::Win32::Media::KernelStreaming::WAVE_FORMAT_EXTENSIBLE;
use windows::Win32::Media::Multimedia::KSDATAFORMAT_SUBTYPE_IEEE_FLOAT;
use windows::Win32::System::Com::StructuredStorage::PropVariantClear;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::Win32::System::Variant::VT_LPWSTR;

/// What the device keeps ahead: 50 ms, in units of 100 ns.
const AHEAD: i64 = 500_000;

pub struct Wasapi {
    client: IAudioClient,
    render: IAudioRenderClient,
    event: HANDLE,
    size: u32,
}

fn name_of(device: &IMMDevice) -> Option<String> {
    // SAFETY: the store is read-only; the value is cleared after it is read.
    unsafe {
        let store = device.OpenPropertyStore(STGM_READ).ok()?;
        let mut value = store.GetValue(&PKEY_Device_FriendlyName).ok()?;
        let inner = &value.Anonymous.Anonymous;
        let name = (inner.vt == VT_LPWSTR)
            .then(|| inner.Anonymous.pwszVal.to_string().ok())
            .flatten();
        let _ = PropVariantClear(&mut value);
        name
    }
}

fn enumerator() -> Result<IMMDeviceEnumerator, String> {
    // SAFETY: COM is started for this thread (the device thread), then the enumerator made.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| format!("no audio devices: {e}"))
    }
}

/// The names of the output devices that are on.
pub fn devices() -> Vec<String> {
    let Ok(en) = enumerator() else {
        return Vec::new();
    };
    // SAFETY: plain COM calls on live interfaces.
    unsafe {
        let Ok(all) = en.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) else {
            return Vec::new();
        };
        (0..all.GetCount().unwrap_or(0))
            .filter_map(|i| all.Item(i).ok())
            .filter_map(|d| name_of(&d))
            .collect()
    }
}

fn find(en: &IMMDeviceEnumerator, name: &str) -> Result<IMMDevice, String> {
    // SAFETY: plain COM calls on live interfaces.
    unsafe {
        if name.is_empty() {
            return en
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|e| format!("no default sound output: {e}"));
        }
        let all = en
            .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
            .map_err(|e| e.to_string())?;
        for i in 0..all.GetCount().unwrap_or(0) {
            if let Ok(d) = all.Item(i) {
                if name_of(&d).is_some_and(|n| n.eq_ignore_ascii_case(name)) {
                    return Ok(d);
                }
            }
        }
    }
    Err(format!("the sound output {name} is not connected"))
}

pub fn open(device: &str) -> Result<Box<dyn Output>, String> {
    let en = enumerator()?;
    let device = find(&en, device)?;
    let block = (CHANNELS * 4) as u16;
    let format = WAVEFORMATEXTENSIBLE {
        Format: WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_EXTENSIBLE as u16,
            nChannels: CHANNELS as u16,
            nSamplesPerSec: RATE,
            nAvgBytesPerSec: RATE * block as u32,
            nBlockAlign: block,
            wBitsPerSample: 32,
            cbSize: (std::mem::size_of::<WAVEFORMATEXTENSIBLE>() - std::mem::size_of::<WAVEFORMATEX>()) as u16,
        },
        Samples: WAVEFORMATEXTENSIBLE_0 {
            wValidBitsPerSample: 32,
        },
        // Front left and front right.
        dwChannelMask: 0x3,
        SubFormat: KSDATAFORMAT_SUBTYPE_IEEE_FLOAT,
    };
    // SAFETY: the format lives through Initialize; the event is closed by Drop.
    unsafe {
        let client: IAudioClient = device.Activate(CLSCTX_ALL, None).map_err(|e| e.to_string())?;
        client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                    | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                    | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                AHEAD,
                0,
                &format.Format,
                None,
            )
            .map_err(|e| format!("cannot open the sound output: {e}"))?;
        let event = CreateEventW(None, false, false, PCWSTR::null()).map_err(|e| e.to_string())?;
        let opened = (|| {
            client.SetEventHandle(event)?;
            let size = client.GetBufferSize()?;
            let render: IAudioRenderClient = client.GetService()?;
            client.Start()?;
            Ok::<_, windows::core::Error>((size, render))
        })();
        match opened {
            Ok((size, render)) => Ok(Box::new(Wasapi {
                client,
                render,
                event,
                size,
            })),
            Err(e) => {
                let _ = CloseHandle(event);
                Err(format!("cannot start the sound output: {e}"))
            }
        }
    }
}

impl Wasapi {
    /// Wait until the device wants more; false when it stopped answering.
    fn wait(&self) -> bool {
        // SAFETY: the event is open.
        unsafe { WaitForSingleObject(self.event, 2000) == WAIT_OBJECT_0 }
    }
}

impl Output for Wasapi {
    fn write(&mut self, frames: &[f32]) -> Result<(), String> {
        let mut rest = frames;
        while !rest.is_empty() {
            // SAFETY: the buffer handed out holds `n` frames, written and released at once.
            unsafe {
                let padding = self.client.GetCurrentPadding().map_err(|e| e.to_string())?;
                let room = self.size.saturating_sub(padding) as usize;
                if room == 0 {
                    if !self.wait() {
                        return Err("the sound output stopped".into());
                    }
                    continue;
                }
                let n = room.min(rest.len() / CHANNELS);
                let at = self.render.GetBuffer(n as u32).map_err(|e| e.to_string())?;
                std::ptr::copy_nonoverlapping(rest.as_ptr(), at.cast::<f32>(), n * CHANNELS);
                self.render.ReleaseBuffer(n as u32, 0).map_err(|e| e.to_string())?;
                rest = &rest[n * CHANNELS..];
            }
        }
        Ok(())
    }

    fn drain(&mut self) {
        // SAFETY: plain calls on the open client.
        unsafe {
            while self.client.GetCurrentPadding().is_ok_and(|p| p > 0) {
                if !self.wait() {
                    break;
                }
            }
        }
    }
}

impl Drop for Wasapi {
    fn drop(&mut self) {
        // SAFETY: the client and the event are open and closed once.
        unsafe {
            let _ = self.client.Stop();
            let _ = CloseHandle(self.event);
        }
    }
}
