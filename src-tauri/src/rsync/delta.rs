//! The delta algorithm: block sums of the old file, a scan of the new file for blocks the
//! old one already has, and rebuilding the new file from those blocks and literal data.

use std::collections::HashMap;
use std::io::{self, BufReader, Read, Seek, Write};

use super::checksum::{block_sum, FileSum, RollingSum, STRONG_SUM_BYTES};

/// Literal data longer than this is sent before the scan finds the next match.
const LITERAL_FLUSH: usize = 256 * 1024;
const READ_CHUNK: usize = 256 * 1024;
const TAG_BITS: u32 = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockSum {
    pub weak: u32,
    pub strong: [u8; STRONG_SUM_BYTES],
}

/// The block sums of a basis file, as one side sends them to the other.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Signature {
    pub block_length: u32,
    /// Length of the last block when it is shorter than the others.
    pub remainder: u32,
    /// Bytes of each strong sum that are compared.
    pub strong_length: u32,
    pub blocks: Vec<BlockSum>,
}

impl Signature {
    pub fn generate(
        basis: impl Read,
        block_length: u32,
        seed: i32,
        mut keep_going: impl FnMut() -> bool,
    ) -> io::Result<Self> {
        let mut reader = BufReader::with_capacity(READ_CHUNK, basis);
        let mut block = vec![0; block_length as usize];
        let mut blocks = Vec::new();
        let mut remainder = 0;
        loop {
            if !keep_going() {
                return Err(io::Error::from(io::ErrorKind::Interrupted));
            }
            let filled = read_full(&mut reader, &mut block)?;
            if filled == 0 {
                break;
            }
            let data = &block[..filled];
            blocks.push(BlockSum {
                weak: RollingSum::new(data).value(),
                strong: block_sum(data, seed),
            });
            if filled < block.len() {
                remainder = filled as u32;
                break;
            }
        }
        Ok(Self {
            block_length,
            remainder,
            strong_length: STRONG_SUM_BYTES as u32,
            blocks,
        })
    }

    pub fn block_size(&self, index: usize) -> usize {
        if index + 1 == self.blocks.len() && self.remainder != 0 {
            self.remainder as usize
        } else {
            self.block_length as usize
        }
    }

    fn full_blocks(&self) -> usize {
        self.blocks
            .len()
            .saturating_sub(usize::from(self.remainder != 0))
    }

