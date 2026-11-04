//! Speech to Text: Windows' own recognizer (SAPI, in-process, the default microphone), listening on its
//! own thread only while an action uses Speech to Text Dictation or Command (`engine/speech.rs`). Linux
//! has no system recognizer: there it says so and nothing listens. Built from SAPI's documentation; not
//! tried on a real Windows yet (Wine has no recognizer).

/// What was heard: the text and how sure the recognizer was (0 to 1).
#[derive(Clone, Debug, PartialEq)]
pub struct Heard {
    pub text: String,
    pub confidence: f64,
}

/// Stops the listening thread when dropped.
pub struct Listening(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl Drop for Listening {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Start listening; each phrase heard goes to `tx`.
pub fn listen(tx: tokio::sync::mpsc::UnboundedSender<Heard>) -> Result<Listening, String> {
    imp::listen(tx)
}

#[cfg(windows)]
mod imp {
    use super::{Heard, Listening};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use windows::core::{Interface, PWSTR};
    use windows::Win32::Media::Speech::{
        ISpAudio, ISpPhrase, ISpRecoContext, ISpRecoResult, ISpRecognizer, SpInprocRecognizer, SpMMAudioIn,
        SPEI_RECOGNITION, SPET_LPARAM_IS_OBJECT, SPEVENT, SPLO_STATIC, SPRST_ACTIVE, SPRS_ACTIVE,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
    };

    /// SAPI's SPFEI: an event's interest bit, with the two flag-check bits it requires.
    const fn spfei(e: i32) -> u64 {
        (1u64 << e) | (1u64 << 30) | (1u64 << 33)
    }

    pub fn listen(tx: tokio::sync::mpsc::UnboundedSender<Heard>) -> Result<Listening, String> {
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let stop2 = stop.clone();
        std::thread::Builder::new()
            .name("speech".into())
            .spawn(move || {
                // SAFETY: COM on this thread only, released before it ends.
                unsafe {
                    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                    let r = run(&tx, &stop2, &ready_tx);
                    if let Err(e) = r {
                        let _ = ready_tx.send(Err(crate::fmt::win_error(&e)));
                    }
                    CoUninitialize();
                }
            })
            .map_err(|e| e.to_string())?;
        match ready_rx.recv_timeout(std::time::Duration::from_secs(10)) {
            Ok(Ok(())) => Ok(Listening(stop)),
            Ok(Err(e)) => Err(e),
            Err(_) => Err("the speech recognizer didn't start".into()),
        }
    }

    unsafe fn run(
        tx: &tokio::sync::mpsc::UnboundedSender<Heard>,
        stop: &AtomicBool,
        ready: &std::sync::mpsc::Sender<Result<(), String>>,
    ) -> windows::core::Result<()> {
        let rec: ISpRecognizer = CoCreateInstance(&SpInprocRecognizer, None, CLSCTX_ALL)?;
        let audio: ISpAudio = CoCreateInstance(&SpMMAudioIn, None, CLSCTX_ALL)?;
        rec.SetInput(&audio, true)?;
        let ctx: ISpRecoContext = rec.CreateRecoContext()?;
        ctx.SetNotifyWin32Event()?;
        ctx.SetInterest(spfei(SPEI_RECOGNITION.0), spfei(SPEI_RECOGNITION.0))?;
        let grammar = ctx.CreateGrammar(1)?;
        grammar.LoadDictation(None, SPLO_STATIC)?;
        grammar.SetDictationState(SPRS_ACTIVE)?;
        rec.SetRecoState(SPRST_ACTIVE)?;
        let _ = ready.send(Ok(()));
        while !stop.load(Ordering::Relaxed) {
            // A quarter second at a time, to notice the stop.
            if ctx.WaitForNotifyEvent(250).is_err() {
                continue;
            }
            loop {
                let mut ev = SPEVENT::default();
                let mut got = 0u32;
                if ctx.GetEvents(1, &mut ev, &mut got).is_err() || got == 0 {
                    break;
                }
                if ev._bitfield & 0xFFFF != SPEI_RECOGNITION.0 || (ev._bitfield >> 16) != SPET_LPARAM_IS_OBJECT.0 {
                    continue;
                }
                // The event holds a reference to the result: take it over.
                let Ok(result) = ISpRecoResult::from_raw(ev.lParam.0 as *mut _).cast::<ISpRecoResult>() else {
                    continue;
                };
                let mut text = PWSTR::null();
                if result.GetText(u32::MAX, u32::MAX, true, &mut text, None).is_err() || text.is_null() {
                    continue;
                }
                let words = text.to_string().unwrap_or_default();
                CoTaskMemFree(Some(text.0 as _));
                let confidence = result
                    .cast::<ISpPhrase>()
                    .and_then(|p| p.GetPhrase())
                    .ok()
                    .filter(|p| !p.is_null())
                    .map(|p| {
                        let c = f64::from((*p).Base.Rule.SREngineConfidence);
                        CoTaskMemFree(Some(p as _));
                        c
                    })
                    .unwrap_or(0.0);
                let _ = tx.send(Heard {
                    text: words,
                    confidence,
                });
            }
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn listen(_: tokio::sync::mpsc::UnboundedSender<super::Heard>) -> Result<super::Listening, String> {
        Err("Speech to Text needs Windows' speech recognizer; Linux has none".into())
    }
}
