//! ChaCha8 keyed from splitmix64 (spec 4.7).

/// The standard splitmix64 generator.
pub struct SplitMix64(pub u64);

impl SplitMix64 {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

const CONSTANTS: [u32; 4] = [0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574];

fn quarter_round(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = (s[d] ^ s[a]).rotate_left(16);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = (s[b] ^ s[c]).rotate_left(12);
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = (s[d] ^ s[a]).rotate_left(8);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = (s[b] ^ s[c]).rotate_left(7);
}

/// One ChaCha block. `tail` is state words 12..16 (counter and nonce).
pub fn block(key: &[u32; 8], tail: [u32; 4], rounds: u32) -> [u8; 64] {
    let mut input = [0u32; 16];
    input[..4].copy_from_slice(&CONSTANTS);
    input[4..12].copy_from_slice(key);
    input[12..].copy_from_slice(&tail);
    let mut s = input;
    for _ in 0..rounds / 2 {
        quarter_round(&mut s, 0, 4, 8, 12);
        quarter_round(&mut s, 1, 5, 9, 13);
        quarter_round(&mut s, 2, 6, 10, 14);
        quarter_round(&mut s, 3, 7, 11, 15);
        quarter_round(&mut s, 0, 5, 10, 15);
        quarter_round(&mut s, 1, 6, 11, 12);
        quarter_round(&mut s, 2, 7, 8, 13);
        quarter_round(&mut s, 3, 4, 9, 14);
    }
    let mut out = [0u8; 64];
    for (i, chunk) in out.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        *chunk = s[i].wrapping_add(input[i]).to_le_bytes();
    }
    out
}

/// The key for `seed`: the first 4 splitmix64 outputs, each little-endian.
pub fn seed_key(seed: u64) -> [u8; 32] {
    let mut sm = SplitMix64(seed);
    let mut bytes = [0u8; 32];
    for chunk in bytes.as_chunks_mut::<8>().0 {
        *chunk = sm.next().to_le_bytes();
    }
    bytes
}

/// The ChaCha8 keystream with `rand_chacha`'s layout: a 64-bit block counter and a 64-bit stream
/// id, both starting at 0. Bytes are used in order.
pub struct ChaCha8 {
    key: [u32; 8],
    counter: u64,
    buf: [u8; 64],
    used: usize,
}

impl ChaCha8 {
    pub fn from_key(bytes: [u8; 32]) -> Self {
        let mut key = [0u32; 8];
        for (word, chunk) in key.iter_mut().zip(bytes.as_chunks::<4>().0) {
            *word = u32::from_le_bytes(*chunk);
        }
        ChaCha8 {
            key,
            counter: 0,
            buf: [0; 64],
            used: 64,
        }
    }

    pub fn from_seed(seed: u64) -> Self {
        Self::from_key(seed_key(seed))
    }

    pub fn next_byte(&mut self) -> u8 {
        if self.used == self.buf.len() {
            let tail = [self.counter as u32, (self.counter >> 32) as u32, 0, 0];
            self.buf = block(&self.key, tail, 8);
            self.counter = self.counter.wrapping_add(1);
            self.used = 0;
        }
        let byte = self.buf[self.used];
        self.used += 1;
        byte
    }

    /// The next nonzero byte, uniform on 1..=255.
    pub fn draw(&mut self) -> u8 {
        loop {
            let byte = self.next_byte();
            if byte != 0 {
                return byte;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rand_chacha::ChaCha8Rng;
    use rand_chacha::rand_core::{Rng, SeedableRng};

    use super::*;

    #[test]
    fn splitmix64_known_answer() {
        let mut sm = SplitMix64(0);
        assert_eq!(sm.next(), 0xe220_a839_7b1d_cdaf);
        assert_eq!(sm.next(), 0x6e78_9e6a_a1b9_65f4);
    }

    #[test]
    fn rfc_8439_block() {
        // RFC 8439 section 2.3.2: key 00..1f, counter 1, nonce 000000090000004a00000000.
        let mut key = [0u8; 32];
        for (i, b) in key.iter_mut().enumerate() {
            *b = i as u8;
        }
        let words = ChaCha8::from_key(key).key;
        let tail = [1, 0x0900_0000, 0x4a00_0000, 0];
        let expected: [u8; 64] = [
            0x10, 0xf1, 0xe7, 0xe4, 0xd1, 0x3b, 0x59, 0x15, 0x50, 0x0f, 0xdd, 0x1f, 0xa3, 0x20,
            0x71, 0xc4, 0xc7, 0xd1, 0xf4, 0xc7, 0x33, 0xc0, 0x68, 0x03, 0x04, 0x22, 0xaa, 0x9a,
            0xc3, 0xd4, 0x6c, 0x4e, 0xd2, 0x82, 0x64, 0x46, 0x07, 0x9f, 0xaa, 0x09, 0x14, 0xc2,
            0xd7, 0x05, 0xd9, 0x8b, 0x02, 0xa2, 0xb5, 0x12, 0x9c, 0xd1, 0xde, 0x16, 0x4e, 0xb9,
            0xcb, 0xd0, 0x83, 0xe8, 0xa2, 0x50, 0x3c, 0x4e,
        ];
        assert_eq!(block(&words, tail, 20), expected);
    }

    #[test]
    fn keystream_matches_rand_chacha() {
        for seed in [0, 1, 42, u64::MAX] {
            // Long enough to cross many blocks.
            let mut expected = vec![0u8; 64 * 1000 + 7];
            ChaCha8Rng::from_seed(seed_key(seed)).fill_bytes(&mut expected);
            let mut ours = ChaCha8::from_seed(seed);
            let actual: Vec<u8> = (0..expected.len()).map(|_| ours.next_byte()).collect();
            assert_eq!(actual, expected, "seed {seed}");
        }
    }

    #[test]
    fn draw_skips_zero_bytes() {
        let mut expected = vec![0u8; 100_000];
        ChaCha8Rng::from_seed(seed_key(7)).fill_bytes(&mut expected);
        expected.retain(|&b| b != 0);
        let mut ours = ChaCha8::from_seed(7);
        let actual: Vec<u8> = (0..expected.len()).map(|_| ours.draw()).collect();
        assert_eq!(actual, expected);
    }
}
