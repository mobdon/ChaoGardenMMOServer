//! Client for the headless recompiled game (client repo `sa2_boot::serve`,
//! `SA2_SERVE`). The worker is built locally from the owner's own game files
//! and never committed; the server only starts it and talks to it.
//!
//! The game runs one frame per `frame` command and answers `read`, `write`,
//! `call` and `sec` at frame boundaries, so Chao AI, saves and competitions
//! run through the game's own code.
use std::io::{self, BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

pub struct Worker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Worker {
    /// Start `exe args..` in server mode with `envs` (e.g. SA2_STEAM_DIR).
    pub fn spawn(exe: &str, args: &[&str], envs: &[(&str, &str)]) -> io::Result<Self> {
        let mut child = Command::new(exe)
            .args(args)
            .envs(envs.iter().copied())
            .env("SA2_SERVE", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
        Ok(Self {
            child,
            stdin,
            stdout,
        })
    }

    fn ask(&mut self, line: &str) -> io::Result<String> {
        writeln!(self.stdin, "{line}")?;
        self.stdin.flush()?;
        let mut reply = String::new();
        if self.stdout.read_line(&mut reply)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "worker exited",
            ));
        }
        let reply = reply.trim_end();
        match reply.split_once(' ').unwrap_or((reply, "")) {
            ("ok", rest) => Ok(rest.to_owned()),
            ("err", e) => Err(io::Error::other(format!("{line}: {e}"))),
            _ => Err(io::Error::other(format!("{line}: bad reply {reply:?}"))),
        }
    }

    fn hex(s: &str) -> io::Result<u32> {
        u32::from_str_radix(s, 16).map_err(io::Error::other)
    }

    /// Run `n` frames; returns the game's frame count.
    pub fn frame(&mut self, n: u64) -> io::Result<u64> {
        self.ask(&format!("frame {n}"))?
            .parse()
            .map_err(io::Error::other)
    }

    pub fn read(&mut self, addr: u32, len: u32) -> io::Result<Vec<u8>> {
        let s = self.ask(&format!("read {addr:x} {len:x}"))?;
        (0..s.len() / 2)
            .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(io::Error::other))
            .collect()
    }

    pub fn r32(&mut self, addr: u32) -> io::Result<u32> {
        let b = self.read(addr, 4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn write(&mut self, addr: u32, bytes: &[u8]) -> io::Result<()> {
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        self.ask(&format!("write {addr:x} {hex}")).map(drop)
    }

    /// Call a game function with args in r3..; returns r3.
    pub fn call(&mut self, addr: u32, args: &[u32]) -> io::Result<u32> {
        let mut line = format!("call {addr:x}");
        for a in args {
            line.push_str(&format!(" {a:x}"));
        }
        Self::hex(&self.ask(&line)?)
    }

    /// Run-time address of a linked REL section (ChaoMain is module 0x3C).
    pub fn sec(&mut self, module: u32, section: u32) -> io::Result<u32> {
        Self::hex(&self.ask(&format!("sec {module:x} {section:x}"))?)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = writeln!(self.stdin, "quit");
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Protocol shape only, against a scripted stand-in (not the game).
    #[test]
    fn parses_replies_and_errors() {
        let script = "while read l; do case \"$l\" in \
            frame*) echo 'ok 42';; read*) echo 'ok 0000beef';; call*) echo 'ok 1f';; \
            sec*) echo 'err module 3c not linked';; *) echo ok;; esac; done";
        let mut w = Worker::spawn("sh", &["-c", script], &[]).unwrap();
        assert_eq!(w.frame(3).unwrap(), 42);
        assert_eq!(w.r32(0x8000_1000).unwrap(), 0xBEEF);
        assert_eq!(w.call(0x8000_2000, &[1, 2]).unwrap(), 0x1F);
        w.write(0x8000_1000, &[1, 2]).unwrap();
        assert!(w.sec(0x3C, 6).is_err());
    }
}
