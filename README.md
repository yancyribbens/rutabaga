# Rutabaga Wallet

## Node Cheatsheet

Run Node
```
cargo run --bin node 91.192.45.220:38333 ~/rutabaga ~/rutabaga_keys ~/rutabaga_output_file ~/rutabaga_spent_file
```

Run Node with Info Messages
```
RUST_LOG=info cargo run --bin node 174.20.14.111:38333 ~/rutabaga ~/rutabaga_keys ~/rutabaga_output_file ~/rutabaga_spent_file
```

Run Node with Debug and Info Messages
```
RUST_LOG=debug cargo run --bin node 174.20.14.111:38333 ~/rutabaga ~/rutabaga_keys ~/rutabaga_output_file ~/rutabaga_spent_file
```

Run Node with Debug, Info and Trace Messages
```
RUST_LOG=trace cargo run --bin node 174.20.14.111:38333 ~/rutabaga ~/rutabaga_keys ~/rutabaga_output_file ~/rutabaga_spent_file
```

## Wallet Cheatsheet

Show Commands
```
cargo run --bin wallet
```

Generate Address
```
cargo run --bin wallet generate-address
```

Generate Address to File
```
cargo run --bin wallet generate-address --out /tmp/out
```

Print Keys File
```
cargo run --bin wallet print-keys-from-keys-file ~/rutabaga_keys
```

Show Outputs File
```
cargo run --bin wallet print-outputs ~/rutabaga_output_file
```

show ledger spent outputs
```
cargo run --bin wallet print-spent-outputs ~/rutabaga_spent_ledger
```

show ledger UTXOs
```
cargo run --bin wallet print-utxos ~/rutabaga_output_ledger ~/rutabaga_spent_ledger
```

print balance
```
cargo run --bin wallet print-balance ~/rutabaga_output_ledger ~/rutabaga_spent_ledger
```

# transaction

spend utxo 0,1,2 at 5 sats/vB
```
cargo run --bin wallet spend-utxos 0,1,2 ~/rutabaga_output_ledger ~/rutabaga_spent_ledger ~/rutabaga_keys recipient_addr 5
```
