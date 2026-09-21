pub mod output_ledger;
pub mod spent_ledger;
pub mod wallet;

#[cfg(test)]
mod tests {
    use bitcoinkernel::Transaction;
    use std::fs;

    pub fn get_transaction(file: &str) -> Transaction {
        let file = format!("tests/transaction_{file}.bin");
        let tx_data = fs::read(file).unwrap();
        Transaction::new(&tx_data).unwrap()
    }
}
