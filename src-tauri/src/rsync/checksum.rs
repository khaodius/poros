//! The checksums of rsync protocol 29: a rolling weak sum that finds candidate blocks, and
//! MD4 sums that confirm them and verify whole files.

use md4::{Digest, Md4};

pub const STRONG_SUM_BYTES: usize = 16;
/// rsync's smallest block, used for every file up to 490,000 bytes.
const MIN_BLOCK_LENGTH: u64 = 700;
/// The largest block rsync 3 picks, which keeps the window a worker holds small.
const MAX_BLOCK_LENGTH: u64 = 128 * 1024;

/// rsync's `get_checksum1`. Bytes are summed as signed values, as the C code does.
#[derive(Debug, Clone, Copy)]
pub struct RollingSum {
    low: u32,
    high: u32,
    window: u32,
}

impl RollingSum {
    pub fn new(window: &[u8]) -> Self {
        let mut low = 0u32;
        let mut high = 0u32;
        for &byte in window {
            low = low.wrapping_add(signed(byte));
            high = high.wrapping_add(low);
        }
        Self {
            low,
            high,
            window: window.len() as u32,
        }
    }

    /// Moves the window one byte: `outgoing` leaves the front and `incoming` joins the back.
    pub fn roll(&mut self, outgoing: u8, incoming: u8) {
        let outgoing = signed(outgoing);
        self.low = self
            .low
            .wrapping_sub(outgoing)
            .wrapping_add(signed(incoming));
        self.high = self
            .high
            .wrapping_sub(self.window.wrapping_mul(outgoing))
            .wrapping_add(self.low);
    }

    pub fn value(&self) -> u32 {
        (self.low & 0xffff) | (self.high << 16)
    }
}

fn signed(byte: u8) -> u32 {
    byte as i8 as i32 as u32
}

/// The strong sum of one block; rsync appends the seed unless it is zero.
pub fn block_sum(block: &[u8], seed: i32) -> [u8; STRONG_SUM_BYTES] {
    let mut digest = Md4::new();
    digest.update(block);
    if seed != 0 {
        digest.update(seed.to_le_bytes());
    }
    digest.finalize().into()
}

/// The whole-file sum both sides compare after a transfer; the seed comes first.
pub struct FileSum(Md4);

impl FileSum {
    pub fn new(seed: i32) -> Self {
        let mut digest = Md4::new();
        digest.update(seed.to_le_bytes());
        Self(digest)
    }

    pub fn update(&mut self, data: &[u8]) {
        self.0.update(data);
    }

    pub fn finish(self) -> [u8; STRONG_SUM_BYTES] {
        self.0.finalize().into()
    }
}

/// rsync's square-root block size, rounded down to a multiple of 8.
pub fn block_length(file_length: u64) -> u32 {
    let root = file_length.isqrt() / 8 * 8;
    root.clamp(MIN_BLOCK_LENGTH, MAX_BLOCK_LENGTH) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rolling_matches_a_fresh_sum_at_every_offset() {
        let data: Vec<u8> = (0..4096u32)
            .map(|index| (index.wrapping_mul(2_654_435_761) >> 13) as u8)
            .collect();
        let window = 700;
        let mut rolling = RollingSum::new(&data[..window]);
        for start in 1..data.len() - window {
            rolling.roll(data[start - 1], data[start + window - 1]);
            assert_eq!(
                rolling.value(),
                RollingSum::new(&data[start..start + window]).value()
            );
        }
    }

    #[test]
    fn weak_sum_treats_bytes_as_signed() {
        // s1 = -1 + 1 = 0 and s2 = -1 + 0 = -1 with signed bytes.
        assert_eq!(RollingSum::new(&[0xff, 0x01]).value(), 0xffff_0000);
    }

    #[test]
    fn strong_sums_are_md4() {
        assert_eq!(
            block_sum(b"abc", 0),
            [
                0xa4, 0x48, 0x01, 0x7a, 0xaf, 0x21, 0xd8, 0x52, 0x5f, 0xc1, 0x0a, 0xe8, 0x7a, 0xa6,
                0x72, 0x9d
            ]
        );
        let mut whole = FileSum::new(0);
        whole.update(b"abc");
        let mut seeded = Md4::new();
        seeded.update([0, 0, 0, 0]);
        seeded.update(b"abc");
        assert_eq!(whole.finish(), <[u8; 16]>::from(seeded.finalize()));
    }

    #[test]
    fn block_length_follows_the_square_root() {
        assert_eq!(block_length(0), 700);
        assert_eq!(block_length(490_000), 700);
        assert_eq!(block_length(100_000_000), 10_000);
        assert_eq!(block_length(1 << 40), 128 * 1024);
    }
}
