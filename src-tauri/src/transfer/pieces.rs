//! Splits a file into pieces that workers claim in order, so several connections can move one
//! large file at once, and an interrupted transfer knows how much of the target is complete.

use std::ops::Range;

pub struct Pieces {
    start: u64,
    end: u64,
    piece_size: u64,
    next_unclaimed: u64,
    /// Indexed by `(offset - start) / piece_size`.
    completed: Vec<bool>,
    /// Bytes from the start of each piece known to be in the target.
    reached: Vec<u64>,
}

impl Pieces {
    pub fn new(start: u64, end: u64, piece_size: u64) -> Self {
        let piece_size = piece_size.max(1);
        let count = end.saturating_sub(start).div_ceil(piece_size) as usize;
        Self {
            start,
            end: end.max(start),
            piece_size,
            next_unclaimed: start,
            completed: vec![false; count],
            reached: vec![0; count],
        }
    }

    pub fn end(&self) -> u64 {
        self.end
    }

    pub fn has_unclaimed(&self) -> bool {
        self.next_unclaimed < self.end
    }

    pub fn claim(&mut self) -> Option<Range<u64>> {
        if !self.has_unclaimed() {
            return None;
        }
        let piece_start = self.next_unclaimed;
        let piece_end = (piece_start + self.piece_size).min(self.end);
        self.next_unclaimed = piece_end;
        Some(piece_start..piece_end)
    }

    pub fn complete(&mut self, piece_start: u64) {
        if let Some(done) = self
            .index(piece_start)
            .and_then(|index| self.completed.get_mut(index))
        {
            *done = true;
        }
    }

    /// Everything from `piece_start` up to `offset` is in the target.
    pub fn reach(&mut self, piece_start: u64, offset: u64) {
        if let Some(reached) = self
            .index(piece_start)
            .and_then(|index| self.reached.get_mut(index))
        {
            *reached = (*reached).max(offset.saturating_sub(piece_start));
        }
    }

    /// The source ended early at `offset`: pieces beyond it no longer exist.
    pub fn end_at(&mut self, offset: u64) {
        if offset >= self.end {
            return;
        }
        self.end = offset.max(self.start);
        self.next_unclaimed = self.next_unclaimed.min(self.end);
        let count = (self.end - self.start).div_ceil(self.piece_size) as usize;
        self.completed.truncate(count);
        self.reached.truncate(count);
    }

    /// Everything before this offset is in the target.
    pub fn complete_prefix(&self) -> u64 {
        match self.completed.iter().position(|done| !done) {
            Some(index) => {
                let piece_start = self.start + index as u64 * self.piece_size;
                (piece_start + self.reached[index]).min(self.end)
            }
            None => self.end,
        }
    }

    pub fn is_complete(&self) -> bool {
        self.completed.iter().all(|done| *done)
    }

    fn index(&self, piece_start: u64) -> Option<usize> {
        piece_start
            .checked_sub(self.start)
            .map(|relative| (relative / self.piece_size) as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claims_in_order_and_tracks_the_complete_prefix() {
        let mut pieces = Pieces::new(0, 25, 10);
        assert_eq!(pieces.claim(), Some(0..10));
        assert_eq!(pieces.claim(), Some(10..20));
        assert_eq!(pieces.claim(), Some(20..25));
        assert_eq!(pieces.claim(), None);

        pieces.complete(10);
        assert_eq!(pieces.complete_prefix(), 0);
        pieces.reach(0, 4);
        assert_eq!(pieces.complete_prefix(), 4);
        pieces.reach(0, 2);
        assert_eq!(pieces.complete_prefix(), 4);
        pieces.complete(0);
        assert_eq!(pieces.complete_prefix(), 20);
        assert!(!pieces.is_complete());
        pieces.complete(20);
        assert_eq!(pieces.complete_prefix(), 25);
        assert!(pieces.is_complete());
    }

    #[test]
    fn resumes_from_an_offset() {
        let mut pieces = Pieces::new(15, 40, 10);
        assert_eq!(pieces.claim(), Some(15..25));
        pieces.complete(15);
        assert_eq!(pieces.complete_prefix(), 25);
    }

    #[test]
    fn early_end_drops_later_pieces() {
        let mut pieces = Pieces::new(0, 100, 10);
        let first = pieces.claim().unwrap();
        let second = pieces.claim().unwrap();
        pieces.end_at(14);
        assert_eq!(pieces.claim(), None);
        pieces.complete(first.start);
        assert!(!pieces.is_complete());
        pieces.complete(second.start);
        assert!(pieces.is_complete());
        assert_eq!(pieces.complete_prefix(), 14);
    }

    #[test]
    fn empty_files_are_complete() {
        let mut pieces = Pieces::new(0, 0, 10);
        assert_eq!(pieces.claim(), None);
        assert!(pieces.is_complete());
        assert_eq!(pieces.complete_prefix(), 0);
    }
}
