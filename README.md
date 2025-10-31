# Ultra-Low-Latency Order Book (Rust)

> A high-performance matching engine experiment focusing on correctness first, then latency.

![Order Book Demo](./assets/orderbook.png)

## 🚀 Overview

This project is a **low-latency order book and matching engine** written in Rust. It exposes a TCP binary protocol, receives orders, and processes matches in real-time.

The design starts simple (single threaded, async networking) and evolves toward HFT-grade upgrades.

## ✅ Current Features

* Binary wire protocol (13-byte messages)
* TCP server receiving orders
* Matching engine with BTreeMap order book
* Single-producer channel from network → engine
* Simple client to send BID/ASK orders
* Console logging for trades + book state

## 🧠 Protocol Format

| Field      | Type  | Meaning              |
| ---------- | ----- | -------------------- |
| Byte 0     | `u8`  | `0 = ASK`, `1 = BID` |
| Bytes 1–8  | `u64` | Price (LE)           |
| Bytes 9–12 | `u32` | Quantity (LE)        |

Total: **13 bytes per order**

## 🛠️ Build

```bash
cargo build --release
```

## ▶️ Run Engine (Server)

```bash
cargo run --bin orderbook
```

Expected output:

```
Server listening on 127.0.0.1:4000
```

## 💻 Run Client (Send Orders)

```bash
cargo run --bin client
```

Client sends BID/ASK messages and engine prints trades.

## 🧪 Example Output

```
Trade: 10 @ 500
Bids: {}
Asks: {}
```

## 📂 Project Structure

```
src/
 ├── book.rs        # Order book
 ├── connection.rs  # TCP listener
 ├── engine.rs      # Matching loop
 ├── main.rs        # Launch engine
 └── bin/
      └── client.rs # Test order sender
```

## 🧭 Roadmap

| Stage    | Goal                                             |
| -------- | ------------------------------------------------ |
| ✅ Now    | Functional matching engine + binary client       |
| ⏳ Next   | Stress client (1M orders/sec), ring buffer IPC   |
| 🔜 Later | Core-pinned threads, lock-free queues, snapshots |

## ❤️ Contributing

PRs welcome — especially for latency improvements.

## 📜 License

MIT

---

*For learning & performance tuning — not a trading system (yet).*
