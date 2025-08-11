# Ultra-Low-Latency Order Book (Rust) - Starter Skeleton

This skeleton contains a minimal, correct order-book + a bench client written in Rust using Tokio.

**Note:** This is intentionally a correctness-first starting point. Next steps to make it "ultra-low-latency":
- replace newline JSON messages with fixed-size binary protocol
- shard instruments -> single writer per shard
- remove Mutex and use per-shard single-threaded engine
- pre-allocate pools and avoid heap allocations in hot path
- use pinned threads and CPU affinity
- replace Tokio with lower-level IO (mio or tokio-uring) if needed

## To build

```bash
cargo build --release --bin engine
cargo build --release --bin bench_client
```

## To run locally

1. start engine:

```bash
cargo run --release --bin engine -- --port 4000
```

2. start bench client:

```bash
cargo run --release --bin bench_client -- --addr 127.0.0.1:4000 --n 10000 --instrument 1
```
