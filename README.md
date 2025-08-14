# Ultra-Low-Latency Order Book (Rust)

This project is a production-grade, ultra-low-latency order book and matching engine written in Rust.

## Features

- Fixed-size binary protocol for all messages (no JSON in hot path)
- Multi-core sharded engine: each instrument is handled by a dedicated, single-writer thread
- Lock-free, per-shard order book logic (no global Mutex)
- Preallocated order books and custom fixed-size queues for all instruments
- Cache-line alignment for all hot data structures
- Worker threads are pinned to CPU cores for maximum cache locality
- Hybrid spin/yield queues for ultra-low-latency inter-thread communication
- Metrics collection (orders, cancels, trades) and logging for all key events
- Health check HTTP endpoint (on port 8081)
- Stubs for snapshot/recovery logic (ready for persistence)

## To build

```bash
cargo build --release --bin engine
cargo build --release --bin bench_client
```

## To run locally

1. Start engine:

```bash
cargo run --release --bin engine -- --port 4000
```

2. Start bench client:

```bash
cargo run --release --bin bench_client -- --addr 127.0.0.1:4000 --n 10000 --instrument 1
```

## Monitoring

- Health check: [http://localhost:8081](http://localhost:8081)
- Metrics: see logs or extend with Prometheus/simplemetrics exporter

## Extending

- Implement snapshot/recovery logic for persistence
- Add more advanced monitoring or alerting as needed
