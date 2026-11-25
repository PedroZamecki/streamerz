//! Linux output through a small audio process: `Streamerz --sound-output <device>` loads PulseAudio
//! (PipeWire answers it too) and plays what it reads on its stdin. It starts with the first sound and
//! ends after the last, so the PulseAudio library and what it loads (about 2.4 MB) never stay in
//! Streamerz itself. The pipe is kept small, so a stop or a change of volume is heard at once.

use super::{Output, CHANNELS, CHUNK};
use std::io::{BufRead, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};

/// The program started as the audio process: this one, unless a test points elsewhere.
fn program() -> Result<std::path::PathBuf, String> {
    if let Some(exe) = std::env::var_os("STREAMERZ_SOUND_EXE") {
        return Ok(exe.into());
    }
    std::env::current_exe().map_err(|e| format!("cannot find Streamerz to play sounds: {e}"))
}

pub struct Helper {
    child: Child,
    stdin: Option<ChildStdin>,
    bytes: Vec<u8>,
}

impl Helper {
    /// What the audio process said when it gave up.
    fn why(&mut self) -> String {
        self.stdin = None;
        let _ = self.child.wait();
        let mut said = String::new();
        if let Some(err) = self.child.stderr.as_mut() {
            let _ = err.read_to_string(&mut said);
        }
        let said = said.trim();
        if said.is_empty() {
            "the sound output stopped".into()
        } else {
            said.to_string()
        }
    }
}

pub fn open(device: &str) -> Result<Box<dyn Output>, String> {
    let mut child = Command::new(program()?)
        .arg("--sound-output")
        .arg(device)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start the sound output: {e}"))?;
    let stdin = child.stdin.take();
    if let Some(pipe) = &stdin {
        use std::os::fd::AsRawFd;
        // SAFETY: fcntl on the pipe this process owns; a smaller pipe is only a request.
        unsafe { libc::fcntl(pipe.as_raw_fd(), libc::F_SETPIPE_SZ, 4096) };
    }
    let mut helper = Helper {
        child,
        stdin,
        bytes: Vec::new(),
    };
    // It says "ok" once the device is open, or why it isn't (and ends).
    let ready = helper.child.stdout.take().is_some_and(|out| {
        let mut line = String::new();
        let mut reader = std::io::BufReader::new(out.take(16));
        reader.read_line(&mut line).is_ok() && line == "ok\n"
    });
    if !ready {
        return Err(helper.why());
    }
    Ok(Box::new(helper))
}

impl Output for Helper {
    fn write(&mut self, frames: &[f32]) -> Result<(), String> {
        if !frames.len().is_multiple_of(CHANNELS) {
            return Err("incomplete sound frame".into());
        }
        self.bytes.clear();
        self.bytes.extend(frames.iter().flat_map(|s| s.to_le_bytes()));
        let Some(stdin) = self.stdin.as_mut() else {
            return Err("the sound output is closed".into());
        };
        if stdin.write_all(&self.bytes).is_err() {
            return Err(self.why());
        }
        Ok(())
    }

    fn drain(&mut self) -> Result<(), String> {
        // Closing its input tells it to play what it has and end.
        self.stdin = None;
        match self.child.wait() {
            Ok(status) if status.success() => Ok(()),
            Ok(_) => Err(self.why()),
            Err(e) => Err(format!("cannot wait for the sound output: {e}")),
        }
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        self.stdin = None;
        let _ = self.child.wait();
    }
}

/// The audio process itself (`Streamerz --sound-output <device>`): open the device, say "ok", play
/// stdin until it closes, let the device finish, end. Its exit code: 0, or 1 when the device failed.
pub fn run(device: &str) -> i32 {
    let mut out = match super::pulse::open(device) {
        Ok(out) => out,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    {
        let mut stdout = std::io::stdout();
        if writeln!(stdout, "ok").and_then(|_| stdout.flush()).is_err() {
            return 1;
        }
    }
    match forward(std::io::stdin().lock(), &mut *out).and_then(|()| out.drain()) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn forward(mut stdin: impl Read, out: &mut dyn Output) -> Result<(), String> {
    let frame = CHANNELS * 4;
    let mut bytes = vec![0u8; CHUNK * frame];
    let mut have = 0;
    let mut samples = Vec::with_capacity(CHUNK * CHANNELS);
    loop {
        let n = match stdin.read(&mut bytes[have..]) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.to_string()),
        };
        have += n;
        let whole = have / frame * frame;
        if whole == 0 {
            continue;
        }
        samples.clear();
        samples.extend(bytes[..whole].as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)));
        out.write(&samples)?;
        bytes.copy_within(whole..have, 0);
        have -= whole;
    }
    if have == 0 {
        Ok(())
    } else {
        Err("incomplete sound frame".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Capture(Vec<f32>);

    impl Output for Capture {
        fn write(&mut self, frames: &[f32]) -> Result<(), String> {
            assert_eq!(frames.len() % CHANNELS, 0);
            self.0.extend_from_slice(frames);
            Ok(())
        }

        fn drain(&mut self) -> Result<(), String> {
            Ok(())
        }
    }

    struct Fragmented<'a>(&'a [u8]);

    impl Read for Fragmented<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = buf.len().min(3);
            self.0.read(&mut buf[..n])
        }
    }

    #[test]
    fn helper_decodes_fragmented_little_endian_stereo() {
        let data: Vec<u8> = [0.25f32, -0.5, 1.0, -0.25]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        let mut out = Capture::default();
        forward(Fragmented(&data), &mut out).unwrap();
        assert_eq!(out.0, [0.25, -0.5, 1.0, -0.25]);

        let mut out = Capture::default();
        assert!(forward(Fragmented(&data[..data.len() - 1]), &mut out).is_err());
        assert_eq!(out.0, [0.25, -0.5]);
    }
}
