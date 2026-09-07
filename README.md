# Ultra-Low-Latency Order Book Simulator (v4)

> A deterministic, multi-asset high-frequency trading (HFT) exchange simulator written in Rust.
> Built with genuine low-latency architecture: zero-mutex single-writer matching engine thread, pre-allocated order & price-level arenas with intrusive doubly-linked queues, open-addressing hash tables with Knuth backward-shift deletion, two-state margin accounting with Self-Trade Prevention (STP), non-blocking asynchronous CRC32 write-ahead logging (WAL), and sub-bucket HDR latency histograms.

---

## 🚀 Architecture: 3-Stage Decoupled Pipeline

The system is decoupled into independent execution stages communicating via bounded asynchronous channels. **The core matching engine thread never acquires a mutex on the execution hot path and never stalls on synchronous disk I/O.**

```text
  ┌─────────────────────────────────────────────────────────────┐
  │              STAGE 1: Ingress Gateway (I/O)                 │
  │  - Accepts client TCP connections                           │
  │  - Frames 48-byte Order messages with Little-Endian safety   │
  │  - High-precision monotonic arrival timestamps (Instant)    │
  └──────────────────────────────┬──────────────────────────────┘
                                 │ Bounded Channel (100,000 slots)
                                 ▼
  ┌─────────────────────────────────────────────────────────────┐
  │           STAGE 2: Core Matching Engine (Single Thread)     │
  │  - ZERO MUTEX, single-writer state machine                   │
  │  - Pre-trade two-state margin locking (available vs locked) │
  │  - FastPriceIndex + Intrusive Doubly-Linked Price Levels    │
  │  - FastOrderIndex & FastClientIndex (backward-shift deletion)│
  │  - Self-Trade Prevention (Cancel-Aggressive STP)            │
  │  - Post-trade settlement & surplus budget unlocking         │
  │  - Asynchronous CRC32 Write-Ahead Log (WAL) offloading      │
  │  - Sub-bucket HDR Latency Histogram (1ns to 68.7s, 6.25% res)│
  └───────────────┬──────────────────────────────┬──────────────┘
                  │ Bounded WAL Channel          │ Bounded Egress Channel
                  ▼                              ▼
  ┌──────────────────────────────┐ ┌───────────────────────────┐
  │  Background WAL Journaler    │ │ STAGE 3: Egress Dispatcher│
  │  - Batch flushes to disk     │ │ - Tokio event-driven loop │
  │  - Calls sync() periodically │ │ - Routes Fills to Maker/  │
  │  - Zero hot-path disk stalls │ │   Taker with backpressure │
  └──────────────────────────────┘ └───────────────────────────┘
```

---

## ⚡ Key Engineering Features

### 1. Open-Addressing Price Level Index & Intrusive Arenas (`src/book.rs`)
- **Fast Price-to-Level Lookup:** `FastPriceIndex` provides strict $O(1)$ price level index resolution via 64-bit integer hashing and open addressing, eliminating dynamic heap map node allocations.
- **Intrusive Price Level Ladder:** Price levels form an intrusive doubly-linked list (`prev_level`, `next_level`) inside `PriceLevelArena`, maintaining strict price priority (`best_bid` / `best_ask` pointers) without tree rebalancing overhead.
- **Zero Heap Order Allocations:** All resting orders reside in pre-allocated intrusive arenas (`MAX_ARENA_ORDERS = 131,072`, `MAX_PRICE_LEVELS = 4,096`).
- **Strict $O(1)$ Order FIFO and Unlinking:** Intrusive pointers (`prev`, `next`) inside `OrderNode` enable $O(1)$ order queue insertions, cancellations, and FIFO head pops without array shifting.

### 2. Fast Hash Indices with Backward-Shift Deletion (`FastOrderIndex` & `FastClientIndex`)
- **Open-Addressing with Backward-Shift Deletion:** Employs Knuth backward-shift deletion on cancellations/fills to preserve probe chain continuity without tombstones.
- **$O(1)$ Client Order Cancellation:** `FastClientIndex` maps `(user_id, client_order_id)` directly to internal `exchange_order_id`, eliminating $O(N)$ linear scans during client-initiated cancellations.
- **Zero Ghost Entry Leaks:** Filled and cancelled orders are actively removed from the indices, maintaining optimal load factors (< 50%).

