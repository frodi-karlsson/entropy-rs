use std::io;

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{Rng, SeedableRng};

use super::*;
use crate::rng::seed_key;

fn runtime(rate: f32, seed: u64, fuel: Option<u64>) -> Runtime<Vec<u8>> {
    Runtime::new(rate, Some(seed), fuel, Vec::new())
}

/// The nonzero keystream bytes for `seed`, computed with `rand_chacha` as an independent check.
fn reference_draws(seed: u64, n: usize) -> Vec<u8> {
    let key = seed_key(seed);
    // One large `fill_bytes`: smaller calls each discard the rest of a 32-bit word.
    let mut bytes = vec![0u8; 4 * n + 1024];
    ChaCha8Rng::from_seed(key).fill_bytes(&mut bytes);
    let mut draws: Vec<u8> = bytes.into_iter().filter(|&b| b != 0).collect();
    draws.truncate(n);
    draws
}

#[test]
fn steps_follow_the_spec_formula() {
    let draws = reference_draws(42, 200);
    let mut rt = runtime(2.0, 42, None);
    let mut v = 10.0f32;
    let mut expected = 10.0f32;
    for &[k, s] in draws.as_chunks::<2>().0 {
        let delta = 2.0 / f32::from(k);
        expected += if s <= 127 { -delta } else { delta };
        assert_eq!(rt.consume_real(&mut v), Ok(expected));
    }
    assert_eq!(v, expected);
}

#[test]
fn same_seed_same_steps() {
    let mut a = runtime(2.0, 9, None);
    let mut b = runtime(2.0, 9, None);
    let (mut x, mut y) = (0.0, 0.0);
    for _ in 0..1000 {
        assert_eq!(a.consume_real(&mut x), b.consume_real(&mut y));
    }
}

#[test]
fn rate_zero_still_draws() {
    let mut rt = runtime(0.0, 5, None);
    let mut v = 3.5;
    rt.step(&mut v).unwrap();
    assert_eq!(v, 3.5);
    let draws = reference_draws(5, 3);
    assert_eq!(rt.rng.draw(), draws[2]);
}

#[test]
fn fuel_limits_steps() {
    let mut rt = runtime(2.0, 1, Some(3));
    let mut v = 0.0;
    for _ in 0..3 {
        rt.step(&mut v).unwrap();
    }
    let before = v;
    assert_eq!(rt.step(&mut v), Err(Halt::OutOfFuel));
    assert_eq!(v, before);
}

#[test]
fn chars_step_in_place() {
    let mut rt = runtime(2.0, 3, None);
    let mut c = CharObj(65.0);
    let v = rt.consume_char(&mut c).unwrap();
    assert_eq!(c.0, v);
    assert_ne!(v, 65.0);
}

#[test]
fn strings_step_every_cell_in_order() {
    let draws = reference_draws(4, 6);
    let mut rt = runtime(1.0, 4, None);
    let mut s = StrObj::from_text("abc");
    rt.consume_str(&mut s).unwrap();
    for (i, cell) in s.0.iter().enumerate() {
        let (k, sign) = (draws[2 * i], draws[2 * i + 1]);
        let delta = 1.0 / f32::from(k);
        let start = f32::from(b"abc"[i]);
        assert_eq!(*cell, start + if sign <= 127 { -delta } else { delta });
    }
}

#[test]
fn string_fuel_runs_out_mid_string() {
    let mut rt = runtime(2.0, 1, Some(2));
    let mut s = StrObj::from_text("abc");
    assert_eq!(rt.consume_str(&mut s), Err(Halt::OutOfFuel));
    assert_eq!(s.0[2], f32::from(b'c'));
}

#[test]
fn display_edge_cases() {
    let cases = [
        (65.0, 'A'),
        (65.4, 'A'),
        (65.5, 'B'),
        (66.5, 'B'),
        (-0.4, '\0'),
        (-0.6, '\u{fffd}'),
        (f32::NAN, '\u{fffd}'),
        (f32::INFINITY, '\u{fffd}'),
        (f32::NEG_INFINITY, '\u{fffd}'),
        (55_295.0, '\u{d7ff}'),
        (55_296.0, '\u{fffd}'),
        (57_343.0, '\u{fffd}'),
        (57_344.0, '\u{e000}'),
        (1_114_111.0, '\u{10ffff}'),
        (1_114_112.0, '\u{fffd}'),
    ];
    for (v, expected) in cases {
        assert_eq!(display_char(v), expected, "{v}");
    }
}

#[test]
fn real_formatting() {
    let cases = [
        (1.0, "1"),
        (0.1, "0.1"),
        (-2.5, "-2.5"),
        (-0.0, "-0"),
        (1e-7, "0.0000001"),
        (1e30, "1000000000000000000000000000000"),
        (f32::NAN, "NaN"),
        (f32::INFINITY, "inf"),
        (f32::NEG_INFINITY, "-inf"),
    ];
    for (v, expected) in cases {
        assert_eq!(format_real(v), expected);
    }
}

#[test]
fn string_objects() {
    assert_eq!(StrObj::from_text("a😀").0, [97.0, 128_512.0]);
    assert_eq!(StrObj::from_real(1.5).0, [49.0, 46.0, 53.0]);
    assert_eq!(StrObj::from_char(65.4).0, [65.0]);
    let drifted = StrObj(vec![65.3, 66.7, -5.0]);
    assert_eq!(drifted.snapshot().0, [65.0, 67.0, 65_533.0]);
    assert_eq!(drifted.text(), "AC\u{fffd}");
    assert_eq!(
        StrObj::from_text("ab").concat(&StrObj::from_text("c")).0,
        [97.0, 98.0, 99.0]
    );
}

#[test]
fn output_flushes_on_newline() {
    let mut rt = runtime(2.0, 1, None);
    rt.print_real(1.5).unwrap();
    rt.print_char(32.0).unwrap();
    assert!(rt.out.is_empty());
    rt.print_str(&StrObj::from_text("x\ny")).unwrap();
    assert_eq!(rt.out, b"1.5 x\ny");
    rt.print_char(10.0).unwrap();
    rt.flush().unwrap();
    assert_eq!(rt.into_output(), b"1.5 x\ny\n");
}

struct Closed;

impl Write for Closed {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn closed_output_halts() {
    let mut rt = Runtime::new(2.0, Some(1), None, Closed);
    assert_eq!(rt.print_real(1.0), Ok(()));
    assert_eq!(rt.flush(), Err(Halt::OutputClosed));
    assert_eq!(
        rt.print_str(&StrObj::from_text("\n")),
        Err(Halt::OutputClosed)
    );
}

#[test]
fn draw_distribution_is_sane() {
    let mut rt = runtime(2.0, 12_345, None);
    let n = 2_000_000;
    let mut counts = [0u32; 256];
    for _ in 0..n {
        counts[usize::from(rt.rng.draw())] += 1;
    }
    assert_eq!(counts[0], 0);
    assert!(counts[1..].iter().all(|&c| c > 0));
    // A step is negative when its sign draw is at most 127.
    let p = 127.0 / 255.0;
    let negative = counts[1..=127].iter().map(|&c| f64::from(c)).sum::<f64>() / n as f64;
    let sigma = (p * (1.0 - p) / n as f64).sqrt();
    assert!((negative - p).abs() < 4.0 * sigma, "{negative} vs {p}");
}

#[test]
fn os_seeds_differ() {
    assert_ne!(os_seed(), os_seed());
}
