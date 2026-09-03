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
