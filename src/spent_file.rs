//! # SpentFile
//!
//! When an Output (TxOut) is used as input in a transaction on chain, it has been
//! spent (consumed).  Track when any outpoint that is tracked in the OutsFile was
//! found to be spent by writing it to file.  That way, the Outpoints not found in
//! this file are known to be spendable for future transactions.

use bitcoin::hashes::Hash;
use bitcoin::OutPoint;
use log::{debug, info};
use std::fs::File;
use std::io::Write;
use std::path::Path;

use std::io::prelude::*;

pub struct SpentFile {
    pub file: File,
}

impl SpentFile {
    // entry size:
    //  OutPoint { tx_id,: 32 bytes, vout: 4 bytes }
    //
    // memory layout:
    //  [32 bytes, 4 bytes] -> 36 bytes
    const ENTRY_SIZE: usize = 32 + 4;

    pub fn new(path: &Path) -> Self {
        debug!("Loading SpentFile {:?}", path);
        let file =
            std::fs::File::options().read(true).create(true).append(true).open(path).unwrap();
        Self { file }
    }

    pub fn len(&mut self) -> usize {
        self.read().len()
    }

    pub fn is_empty(&mut self) -> bool {
        self.read().len() == 0
    }

    // append a list of entries to file
    pub fn append_entries(&mut self, outpoints: Vec<OutPoint>) {
        outpoints.into_iter().for_each(|outpoint| {
            self.append_entry(outpoint);
        });
    }

    pub fn append_entry(&mut self, outpoint: OutPoint) {
        info!("write entry: {:?}", outpoint);
        let stored_outpoints = self.read();
        let duplicate = stored_outpoints.contains(&outpoint);

        if !duplicate {
            let mut buf = [0; Self::ENTRY_SIZE];
            let tx_id = outpoint.txid.to_byte_array();
            let vout = outpoint.vout.to_le_bytes();

            buf[0..32].copy_from_slice(&tx_id);
            buf[32..Self::ENTRY_SIZE].copy_from_slice(&vout);
            self.file.write_all(&buf).unwrap()
        }
    }

    // read a list of entries from file
    pub fn read(&mut self) -> Vec<OutPoint> {
        let mut buf = [0; Self::ENTRY_SIZE];
        // reset file pointer so we know its reading from start
        self.file.rewind().unwrap();

        let mut outs = vec![];
        loop {
            match self.file.read_exact(&mut buf) {
                Ok(_) => {
                    let out = Self::read_outpoint(&buf);
                    outs.push(out);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    break;
                }
                Err(_e) => {
                    // I/O error
                    break;
                }
            }
        }
        outs
    }

    // read:
    //  OutPoint { tx_id,: 32 bytes, vout: 4 bytes }
    fn read_outpoint(bytes: &[u8]) -> OutPoint {
        let mut txid = [0; 32];
        txid.copy_from_slice(&bytes[0..32]);
        let txid = bitcoin::Txid::from_byte_array(txid);

        let mut index_bytes = [0; 4];
        index_bytes.copy_from_slice(&bytes[32..Self::ENTRY_SIZE]);
        let index = u32::from_le_bytes(index_bytes);

        bitcoin::OutPoint::new(txid, index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile;

    use crate::tests::get_transaction;

    fn outpoints_from_tx(tx: &bitcoin::Transaction) -> Vec<OutPoint> {
        tx.input
            .iter()
            .map(|i| {
                let outpoint = i.previous_output;
                let vout = outpoint.vout;
                let txid = outpoint.txid;
                bitcoin::OutPoint { txid, vout }
            })
            .collect()
    }

    fn spent_file() -> SpentFile {
        let dir = tempfile::tempdir().unwrap();
        let spent_path = dir.path().join("spent_outpoints");
        SpentFile::new(&spent_path)
    }

    #[test]
    fn read_tx() {
        let mut spent_file = spent_file();

        let tx_2462 = tests::get_transaction("2462");
        let outpoints = outpoints_from_tx(&tx_2462);
        spent_file.append_entries(outpoints);

        let tx_8926 = tests::get_transaction("8926");
        let outpoints = outpoints_from_tx(&tx_8926);
        spent_file.append_entries(outpoints);

        let spent_outpoints = spent_file.read();

        let outpoint = &spent_outpoints[0];
        assert_eq!(
            outpoint.txid.to_byte_array(),
            tx_2462.input.first().unwrap().previous_output.txid.to_byte_array()
        );
        assert_eq!(outpoint.vout, 0);

        let outpoint = &spent_outpoints[1];
        assert_eq!(
            outpoint.txid.to_byte_array(),
            tx_8926.input.first().unwrap().previous_output.txid.to_byte_array()
        );
        assert_eq!(outpoint.vout, 2);
    }

    #[test]
    fn duplicate_tx() {
        let mut spent_file = spent_file();

        let tx_2462 = tests::get_transaction("2462");
        let outpoints = outpoints_from_tx(&tx_2462);
        spent_file.append_entries(outpoints.clone());
        spent_file.append_entries(outpoints.clone());

        assert_eq!(spent_file.len(), 1);
    }
}
