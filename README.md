# 🚀 Solana gRPC Integrity Checker

A Rust tool for validating **data consistency** between:

- **Solana Geyser gRPC** (Yellowstone Geyser gRPC plugin), and  
- **Solana RPC (`getBlock`)**

The checker streams **finalized blocks** from gRPC, fetches the same blocks from an RPC endpoint, and compares:

- Number of transactions (gRPC)  
- Number of transactions (RPC)

This reveals whether your gRPC plugin is missing transactions or emitting incorrect data.

---

## 🔍 Why This Tool?

Use this tool to **verify** the fidelity of your Solana data pipeline. It compares the number of transactions emitted by your Geyser gRPC plugin against the canonical `getBlock` response returned by RPC for each slot. This slot-level verification helps detect:
- missing transactions in the gRPC stream,
- differences introduced by plugin/LUT bugs,
- problems caused by versioned transactions or RPC configuration,
so you can quickly locate and fix data divergence before it affects analytics or downstream systems.

This tool allows you to **run live consistency checks** to ensure your gRPC block data is complete.

---

## ✨ Features

- Streams **finalized** blocks from gRPC  
- Fetches block via RPC using `getBlock`  
- Correctly handles versioned transactions (`maxSupportedTransactionVersion = 0`)  
- Logs:
  - Slot number  
  - gRPC tx count  
  - RPC tx count  
  - Delta (difference)  
- Produces a final summary report  
- Auto-retry gRPC stream on disconnect  
- Stop execution after a configured duration  
- Clean structured logs:

```
[MATCH] slot=380658540 grpc_tx=1143 rpc_tx=1143
[MISMATCH] slot=380658542 grpc_tx=1109 rpc_tx=1110 delta=-1
```

---

## ⚙️ Installation

Clone this repository:

```bash
git clone https://github.com/your-repo/solana-grpc-integrity-checker.git
cd solana-grpc-integrity-checker
```

Build:

```bash
cargo build --release
```

Binary will be available at:

```
target/release/solana_grpc_integrity_checker
```

---

## 🚀 Usage

### CLI Arguments

| Flag | Description |
|------|-------------|
| `--endpoint` | Geyser gRPC endpoint URL |
| `--x_token` | Authentication token for gRPC |
| `--rpc_uri` | Solana RPC endpoint |
| `--duration` | Duration in seconds (*default*: 60) |

### Example

```bash
./target/build/release/solana_grpc_integrity_checker --endpoint https://grpc.sgp.shyft.to --x_token YOUR_X_TOKEN     --rpc_uri https://rpc.sgp.shyft.to?api_key=YOUR_API_KEY --duration 30
```
or
```bash
cargo run -- --endpoint https://grpc.sgp.shyft.to --x_token YOUR_X_TOKEN     --rpc_uri https://rpc.sgp.shyft.to?api_key=YOUR_API_KEY --duration 30
```

The checker will run for the configured duration (default: 60 seconds) and produce logs like:

```
[2025-11-17T11:19:17Z INFO  solana_grpc_integrity_checker] MATCH slot 380664003 → gRPC Tx Count=1023 RPC Tx Count=1023
[2025-11-17T11:19:17Z INFO  solana_grpc_integrity_checker] MATCH slot 380664004 → gRPC Tx Count=1140 RPC Tx Count=1140
[2025-11-17T11:19:18Z INFO  solana_grpc_integrity_checker] MATCH slot 380664005 → gRPC Tx Count=1071 RPC Tx Count=1071
```

---

## 🧪 How It Works

1. Connects to Geyser gRPC using Yellowstone client.
2. Subscribes to **Finalized** block stream:
   ```rust
   include_transactions = true
   ```
3. For each block received:
   - Extract `gRPC transaction count`
   - Fetch same block from RPC via:
     ```json
     {
       "method": "getBlock",
       "params": [slot, { "maxSupportedTransactionVersion": 0 }]
     }
     ```
   - Extract `RPC transaction count`
4. Compare → print MATCH / MISMATCH  
5. After timeout → stop stream, print summary

---

## 📊 Final Report

At the end of the run:

```
============== FINAL REPORT ==============
Total Blocks Received: 10
Total gRPC Tx Count: 11639
Total RPC Tx Count: 11640
Mismatched Blocks: 1
===========================================
```

If mismatches exist, they are listed:

```
--- MISMATCH DETAILS ---
Slot 380664009 mismatch: gRPC Tx Count=1330 RPC Tx Count=1331
```

---

## 🛠 Technical Notes

- Uses `RpcBlockConfig` with:
  ```rust
  transaction_details: Signatures
  max_supported_transaction_version: 0
  ```
- Ensures compatibility with versioned transactions
- Uses `retry` with exponential backoff for gRPC reconnect
- Aborts retry loop when timer ends
- Uses structured logging (`log` + `env_logger`)

---

## 📄 Example Output

```
[2025-11-17T11:19:17Z INFO  solana_grpc_integrity_checker] MATCH slot 380664003 → gRPC Tx Count=1023 RPC Tx Count=1023
[2025-11-17T11:19:17Z INFO  solana_grpc_integrity_checker] MATCH slot 380664004 → gRPC Tx Count=1140 RPC Tx Count=1140
[2025-11-17T11:19:18Z INFO  solana_grpc_integrity_checker] MATCH slot 380664005 → gRPC Tx Count=1071 RPC Tx Count=1071
[2025-11-17T11:19:18Z INFO  solana_grpc_integrity_checker] MATCH slot 380664006 → gRPC Tx Count=1112 RPC Tx Count=1112
[2025-11-17T11:19:19Z INFO  solana_grpc_integrity_checker] MATCH slot 380664007 → gRPC Tx Count=1120 RPC Tx Count=1120
[2025-11-17T11:19:19Z INFO  solana_grpc_integrity_checker] MATCH slot 380664008 → gRPC Tx Count=1263 RPC Tx Count=1263
[2025-11-17T11:19:21Z INFO  solana_grpc_integrity_checker] MATCH slot 380664009 → gRPC Tx Count=1330 RPC Tx Count=1331
[2025-11-17T11:19:22Z INFO  solana_grpc_integrity_checker] MATCH slot 380664010 → gRPC Tx Count=1126 RPC Tx Count=1126
[2025-11-17T11:19:23Z INFO  solana_grpc_integrity_checker] MATCH slot 380664011 → gRPC Tx Count=1276 RPC Tx Count=1276
[2025-11-17T11:19:24Z INFO  solana_grpc_integrity_checker] MATCH slot 380664012 → gRPC Tx Count=1178 RPC Tx Count=1178
[2025-11-17T11:19:24Z INFO  solana_grpc_integrity_checker] Timer finished — stopping stream...

============== FINAL REPORT ==============
Total Blocks Received: 10
Total gRPC Tx Count: 11639
Total RPC Tx Count: 11640
Mismatched Blocks: 1

--- MISMATCH DETAILS ---
Slot 380664009 mismatch: gRPC Tx Count=1330 RPC Tx Count=1331
===========================================
```

---

## 🤝 Contributing

Contributions are welcome!  
Feel free to open PRs for:

- Additional metrics (latency, slot gaps, account updates)
- Prometheus exporter
- Support for multiple RPC endpoints

---