    fn strong_matches(&self, index: usize, strong: &[u8; STRONG_SUM_BYTES]) -> bool {
        let length = self.strong_length as usize;
        self.blocks[index].strong[..length] == strong[..length]
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Token<'a> {
    Literal(&'a [u8]),
    Block(u32),
}

/// Scans `source` for blocks of `signature` and hands out the tokens that rebuild it,
/// returning the whole-file sum.
pub fn find_matches(
    signature: &Signature,
    seed: i32,
    source: impl Read,
    mut emit: impl FnMut(Token) -> io::Result<()>,
) -> io::Result<[u8; STRONG_SUM_BYTES]> {
    let block_length = signature.block_length as usize;
    let full_blocks = signature.full_blocks();
    let mut by_weak: HashMap<u32, Vec<u32>> = HashMap::new();
    // A table of hashed weak sums rules out most positions without a map lookup.
    let mut tags = vec![false; 1 << TAG_BITS];
    for (index, block) in signature.blocks[..full_blocks].iter().enumerate() {
        by_weak.entry(block.weak).or_default().push(index as u32);
        tags[tag(block.weak)] = true;
    }

    let mut input = Input {
        source,
        buffer: Vec::new(),
        ended: false,
        sum: FileSum::new(seed),
    };
    let mut position = 0;
    let mut literal_start = 0;
    let mut rolling: Option<RollingSum> = None;
    let mut expected_next = 0u32;

    if block_length > 0 && full_blocks > 0 {
        loop {
            if input.available(position) < block_length {
                input.compact(&mut literal_start, &mut position);
                input.fill(position + block_length + 1)?;
                if input.available(position) < block_length {
                    break;
                }
            }
            let window = &input.buffer[position..position + block_length];
            let weak = match rolling {
                Some(sum) => sum.value(),
                None => {
                    let sum = RollingSum::new(window);
                    rolling = Some(sum);
                    sum.value()
                }
            };
            let matched = if tags[tag(weak)] {
                by_weak.get(&weak).and_then(|candidates| {
                    let strong = block_sum(window, seed);
                    let mut found = candidates
                        .iter()
                        .copied()
                        .filter(|&index| signature.strong_matches(index as usize, &strong));
                    let first = found.next()?;
                    // Prefer the block after the last match, which keeps runs of blocks in order.
                    Some(if first == expected_next {
                        first
                    } else {
                        found.find(|&index| index == expected_next).unwrap_or(first)
                    })
                })
            } else {
                None
            };

            if let Some(index) = matched {
                if literal_start < position {
                    emit(Token::Literal(&input.buffer[literal_start..position]))?;
                }
                emit(Token::Block(index))?;
                position += block_length;
                literal_start = position;
                rolling = None;
                expected_next = index + 1;
                continue;
            }

            if position - literal_start >= LITERAL_FLUSH {
                emit(Token::Literal(&input.buffer[literal_start..position]))?;
                literal_start = position;
            }
            if input.available(position) <= block_length {
                input.compact(&mut literal_start, &mut position);
                input.fill(position + block_length + 1)?;
            }
            if input.available(position) <= block_length {
                // The window reached the end of the file.
                break;
            }
            if let Some(sum) = rolling.as_mut() {
                sum.roll(
                    input.buffer[position],
                    input.buffer[position + block_length],
                );
            }
            position += 1;
        }
    }

    // What is left is literal data, apart from a short last block that may end the file.
    let keep = signature.remainder as usize;
    loop {
        input.compact(&mut literal_start, &mut position);
        input.fill(literal_start + LITERAL_FLUSH + keep)?;
        if input.ended {
            break;
        }
        let send_to = input.buffer.len() - keep;
        if send_to > literal_start {
            emit(Token::Literal(&input.buffer[literal_start..send_to]))?;
            literal_start = send_to;
            position = position.max(send_to);
        }
    }
    let end = input.buffer.len();
    if signature.remainder != 0 && !signature.blocks.is_empty() {
        let last = signature.blocks.len() - 1;
        let tail_start = end.saturating_sub(signature.remainder as usize);
        let tail = &input.buffer[tail_start..end];
        let matches_last = tail_start >= position
            && tail.len() == signature.remainder as usize
            && RollingSum::new(tail).value() == signature.blocks[last].weak
            && signature.strong_matches(last, &block_sum(tail, seed));
        if matches_last {
            if literal_start < tail_start {
                emit(Token::Literal(&input.buffer[literal_start..tail_start]))?;
            }
            emit(Token::Block(last as u32))?;
            literal_start = end;
        }
    }
    if literal_start < end {
        emit(Token::Literal(&input.buffer[literal_start..end]))?;
    }
    Ok(input.sum.finish())
}

fn tag(weak: u32) -> usize {
    (weak.wrapping_mul(0x9e37_79b1) >> (32 - TAG_BITS)) as usize
}

/// The part of the source the scan still needs, from the oldest unsent literal byte.
struct Input<R> {
    source: R,
    buffer: Vec<u8>,
    ended: bool,
    sum: FileSum,
}

impl<R: Read> Input<R> {
    fn available(&self, position: usize) -> usize {
        self.buffer.len() - position
    }

    /// Drops bytes already sent, keeping indexes into the buffer valid.
    fn compact(&mut self, literal_start: &mut usize, position: &mut usize) {
        if *literal_start > 0 {
            self.buffer.drain(..*literal_start);
            *position -= *literal_start;
            *literal_start = 0;
        }
    }

    /// Reads until the buffer holds `wanted` bytes or the source ends.
    fn fill(&mut self, wanted: usize) -> io::Result<()> {
        while !self.ended && self.buffer.len() < wanted {
            let start = self.buffer.len();
            self.buffer.resize(start + READ_CHUNK, 0);
            let read = loop {
                match self.source.read(&mut self.buffer[start..]) {
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    other => break other,
                }
            };
            let read = match read {
                Ok(read) => read,
                Err(error) => {
                    self.buffer.truncate(start);
                    return Err(error);
                }
            };
            self.buffer.truncate(start + read);
            self.sum.update(&self.buffer[start..]);
            self.ended = read == 0;
        }
        Ok(())
    }
}

/// Rebuilds a file from tokens, copying matched blocks out of the basis file.
pub struct Reconstruction<B, W> {
    basis: BufReader<B>,
    basis_position: u64,
    output: W,
    signature: Signature,
    sum: FileSum,
    block: Vec<u8>,
    written: u64,
}

impl<B: Read + Seek, W: Write> Reconstruction<B, W> {
    pub fn new(basis: B, output: W, signature: Signature, seed: i32) -> Self {
        Self {
            basis: BufReader::with_capacity(READ_CHUNK, basis),
            basis_position: 0,
            output,
            block: Vec::with_capacity(signature.block_length as usize),
            signature,
            sum: FileSum::new(seed),
            written: 0,
        }
    }

    pub fn literal(&mut self, data: &[u8]) -> io::Result<()> {
        self.output.write_all(data)?;
        self.sum.update(data);
        self.written += data.len() as u64;
        Ok(())
    }

    /// Copies a basis block; returns its length.
    pub fn block(&mut self, index: u32) -> io::Result<u64> {
        let index = index as usize;
        if index >= self.signature.blocks.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("The server referred to block {index}, which does not exist"),
            ));
        }
        let offset = index as u64 * u64::from(self.signature.block_length);
        let length = self.signature.block_size(index);
        if offset != self.basis_position {
            self.basis
                .seek_relative(offset as i64 - self.basis_position as i64)?;
        }
        self.block.resize(length, 0);
        self.basis.read_exact(&mut self.block)?;
        self.basis_position = offset + length as u64;
        self.output.write_all(&self.block)?;
        self.sum.update(&self.block);
        self.written += length as u64;
        Ok(length as u64)
    }

    /// Checks the rebuilt file against the sender's sum; returns its length.
    pub fn finish(mut self, expected: &[u8; STRONG_SUM_BYTES]) -> io::Result<u64> {
        self.output.flush()?;
        if &self.sum.finish() != expected {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "The rebuilt file does not match the server's checksum",
            ));
        }
        Ok(self.written)
    }
}

