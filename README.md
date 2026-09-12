# rutabaga

```bash
cargo run --bin cli wallet
```

# CLI

Generate Address
```
cargo run --bin cli wallet generate-address
```

Generate Address to File
```
cargo run --bin cli wallet generate-address --out /tmp/out
```

Print Keys File
```
cargo run --bin cli wallet print-keys-from-keys-file ~/rutabaga_keys
```

# Node

run node with wallet env vars
```
RUTABAGA_KEY_FILE=~/rutabaga_keys RUTABAGA_LEDGER_FILE=~/rutabaga_output_ledger RUTABAGA_SPENT_FILE=~/rutabaga_spent_ledger cargo run --bin node --release -- --network signet
```

# Ledger

show ledger outputs 
```
cargo run --bin cli wallet print-outputs ~/rutabaga_output_ledger
```

show ledger spent outputs
```
cargo run --bin cli wallet print-spent-outputs ~/rutabaga_spent_ledger
```

show ledger UTXOs
```
cargo run --bin cli wallet print-utxos ~/rutabaga_output_ledger ~/rutabaga_spent_ledger
```

print balance
```
cargo run --bin cli wallet print-balance ~/rutabaga_output_ledger ~/rutabaga_spent_ledger
```

# transaction

spend output 0 at 5 sats/vB
```
cargo run --bin cli wallet spend-utxo 0 ~/rutabaga_output_ledger ~/rutabaga_spent_ledger ~/rutabaga_keys recipient_addr 5
```
