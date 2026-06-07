# Ultra-Low-Latency Order Book Simulator (v3)

> A high-performance, event-driven multi-asset trading exchange written in Rust.
> Designed for correct HFT architecture: kernel-bypass networking, lock-free data structures, deterministic matching, and crash recovery.

## 🚀 Overview

This engine implements a **Staged Event-Driven Architecture (SEDA)** with kernel-bypass I/O and a parallel per-asset matching pipeline.

```
                    ┌─────────────────┐
                    │     Clients     │
                    └────────┬────────┘
                             │
                        QUIC / TCP
                             │
                             ▼
                ┌────────────────────────┐
                │ AF_XDP / DPDK Fetch    │
                │ Kernel Bypass Network  │
                └──────────┬─────────────┘
                           │
                           ▼
                ┌────────────────────────┐
                │ Lock-Free Ring Buffer  │
                └──────────┬─────────────┘
                           │
                           ▼
                ┌────────────────────────┐
                │ Transaction Validation │
                │ SIMD Verification      │
                └──────────┬─────────────┘
                           │
                           ▼
                ┌────────────────────────┐
                │ Sequencer              │
                │ Global Sequence IDs    │
                └──────────┬─────────────┘
                           │
              ┌────────────┴────────────┐
              │                         │
              ▼                         ▼

   ┌──────────────────┐      ┌──────────────────┐
   │ Deterministic    │      │ WAL / Event Log  │
   │ Replay Recorder  │      │ Append Only      │
   └──────────────────┘      └────────┬─────────┘
                                       │
                                       ▼
                ┌────────────────────────┐
                │ Dependency Scheduler   │
                │ Worker Pool            │
                └──────────┬─────────────┘
                           │
         ┌─────────────────┼─────────────────┐
         ▼                 ▼                 ▼

 ┌──────────────┐ ┌──────────────┐ ┌──────────────┐
 │ BTC Engine   │ │ ETH Engine   │ │ SOL Engine   │
 │ Lock-Free    │ │ Lock-Free    │ │ Lock-Free    │
 │ Order Book   │ │ Order Book   │ │ Order Book   │
 └──────┬───────┘ └──────┬───────┘ └──────┬───────┘
        │                │                │
        └────────────────┼────────────────┘
                         ▼

              ┌───────────────────────┐
              │ AccountDB             │
              │ Zero Allocation Path  │
              │ Memory Optimized      │
              └──────────┬────────────┘
                         │
            ┌────────────┼────────────┐
            ▼                         ▼

 ┌──────────────────┐     ┌──────────────────┐
 │ Snapshot Engine  │     │ Market Data      │
 │ Full Snapshot    │     │ Publisher        │
 │ Incremental Snap │     │ Trades / Depth   │
 └────────┬─────────┘     └────────┬─────────┘
          │                        │
          ▼                        ▼

 ┌──────────────────┐     ┌──────────────────┐
 │ Recovery System  │     │ QUIC / WebSocket │
 │ Snapshot + WAL   │     │ Broadcast        │
 └──────────────────┘     └──────────────────┘


 ┌──────────────────────────────────────────────┐
 │ Telemetry / Profiling                        │
 │ p50 / p95 / p99 latency                      │
 │ Throughput, Queue Depth                      │
 │ CPU Cycles, Cache Misses                     │
 │ Flamegraphs, eBPF Metrics                    │
 └──────────────────────────────────────────────┘


 ┌──────────────────────────────────────────────┐
 │ Optional Future Modules                      │
 │ -------------------------------------------- │
 │ GPU Signature Verification                   │
 │ GPU Risk Calculations                        │
 │ NUMA-Aware Sharding                          │
 │ Replication / Consensus Layer                │
 │ FPGA / SmartNIC Acceleration                 │
 └──────────────────────────────────────────────┘
```

## ✅ Features

**Networking**
- **Kernel-Bypass I/O:** AF_XDP / DPDK support — packets go from NIC directly to userspace, skipping the kernel network stack entirely.
- **Lock-Free Ring Buffer:** Ingress packets land in a wait-free SPSC/MPSC ring, eliminating contention between the I/O thread and the processing pipeline.
- **QUIC / TCP Transport:** Both connection-oriented transports supported for client connectivity.