fn read_full(reader: &mut impl Read, buffer: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(read) => filled += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(filled)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn pattern(length: usize, salt: u32) -> Vec<u8> {
        (0..length as u32)
            .map(|index| (index.wrapping_add(salt).wrapping_mul(2_654_435_761) >> 11) as u8)
            .collect()
    }

    /// Runs both halves and returns the rebuilt file and how many literal bytes it took.
    fn round_trip(basis: &[u8], source: &[u8], block_length: u32, seed: i32) -> (Vec<u8>, usize) {
        let signature = Signature::generate(basis, block_length, seed, || true).unwrap();
        let mut rebuilt =
            Reconstruction::new(Cursor::new(basis), Vec::new(), signature.clone(), seed);
        let mut literal = 0;
        let sum = find_matches(&signature, seed, source, |token| match token {
            Token::Literal(data) => {
                literal += data.len();
                rebuilt.literal(data)
            }
            Token::Block(index) => rebuilt.block(index).map(|_| ()),
        })
        .unwrap();
        let output = rebuilt.output.clone();
        rebuilt.finish(&sum).unwrap();
        (output, literal)
    }

    #[test]
    fn identical_files_send_no_literal_data() {
        let data = pattern(100_000, 1);
        let (rebuilt, literal) = round_trip(&data, &data, 700, 12345);
        assert_eq!(rebuilt, data);
        assert_eq!(literal, 0);
    }

    #[test]
    fn edits_send_only_what_changed() {
        let basis = pattern(200_000, 7);
        let mut source = basis.clone();
        source.splice(50_000..50_010, b"inserted bytes".iter().copied());
        source.drain(120_000..120_500);
        source[180_000] ^= 0xff;
        let (rebuilt, literal) = round_trip(&basis, &source, 700, 0);
        assert_eq!(rebuilt, source);
        assert!(literal < 4 * 700, "sent {literal} literal bytes");
    }

    #[test]
    fn short_and_unrelated_files_round_trip() {
        for (basis, source) in [
            (pattern(0, 1), pattern(5000, 2)),
            (pattern(5000, 1), Vec::new()),
            (pattern(300, 1), pattern(300, 1)),
            (pattern(10_000, 3), pattern(20_000, 4)),
            (pattern(1_000_000, 5), pattern(1_000_000, 6)),
        ] {
            let (rebuilt, _) = round_trip(&basis, &source, 700, -9);
            assert_eq!(rebuilt, source);
        }
    }

    #[test]
    fn appended_data_reuses_the_whole_basis() {
        let basis = pattern(70_123, 9);
        let mut source = basis.clone();
        source.extend(pattern(1000, 10));
        let (rebuilt, literal) = round_trip(&basis, &source, 1000, 1);
        assert_eq!(rebuilt, source);
        assert!(literal <= 1000 + 123, "sent {literal} literal bytes");
    }

    #[test]
    fn the_short_last_block_matches_at_the_end() {
        let basis = pattern(10_350, 4);
        let mut source = pattern(500, 8);
        source.extend_from_slice(&basis);
        let (rebuilt, literal) = round_trip(&basis, &source, 700, 3);
        assert_eq!(rebuilt, source);
        assert_eq!(literal, 500);
    }

    #[test]
    fn long_literal_runs_are_sent_in_pieces() {
        let signature = Signature::generate(&pattern(10_000, 1)[..], 700, 0, || true).unwrap();
        let source = pattern(LITERAL_FLUSH * 3, 2);
        let mut largest = 0;
        find_matches(&signature, 0, &source[..], |token| {
            if let Token::Literal(data) = token {
                largest = largest.max(data.len());
            }
            Ok(())
        })
        .unwrap();
        assert!(largest < LITERAL_FLUSH + 700, "{largest}");
    }

    #[test]
    fn a_short_last_block_without_blocks_matches_nothing() {
        let signature = Signature {
            block_length: 700,
            remainder: 120,
            strong_length: STRONG_SUM_BYTES as u32,
            blocks: Vec::new(),
        };
        let source = pattern(1000, 3);
        let mut literal = 0;
        find_matches(&signature, 0, &source[..], |token| {
            if let Token::Literal(data) = token {
                literal += data.len();
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(literal, source.len());
    }

    #[test]
    fn a_wrong_sum_is_refused() {
        let basis = pattern(5000, 1);
        let signature = Signature::generate(&basis[..], 700, 0, || true).unwrap();
        let mut rebuilt = Reconstruction::new(Cursor::new(&basis), Vec::new(), signature, 0);
        rebuilt.literal(b"hello").unwrap();
        assert!(rebuilt.finish(&[0; 16]).is_err());
    }
}
