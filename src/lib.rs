pub mod coin;
pub mod output_ledger;
pub mod spent_ledger;
pub mod wallet;

#[cfg(test)]
mod tests {
    use bitcoinkernel::{Transaction, TransactionRef};
    use std::fs;

    use crate::output_ledger;

    pub fn get_transaction(file: &str) -> Transaction {
        let file = format!("tests/transaction_{file}.bin");
        let tx_data = fs::read(file).unwrap();
        Transaction::new(&tx_data).unwrap()
    }

    pub fn outs_from_tx(tx: &Transaction) -> Vec<(usize, TransactionRef<'_>)> {
        let tx_ref = tx.as_ref();
        // the tx has only one output that's not the coinbase output
        vec![(1, tx_ref)]
    }

    pub fn write_tx_outs_to_file(tx: &Transaction, path: &std::path::Path) {
        let outs = outs_from_tx(tx);
        output_ledger::append(path, outs)
    }
}
