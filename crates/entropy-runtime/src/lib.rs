//! The decay runtime: randomness, decay steps, char and string objects, and output.
//!
//! This is the only place decay is implemented. The core API here never exits the process. It
//! returns [`Halt`] instead, so callers decide what stopping means.

mod rng;

use std::io::{Read, Write};

use rng::ChaCha8;

/// Why a program must stop early.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Halt {
    OutOfFuel,
    /// Writing to stdout failed, for example because the reader closed the pipe.
    OutputClosed,
}

/// A char object. Shared between names (spec 4.2), so callers keep it behind a shared pointer.
#[derive(Clone, Debug, PartialEq)]
pub struct CharObj(pub f32);

/// A string object: one f32 cell per Unicode scalar value.
#[derive(Clone, Debug, PartialEq)]
pub struct StrObj(pub Vec<f32>);

impl StrObj {
    pub fn from_text(text: &str) -> Self {
        StrObj(text.chars().map(|c| c as u32 as f32).collect())
    }

    /// A real's printed form, as a string.
    pub fn from_real(v: f32) -> Self {
        Self::from_text(&format_real(v))
    }

    pub fn from_char(v: f32) -> Self {
        StrObj(vec![display_char(v) as u32 as f32])
    }

    /// The cells as displayed.
    pub fn text(&self) -> String {
        self.0.iter().map(|&v| display_char(v)).collect()
    }

    /// A new object whose cells are the display code points of this one's (spec 4.1).
    pub fn snapshot(&self) -> StrObj {
        StrObj(
            self.0
                .iter()
                .map(|&v| display_char(v) as u32 as f32)
                .collect(),
        )
    }

    /// Joins two snapshots.
    pub fn concat(&self, other: &StrObj) -> StrObj {
        StrObj(self.0.iter().chain(&other.0).copied().collect())
    }
}

/// How a char value displays (spec 4.4).
pub fn display_char(v: f32) -> char {
    if v.is_nan() {
        return '\u{fffd}';
    }
    let r = v.round_ties_even();
    if r < 0.0 || r > 1_114_111.0 || (55_296.0..=57_343.0).contains(&r) {
        return '\u{fffd}';
    }
    char::from_u32(r as u32).unwrap_or('\u{fffd}')
}

/// How a real prints: shortest round-trip, no exponent, `NaN`, `inf`, `-inf`, `-0` (spec 4.6).
pub fn format_real(v: f32) -> String {
    v.to_string()
}

pub struct Runtime<W: Write> {
    rng: ChaCha8,
    rate: f32,
    /// Steps left, if limited.
    fuel: Option<u64>,
    out: W,
    buf: Vec<u8>,
}

impl<W: Write> Runtime<W> {
    /// Without a seed, one is read from the OS.
    pub fn new(rate: f32, seed: Option<u64>, fuel: Option<u64>, out: W) -> Self {
        Runtime {
            rng: ChaCha8::from_seed(seed.unwrap_or_else(os_seed)),
            rate,
            fuel,
            out,
            buf: Vec::new(),
        }
    }

    /// One decay step on `v` (spec 4.3).
    pub fn step(&mut self, v: &mut f32) -> Result<(), Halt> {
        if let Some(fuel) = &mut self.fuel {
            if *fuel == 0 {
                return Err(Halt::OutOfFuel);
            }
            *fuel -= 1;
        }
        let k = self.rng.draw();
        let s = self.rng.draw();
        let delta = self.rate / f32::from(k);
        *v += if f32::from(s) < 127.5 { -delta } else { delta };
        Ok(())
    }

    /// Steps `v` in place and returns the new value.
    pub fn consume_real(&mut self, v: &mut f32) -> Result<f32, Halt> {
        self.step(v)?;
        Ok(*v)
    }

    pub fn consume_char(&mut self, c: &mut CharObj) -> Result<f32, Halt> {
        self.consume_real(&mut c.0)
    }

    /// One step per cell, in order.
    pub fn consume_str(&mut self, s: &mut StrObj) -> Result<(), Halt> {
        for cell in &mut s.0 {
            self.step(cell)?;
        }
        Ok(())
    }

    // Printing never steps. `print e` consumes `e`, then prints it.

    pub fn print_real(&mut self, v: f32) -> Result<(), Halt> {
        self.write(format_real(v).as_bytes())
    }

    pub fn print_char(&mut self, v: f32) -> Result<(), Halt> {
        let mut utf8 = [0u8; 4];
        self.write(display_char(v).encode_utf8(&mut utf8).as_bytes())
    }

    pub fn print_str(&mut self, s: &StrObj) -> Result<(), Halt> {
        self.write(s.text().as_bytes())
    }

    /// Buffers output and flushes it on every newline.
    fn write(&mut self, bytes: &[u8]) -> Result<(), Halt> {
        self.buf.extend_from_slice(bytes);
        if bytes.contains(&b'\n') {
            self.flush()?;
        }
        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), Halt> {
        let result = self
            .out
            .write_all(&self.buf)
            .and_then(|()| self.out.flush());
        self.buf.clear();
        result.map_err(|_| Halt::OutputClosed)
    }

    pub fn into_output(self) -> W {
        self.out
    }
}

/// A seed from `/dev/urandom`, or from the clock if that fails.
fn os_seed() -> u64 {
    let mut bytes = [0u8; 8];
    let read = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes));
    match read {
        Ok(()) => u64::from_le_bytes(bytes),
        Err(_) => {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos() as u64)
                ^ u64::from(std::process::id())
        }
    }
}

#[cfg(test)]
mod tests;
