# Ultra-Low-Latency Order Book Simulator (v2)

> A high-performance, event-driven trading exchange written in Rust.
> Designed for correct High-Frequency Trading (HFT) architecture: sequential risk checks, deterministic matching, and crash recovery.

## 🚀 Overview

This engine implements a **Staged Event-Driven Architecture (SEDA)**. It separates concerns into distinct stages (Ingress -> Core -> Egress) to maximize throughput and safety.

In **v2**, we have introduced the "Gatekeeper" architecture:
1.  **Ingress (Gateway):** Generic fix-sized 48-byte packets over TCP.
2.  **Risk Engine (Core):** Pre-trade validation (Cash & Inventory checks) in the hot path.
3.  **Matching Engine (Core):** Price-Time priority matching returning `Trade` events.
4.  **Settlement (Core):** Atomic post-trade fund/asset transfers.
5.  **Persistence (WAL):** Write-Ahead Logging for zero-data-loss crash recovery.

## ✅ Features

*   **Zero-Copy Networking:** Uses `bytemuck` to cast TCP bytes directly to sized structs.
*   **Split-Thread Model:**
    *   `Gateway` runs on `tokio` (Async I/O).
    *   `Core` (Risk + Match + Settle) runs on a pinned OS Thread (CPU-bound).
*   **Risk Engine:**
    *   Enforces `Account.usd >= Cost` for Buys.
    *   Enforces `Account.btc >= Qty` for Sells.
    *   Prevents Self-Trading.
*   **Persistence:**
    *   Append-only Write-Ahead Log (`orders.wal`).
    *   Replay mechanism on startup.

## 🛠️ Build

```powershell
cargo build --release
```

## ▶️ Run the System

You need two terminals to simulate a market.

### 1. Start the Exchange
This process hosts the Matching Engine, Risk Engine, and TCP Gateway.
```powershell
cargo run --bin exchange
```
*Listens on `127.0.0.1:4000`*

### 2. Start the Market Maker
This simulated client connects to the exchange and places alternating Buy/Sell orders to generate liquidity.
```powershell
cargo run --bin market-maker
```

## 🧠 Protocol Format (v2)

All messages are **fixed 48 bytes** (Zero-Copy friendly).

| Field        | Type      | Size | Description |
| ------------ | --------- | ---- | ----------- |
| `id`         | `u64`     | 8    | Unique Order ID |
| `price`      | `u64`     | 8    | Price in cents ($100.00 = 10000) |
| `qty`        | `u64`     | 8    | Quantity in Satoshis |
| `user_id`    | `u64`     | 8    | Trader ID |
| `company_id` | `u64`     | 8    | Clearing Firm ID |
| `side`       | `u8`      | 1    | `0` = Buy, `1` = Sell |
| `_padding`   | `[u8; 7]` | 7    | Explicit padding to align to 48 bytes |

## 📂 Project Structure

```
.
├── bin/
│   ├── exchange/       # Main Server (Gateway + Core Thread)
│   └── market-maker/   # Test Client
├── crates/
│   ├── api/            # Shared Data Structures (Order, Trade)
│   ├── gateway/        # Tokio TCP Ingress
│   ├── matching-engine/# Order Book Logic (BTreeMap)
│   ├── risk/           # Pre-trade Checks & Settlement
│   ├── persistence/    # Write-Ahead Log (WAL)
│   └── ...             # (auth, market-data, etc.)
```

## 🧭 Roadmap

| Stage    | Goal                                             | Status |
| -------- | ------------------------------------------------ | ------ |
| ✅ v1     | Basic Matching Engine                            | Done   |
| ✅ v2     | Risk Engine, Settlement, Persistence (WAL)       | **Current** |
| ⏳ v3     | Market Data Broadcaster (UDP Multicast)          | Next   |
| 🔜 v4     | GPU Batch Signature Verification                 | Future |
