//! Parts of remote files kept in memory, so a program reading a file in small steps, or
//! reading it twice, asks the server once. Files are cut into blocks of one size; the blocks
//! used least recently go first when the budget is full.

use std::collections::HashMap;
use std::sync::Mutex;

use bytes::Bytes;

/// Large enough that a ranged request per block is cheap next to the data it brings, small
/// enough that a seek wastes little.
pub const BLOCK_SIZE: u64 = 1024 * 1024;

/// One block of one version of a file. A file's version changes whenever its contents may
/// have, so blocks of an older version are never served.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockKey {
    pub file: u64,
    pub version: u64,
    pub index: u64,
}

struct Kept {
    data: Bytes,
    last_used: u64,
}

#[derive(Default)]
struct State {
    blocks: HashMap<BlockKey, Kept>,
    bytes: usize,
    clock: u64,
}

pub struct BlockCache {
    state: Mutex<State>,
    budget: usize,
}

impl BlockCache {
    /// `budget` is the most memory the blocks may take, in bytes.
    pub fn new(budget: usize) -> Self {
        Self {
            state: Mutex::new(State::default()),
            budget,
        }
    }

    pub fn get(&self, key: BlockKey) -> Option<Bytes> {
        let mut state = self.state.lock().unwrap();
        state.clock += 1;
        let clock = state.clock;
        let kept = state.blocks.get_mut(&key)?;
        kept.last_used = clock;
        Some(kept.data.clone())
    }

    pub fn contains(&self, key: BlockKey) -> bool {
        self.state.lock().unwrap().blocks.contains_key(&key)
    }

    pub fn insert(&self, key: BlockKey, data: Bytes) {
        let mut state = self.state.lock().unwrap();
        state.clock += 1;
        let kept = Kept {
            last_used: state.clock,
            data,
        };
        let added = kept.data.len();
        if let Some(replaced) = state.blocks.insert(key, kept) {
            state.bytes -= replaced.data.len();
        }
        state.bytes += added;
        while state.bytes > self.budget {
            let oldest = state
                .blocks
                .iter()
                .filter(|(other, _)| **other != key)
                .min_by_key(|(_, kept)| kept.last_used)
                .map(|(key, _)| *key);
            let Some(oldest) = oldest else {
                break;
            };
            if let Some(removed) = state.blocks.remove(&oldest) {
                state.bytes -= removed.data.len();
            }
        }
    }

    /// Forgets every block of a file, whatever its version.
    pub fn forget_file(&self, file: u64) {
        let mut state = self.state.lock().unwrap();
        let mut freed = 0;
        state.blocks.retain(|key, kept| {
            let keep = key.file != file;
            if !keep {
                freed += kept.data.len();
            }
            keep
        });
        state.bytes -= freed;
    }

    pub fn bytes(&self) -> usize {
        self.state.lock().unwrap().bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(file: u64, index: u64) -> BlockKey {
        BlockKey {
            file,
            version: 0,
            index,
        }
    }

    #[test]
    fn the_least_recently_used_blocks_leave_first() {
        let cache = BlockCache::new(300);
        cache.insert(key(1, 0), Bytes::from(vec![0; 100]));
        cache.insert(key(1, 1), Bytes::from(vec![1; 100]));
        cache.insert(key(1, 2), Bytes::from(vec![2; 100]));
        assert!(cache.get(key(1, 0)).is_some());
        cache.insert(key(1, 3), Bytes::from(vec![3; 100]));
        assert_eq!(cache.bytes(), 300);
        assert!(cache.get(key(1, 0)).is_some());
        assert!(cache.get(key(1, 1)).is_none());
        assert!(cache.get(key(1, 2)).is_some());
        assert!(cache.get(key(1, 3)).is_some());
    }

    #[test]
    fn a_block_larger_than_the_budget_is_still_served_once() {
        let cache = BlockCache::new(10);
        cache.insert(key(1, 0), Bytes::from(vec![0; 100]));
        assert!(cache.get(key(1, 0)).is_some());
        cache.insert(key(1, 1), Bytes::from(vec![0; 100]));
        assert!(cache.get(key(1, 0)).is_none());
        assert_eq!(cache.bytes(), 100);
    }

    #[test]
    fn forgetting_a_file_keeps_the_others() {
        let cache = BlockCache::new(1000);
        cache.insert(key(1, 0), Bytes::from(vec![0; 100]));
        cache.insert(key(2, 0), Bytes::from(vec![0; 50]));
        cache.insert(
            BlockKey {
                file: 1,
                version: 7,
                index: 3,
            },
            Bytes::from(vec![0; 25]),
        );
        cache.forget_file(1);
        assert_eq!(cache.bytes(), 50);
        assert!(cache.get(key(2, 0)).is_some());
        assert!(!cache.contains(key(1, 0)));
    }

    #[test]
    fn replacing_a_block_does_not_count_it_twice() {
        let cache = BlockCache::new(1000);
        cache.insert(key(1, 0), Bytes::from(vec![0; 100]));
        cache.insert(key(1, 0), Bytes::from(vec![0; 40]));
        assert_eq!(cache.bytes(), 40);
    }
}
