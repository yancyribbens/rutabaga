//! # BlockHashTracker
//!
//! The blockchain is a linear set of blocks each hashing to the one before.  This acts as a sort
//! of internal state for advertising to the p2p network which hashes are already known and
//! processed.  This type is also used when the program resumes to know what blocks have already
//! been processed.
//!
//! This type maintains both a file and a hashmap.  The file is written to every time a unique hash
//! is learned from the network and parsed.  The hashmap is loaded from file when the program
//! loads and acts as a working memory, since hashmaps are fast to index into and find duplicates.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

use bitcoin::hashes::Hash;
use bitcoin::BlockHash;
use log::{debug, info, trace};

const GENESIS_BLOCKHASH: &str = "00000008819873e925422c1ff0f99f7cc9bbb232af63a077a480a3633bee1ef6";

pub struct HashTracker {
    file: File,
    map: HashMap<BlockHash, i64>,
}

impl HashTracker {
    pub fn new(path: &Path) -> Self {
        debug!("Loading HashTracker {:?}", path);

        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .unwrap();

        let block_height = Self::compute_block_height(&file);
        info!("Start at blockheight {:?}", block_height);
        let mut map = HashMap::new();

        if block_height == 0 {
            info!("Write genesis blockhash to file {}", GENESIS_BLOCKHASH);
            Self::write_genesis_hash(&mut file, &mut map);
        } else {
            info!("Reading stored hashes into memory");
            Self::read_file_into_map(&mut file, &mut map, block_height);
        }

        info!("Done Loading HashTracker");
        Self { file, map }
    }

    fn compute_block_height(file: &File) -> i64 {
        let bytes = file.metadata().unwrap().len();
        assert!(bytes.is_multiple_of(32));
        let block_height = (bytes / 32) as i64;
        block_height
    }

    fn write_genesis_hash(file: &mut File, map: &mut HashMap<BlockHash, i64>) {
        let genesis_block_hash: BlockHash = GENESIS_BLOCKHASH.parse().unwrap();
        file.write_all(&genesis_block_hash.to_byte_array()).unwrap();
        map.insert(genesis_block_hash, 0i64);
    }

    fn read_file_into_map(file: &mut File, map: &mut HashMap<BlockHash, i64>, block_height: i64) {
        let mut buf = [0; 32];
        let mut read_count = 0;
        while read_count < block_height {
            file.read_exact(&mut buf).unwrap();
            let stored_hash: BlockHash = BlockHash::from_slice(&buf).unwrap();

            trace!(
                "reading hash from file at height: {:?} with hash {:?}",
                read_count,
                stored_hash
            );
            map.insert(stored_hash, read_count);
            read_count += 1;
        }
    }

    pub fn insert_and_store(&mut self, block_hash: BlockHash, height: i64) {
        let _ = self.map.entry(block_hash).insert_entry(height);
        self.file.write_all(&block_hash.to_byte_array()).unwrap()
    }

    pub fn contains_hash(&mut self, block_hash: &BlockHash) -> bool {
        self.map.contains_key(block_hash)
    }

    pub fn last_hash(&mut self) -> (i64, BlockHash) {
        let mut stored_hashes: Vec<_> = self.map.iter().collect();
        stored_hashes.sort_by(|a, b| b.1.cmp(a.1));
        let height = stored_hashes[0].1;
        let hash = stored_hashes[0].0;
        (*height, *hash)
    }

    pub fn get_hash_height(&mut self, hash: &BlockHash) -> Option<&i64> {
        self.map.get(hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_tracker() -> HashTracker {
        let dir = tempfile::tempdir().unwrap();
        let hash_path = dir.path().join("hashes_file");
        HashTracker::new(&hash_path)
    }

    #[test]
    fn new() {
        let mut tracker = new_tracker();
        let (height, hash) = tracker.last_hash();
        assert_eq!(height, 0);
        assert_eq!(hash, GENESIS_BLOCKHASH.parse().unwrap());
    }

    #[test]
    fn insert_and_store() {
        let mut tracker = new_tracker();
        let next_hash_str = "00000086d6b2636cb2a392d45edc4ec544a10024d30141c9adf4bfd9de533b53";
        let next_hash: BlockHash = next_hash_str.parse().unwrap();
        tracker.insert_and_store(next_hash, 1);
        assert_eq!((1, next_hash), tracker.last_hash());
    }

    #[test]
    fn reload() {
        let dir = tempfile::tempdir().unwrap();
        let hash_path = dir.path().join("hashes_file");
        let mut tracker = HashTracker::new(&hash_path);

        let next_hash_str = "00000086d6b2636cb2a392d45edc4ec544a10024d30141c9adf4bfd9de533b53";
        let next_hash: BlockHash = next_hash_str.parse().unwrap();
        tracker.insert_and_store(next_hash, 1);

        // reload
        let mut tracker = HashTracker::new(&hash_path);
        assert!(tracker.contains_hash(&next_hash));
        assert_eq!(*tracker.get_hash_height(&next_hash).unwrap(), 1i64);
    }

    #[test]
    fn unknown() {
        let mut tracker = new_tracker();
        let third_hash_str = "00000032bb881de703dcc968e8258080c7ed4a2933e3a35888fa0b2f75f36029";
        let third_hash: BlockHash = third_hash_str.parse().unwrap();
        assert_eq!(tracker.get_hash_height(&third_hash), None);
    }
}
