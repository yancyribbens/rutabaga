# rutabaga

```bash
cargo run --bin cli wallet
```

# cli

print keys file
```
cargo run --bin cli wallet print-keys-from-keys-file ~/rutabaga_key
```

# node

run node with wallet env vars
```
RUTABAGA_KEY_FILE=~/rutabaga_keys RUTABAGA_LEDGER_FILE=~/rutabaga_output_ledger RUTABAGA_SPENT_FILE=~/rutabaga_spent_ledger cargo run --bin node --release -- --network signet
```

# ledger

show ledger balance
```
cargo run --bin cli wallet print-ledger ~/rutabaga_output_ledger ~/rutabaga_spent_ledger
```

# transaction

spend output 0
```
cargo run --bin cli wallet spend-output 0 ~/rutabaga_output_ledger ~/rutabaga_keys recipient_addr
```
