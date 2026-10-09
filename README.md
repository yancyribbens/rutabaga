# rutabaga

```bash
cargo run --bin wallet wallet
```

# CLI

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

# Node

run node with wallet env vars
```
rutabaga ~/rutabaga 13.140.133.208:38333 ~/rutabaga_keys ~/rutabaga_output_ledger ~/rutabaga_spent_ledger
```

# Ledger

show ledger outputs 
```
cargo run --bin wallet print-outputs ~/rutabaga_output_ledger
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

spend output 0 at 5 sats/vB
```
cargo run --bin wallet spend-utxo 0 ~/rutabaga_output_ledger ~/rutabaga_spent_ledger ~/rutabaga_keys recipient_addr 5
```

spend output 0,1,2 at 5 sats/vB
```
cargo run --bin wallet spend-utxos 0,1,2 ~/rutabaga_output_ledger ~/rutabaga_spent_ledger ~/rutabaga_keys recipient_addr 5
```

spend 3100 sats at 5 sats/vB using automatic coin-selection
```
cargo run --bin wallet spend 3100 ~/rutabaga_output_ledger ~/rutabaga_spent_ledger ~/rutabaga_keys recipient_addr 5
```