### 3. Financial Safety & Two-State Margin Accounting (`src/risk.rs`)
- **Available vs. Locked Balances:** Submitting orders locks available margin immediately (`usd_locked` / `asset_locked`). Orders cannot double-spend funds.
- **Market Order Surplus Unlocking:** Market orders lock a maximum spend budget. Any unexecuted budget remainder is refunded back to `usd_available` immediately upon matching completion.
- **Market Order Slippage Protection:** Market buy orders halt if best ask exceeds the slippage cap; market sell orders halt if best bid falls below the floor.
- **Price Improvement Refunds:** If a buyer bids \$110.00 and matches a resting ask at \$100.00, the \$10.00 difference is refunded to `usd_available` immediately upon settlement.
- **Self-Trade Prevention (STP):** Orders from the same `user_id` are prevented from self-matching. Matches executed prior to an STP condition are preserved, and the aggressive order's remainder is cleanly cancelled with reserved margin unlocked.
- **Authenticated Cancellations:** Only the `user_id` that submitted an order can cancel it. Authorized cancels unlock funds immediately.

### 4. Non-Blocking Write-Ahead Log (WAL) (`src/wal.rs`)
- **Binary Header & Framing:** File magic `WAL1` (`0x57414C31`) + version 1.
- **Table-Driven IEEE 802.3 CRC32:** Precomputed 256-entry lookup table processing every record against bitflips and partial writes.
- **Asynchronous Offloading:** Journaling is offloaded to a background logging worker via a bounded channel, keeping synchronous disk I/O off the matching engine thread.
- **Monotonic Sequence Continuity & Crash Recovery:** Scanning the WAL file on startup allows the matching engine to replay historical orders and resume sequence counting from `last_seq_id + 1`.

### 5. Sub-Bucket HDR Latency Histogram (`src/metrics.rs`)
- **High Dynamic Range (1ns to 68.7s):** 544 linear sub-buckets (16 sub-buckets per power-of-two octave).
- **Sub-Microsecond Resolution:** 6.25% relative precision across all latency ranges.
- **Zero Heap Allocations:** Fixed 2.1 KB array `[u32; 544]` residing permanently in L1/L2 CPU cache. $O(1)$ recording via CPU `leading_zeros` (`lzcnt`).

### 6. Bidirectional Protocol & Gateway (`src/types.rs` & `src/gateway.rs`)
- **Endianness-Safe Wire Format:** Explicit Little-Endian encoding/decoding (`to_le`, `from_le`).
- **Fixed 48-byte Order:** Client $\to$ Server framing with protocol magic `0x4F42`.
- **Fixed 64-byte ServerMessage:** Server $\to$ Client (cacheline-aligned). Dedicated fields for Acks, Rejects, Fills, Cancels, and BBO updates.
- **Event-Driven Egress Dispatcher:** Uses Tokio's asynchronous `select!` loop to route outbound events with zero CPU spinning.
- **Bounded Backpressure:** Client output queues are bounded (8,192 buffer slots) to prevent unbounded memory growth on slow consumers.

---

## 📂 Project Structure

```text
orderbook/
├── Cargo.toml              # Manifest with fat-LTO release profile
├── .gitignore              # Ignores binary database files (*.bin)
├── src/
│   ├── lib.rs              # Re-exports and public interface
│   ├── types.rs            # 48B Order, 64B ServerMessage, Trade, Enums
│   ├── book.rs             # OrderBook, PriceLevelArena, FastOrderIndex, MultiAssetEngine
│   ├── risk.rs             # Account margin locks, surplus refunds, STP unlocks
│   ├── wal.rs              # Table-driven CRC32 WAL with crash replay
│   ├── gateway.rs          # Bidirectional TCP ingress with bounded backpressure
│   ├── metrics.rs          # 544-bucket sub-bucket HDR Latency Histogram
│   └── bin/
│       ├── exchange.rs     # 3-stage exchange pipeline
│       ├── market_maker.rs # Multi-user trading client (STP-safe)
│       └── bench.rs        # Micro-benchmark execution binary
└── tests/
    └── book_tests.rs       # Comprehensive verification test suite
```

---

## 🛠️ Build & Run

### 1. Run the Test Suite
```bash
cargo test -- --nocapture
```

### 2. Build Release Binaries (with Fat LTO)
```bash
cargo build --release
```

### 3. Run the Exchange Server
In Terminal 1:
```bash
cargo run --release --bin exchange
```

### 4. Run the Market Maker Client
In Terminal 2:
```bash
cargo run --release --bin market_maker
```

### 5. Run the In-Memory Micro-Benchmark
```bash
cargo run --release --bin bench
```