**Processing Pipeline**
- **SIMD Transaction Validation:** Batch signature/checksum verification using CPU vector instructions for high throughput.
- **Global Sequencer:** Assigns monotonically increasing sequence IDs before any order touches the books, guaranteeing total ordering across all assets.
- **Dependency Scheduler + Worker Pool:** Detects order-level dependencies and dispatches independent orders to a pinned thread pool for parallel execution.

**Matching Engines**
- **Per-Asset Lock-Free Order Books:** BTC, ETH, and SOL each run on an isolated lock-free book — no cross-asset contention.
- **Price-Time Priority Matching:** Deterministic FIFO matching within price levels.
- **Zero-Allocation AccountDB:** Settlement reads and writes go through a memory-optimized account store with no heap allocation in the hot path.

**Persistence & Recovery**
- **Append-Only WAL / Event Log:** Every sequenced order is durably written before matching.
- **Deterministic Replay Recorder:** The full event stream is captured in replay-compatible format for post-trade analysis and crash recovery.
- **Snapshot Engine:** Both full and incremental snapshots of order book state. Recovery combines the latest snapshot with subsequent WAL entries.

**Market Data & Observability**
- **Market Data Publisher:** Real-time trade and depth feed produced after each match.
- **QUIC / WebSocket Broadcast:** Low-latency dissemination to downstream subscribers.
- **Telemetry:** p50/p95/p99 latency histograms, throughput counters, queue depths, CPU cycle counts, cache miss rates, flamegraph hooks, and eBPF-based kernel metrics.

## 🛠️ Build

```bash
cargo build --release
```

## ▶️ Run the System

You need two terminals to simulate a market.

### 1. Start the Exchange

```bash
cargo run --bin exchange
```

Starts the full pipeline: kernel-bypass ingress → sequencer → dependency scheduler → matching engines → settlement → WAL → market data publisher. Listens on `127.0.0.1:4000`.

### 2. Start the Market Maker

```bash
cargo run --bin market-maker
```

Simulated client that connects and places alternating Buy/Sell orders across supported assets to generate liquidity.

## 🧠 Protocol Format

All messages are **fixed 48 bytes** — zero-copy friendly, cast directly from the wire via `bytemuck`.

| Field        | Type      | Size | Description                        |
| ------------ | --------- | ---- | ---------------------------------- |
| `id`         | `u64`     | 8    | Unique Order ID                    |
| `price`      | `u64`     | 8    | Price in cents ($100.00 = 10000)   |
| `qty`        | `u64`     | 8    | Quantity in Satoshis               |
| `user_id`    | `u64`     | 8    | Trader ID                          |
| `company_id` | `u64`     | 8    | Clearing Firm ID                   |
| `side`       | `u8`      | 1    | `0` = Buy, `1` = Sell              |
| `_padding`   | `[u8; 7]` | 7    | Explicit padding to reach 48 bytes |

## 📂 Project Structure

```
.
├── bin/
│   ├── exchange/           # Main server — full pipeline
│   └── market-maker/       # Test client
├── crates/
│   ├── api/                # Shared types (Order, Trade, Account)
│   ├── gateway/            # AF_XDP / DPDK ingress + ring buffer
│   ├── validator/          # SIMD transaction verification
│   ├── sequencer/          # Global sequence ID assignment
│   ├── scheduler/          # Dependency analysis + worker pool dispatch
│   ├── matching-engine/    # Per-asset lock-free order books (BTreeMap)
│   ├── account-db/         # Zero-allocation settlement store
│   ├── persistence/        # WAL + deterministic replay recorder
│   ├── snapshot/           # Full + incremental snapshot engine
│   ├── market-data/        # Trade/depth feed publisher
│   ├── broadcast/          # QUIC / WebSocket dissemination
│   └── telemetry/          # Latency histograms, eBPF metrics, flamegraphs
```

## 🧭 Roadmap

| Stage | Goal                                                              | Status          |
| ----- | ----------------------------------------------------------------- | --------------- |
| ✅ v1  | Basic Matching Engine                                             | Done            |
| ✅ v2  | Risk Engine, Settlement, Persistence (WAL)                        | Done            |
| ✅ v3  | Kernel-Bypass (AF_XDP/DPDK), SIMD Validation, Sequencer, Multi-Asset Lock-Free Books, Snapshot + Recovery, Market Data Broadcast, Telemetry                          | **Current** |
| 🔜 v4  | GPU Signature Verification, GPU Risk Calculations                 | Next            |
| 🔜 v5  | NUMA-Aware Sharding, FPGA/SmartNIC Acceleration, Replication/Consensus | Future     |


