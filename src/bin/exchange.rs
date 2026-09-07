use orderbook::book::MultiAssetEngine;
use orderbook::gateway::{run_gateway, ConnectionEvent, InboundMessage, OutboundEvent};
use orderbook::metrics::LatencyHistogram;
use orderbook::risk::RiskEngine;
use orderbook::types::{Order, OrderType, RejectReason, ServerMessage, Side, Symbol};
use orderbook::wal::{WalReader, WalWriter};
use std::collections::HashMap;
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    println!("==================================================================");
    println!("   High-Performance Order Book Simulator (3-Stage Lock-Free Pipeline)   ");
    println!("==================================================================");

    // Bounded channels connecting pipeline stages
    let (order_tx, mut order_rx) = mpsc::channel::<InboundMessage>(100_000);
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<OutboundEvent>(100_000);
    let (conn_tx, mut conn_rx) = mpsc::channel::<ConnectionEvent>(1024);
    let (wal_tx, mut wal_rx) = mpsc::channel::<(Order, u64)>(100_000);

    // =========================================================================
    // BACKGROUND WAL LOGGER (Offloads synchronous disk I/O from matching engine)
    // =========================================================================
    thread::spawn(move || {
        let mut wal = WalWriter::new("wal.bin").expect("Failed to open durable WAL");
        let mut count = 0u64;
        while let Some((order, timestamp_ns)) = wal_rx.blocking_recv() {
            let _ = wal.write_order(&order, timestamp_ns);
            count += 1;
            if count % 1_000 == 0 || wal_rx.is_empty() {
                let _ = wal.sync();
            }
        }
        let _ = wal.sync();
    });

    // =========================================================================
    // STAGE 2: Core Matching Engine (Single-Writer, Dedicated Thread, ZERO MUTEX)
    // =========================================================================
    thread::spawn(move || {
        let mut engine = MultiAssetEngine::new();
        let mut risk = RiskEngine::new();

        let mut exec_hist = LatencyHistogram::new();
        let mut e2e_hist = LatencyHistogram::new();

        // Pre-fund test accounts
        // User 1: $1,000,000 USD + 1,000 BTC + 10,000 ETH + 100,000 SOL
        risk.deposit(1, 100_000_000_00, Symbol::Btc as u16, 1_000);
        risk.deposit(1, 0, Symbol::Eth as u16, 10_000);
        risk.deposit(1, 0, Symbol::Sol as u16, 100_000);

        // User 2: $1,000,000 USD + 1,000 BTC + 10,000 ETH + 100,000 SOL
        risk.deposit(2, 100_000_000_00, Symbol::Btc as u16, 1_000);
        risk.deposit(2, 0, Symbol::Eth as u16, 10_000);
        risk.deposit(2, 0, Symbol::Sol as u16, 100_000);

        // WAL Crash Recovery Replay
        let wal_path = "wal.bin";
        if std::path::Path::new(wal_path).exists() {
            match WalReader::read_all(wal_path) {
                Ok(records) => {
                    if !records.is_empty() {
                        println!("Replaying {} WAL records to reconstruct engine state...", records.len());
                        let mut replay_trades = Vec::with_capacity(256);
                        for (_header, order) in records {
                            let _ = engine.replay_order(order, &mut replay_trades, &mut risk);
                        }
                        println!("WAL replay complete. Next exchange order id: {}", engine.next_order_id());
                    }
                }
                Err(e) => {
                    eprintln!("Warning: Failed to read WAL: {:?}", e);
                }
            }
        }

        println!("Core Engine execution thread initialized (Lock-Free Hot Path).");

        let mut scratch_trades = Vec::with_capacity(256);
        let mut processed_count = 0u64;
        let start_time = Instant::now();

        while let Some(inbound) = order_rx.blocking_recv() {
            let t0 = Instant::now();
            let mut order = inbound.order;
            let order_type = order.get_order_type().unwrap_or(OrderType::Limit);

            if order_type == OrderType::Cancel {
                // 1. Authenticated Cancellation
                match engine.cancel_order(order.symbol_id, order.user_id, order.id) {
                    Ok(cancelled) => {
                        risk.unlock_cancelled_order_margin(&cancelled, cancelled.qty);

                        let now_ns = SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .map(|d| d.as_nanos() as u64)
                            .unwrap_or(0);
                        let _ = wal_tx.blocking_send((order, now_ns));

                        let _ = outbound_tx.blocking_send(OutboundEvent::ClientCancelOk {
                            client_id: inbound.client_id,
                            msg: ServerMessage::cancel_ok(
                                cancelled.id,
                                cancelled.client_order_id,
                                order.user_id,
                                order.symbol_id as u8,
                                cancelled.qty,
                            ),
                        });
                    }
                    Err(reason) => {
                        let _ = outbound_tx.blocking_send(OutboundEvent::ClientReject {
                            client_id: inbound.client_id,
                            msg: ServerMessage::reject(
                                order.id,
                                order.client_order_id,
                                order.user_id,
                                order.symbol_id as u8,
                                reason,
                            ),
                        });
                    }
                }
            } else {
                // 2. Pre-trade Margin Reservation
                if let Err(reason) = risk.lock_order_funds(&order) {
                    let _ = outbound_tx.blocking_send(OutboundEvent::ClientReject {
                        client_id: inbound.client_id,
                        msg: ServerMessage::reject(
                            order.id,
                            order.client_order_id,
                            order.user_id,
                            order.symbol_id as u8,
                            reason,
                        ),
                    });
                } else {
                    // Assign exchange order ID if not already assigned
                    if order.id == 0 {
                        order.id = engine.next_order_id();
                    }

                    let now_ns = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_nanos() as u64)
                        .unwrap_or(0);
                    let _ = wal_tx.blocking_send((order, now_ns));

                    let _ = outbound_tx.blocking_send(OutboundEvent::ClientAck {
                        client_id: inbound.client_id,
                        msg: ServerMessage::ack(
                            order.id,
                            order.client_order_id,
                            order.user_id,
                            order.symbol_id as u8,
                        ),
                    });

                    // 3. Deterministic Matching Engine Execution
                    scratch_trades.clear();
                    let match_res = engine.process(order, inbound.client_id, &mut scratch_trades);

                    match match_res {
                        Ok(report) => {
                            // Settle all matched trades
                            let mut total_executed_qty = 0u64;
                            let mut total_executed_cost = 0u64;

                            for trade in &scratch_trades {
                                risk.settle_trade(trade, order.price);
                                total_executed_qty += trade.qty;
                                total_executed_cost += trade.price.saturating_mul(trade.qty);

                                let taker_msg = ServerMessage::fill(
                                    trade.taker_order_id,
                                    trade.taker_client_order_id,
                                    trade.taker_user_id,
                                    trade.symbol_id as u8,
                                    trade.match_id,
                                    trade.price,
                                    trade.qty,
                                    false,
                                );

                                let maker_msg = ServerMessage::fill(
                                    trade.maker_order_id,
                                    trade.maker_client_order_id,
                                    trade.maker_user_id,
                                    trade.symbol_id as u8,
                                    trade.match_id,
                                    trade.price,
                                    trade.qty,
                                    true,
                                );

                                let _ = outbound_tx.blocking_send(OutboundEvent::TradeFill {
                                    taker_client_id: trade.taker_client_id,
                                    maker_client_id: trade.maker_client_id,
                                    taker_msg,
                                    maker_msg,
                                });
                            }

                            // If Self-Trade Prevention triggered, cancel remainder & unlock margin
                            if report.stp_triggered && report.remaining_qty > 0 {
                                risk.unlock_cancelled_order_margin(&order, report.remaining_qty);
                                let _ = outbound_tx.blocking_send(OutboundEvent::ClientReject {
                                    client_id: inbound.client_id,
                                    msg: ServerMessage::reject(
                                        order.id,
                                        order.client_order_id,
                                        order.user_id,
                                        order.symbol_id as u8,
                                        RejectReason::SelfTradePrevented,
                                    ),
                                });
                            }

                            // Market order unexecuted remainder refund
                            if order_type == OrderType::Market && !report.stp_triggered {
                                let side = order.get_side().unwrap_or(Side::Buy);
                                risk.refund_market_order_remainder(
                                    order.user_id,
                                    order.symbol_id,
                                    side,
                                    order.price,
                                    order.qty,
                                    total_executed_qty,
                                    total_executed_cost,
                                );
                            }

                            // Broadcast BBO update
                            if let Some(book) = engine.get_book(order.symbol_id) {
                                let (best_bid, best_ask) = book.get_bbo();
                                let (bid_p, bid_q) = best_bid.unwrap_or((0, 0));
                                let (ask_p, ask_q) = best_ask.unwrap_or((0, 0));

                                let bbo_msg = ServerMessage::bbo(
                                    order.symbol_id as u8,
                                    bid_p,
                                    bid_q,
                                    ask_p,
                                    ask_q,
                                );
                                let _ = outbound_tx.blocking_send(OutboundEvent::BboBroadcast { msg: bbo_msg });
                            }
                        }
                        Err(reason) => {
                            // Engine rejected order (e.g. EngineFull) -> unlock pre-locked margin
                            risk.unlock_cancelled_order_margin(&order, order.qty);
                            let _ = outbound_tx.blocking_send(OutboundEvent::ClientReject {
                                client_id: inbound.client_id,
                                msg: ServerMessage::reject(
                                    order.id,
                                    order.client_order_id,
                                    order.user_id,
                                    order.symbol_id as u8,
                                    reason,
                                ),
                            });
                        }
                    }
                }
            }

            // High-precision monotonic latency tracking
            let exec_ns = t0.elapsed().as_nanos() as u64;
            let e2e_ns = inbound.ingress_instant.elapsed().as_nanos() as u64;

            exec_hist.record(exec_ns);
            e2e_hist.record(e2e_ns);
            processed_count += 1;

            if processed_count % 10_000 == 0 {
                let elapsed = start_time.elapsed().as_secs_f64();
                println!("\n=== TELEMETRY UPDATE (After {} Orders) ===", processed_count);
                exec_hist.stats(elapsed).print_summary("Core Engine (Lock-Free)");
                e2e_hist.stats(elapsed).print_summary("End-to-End (Queue + Matching)");
            }
        }
    });

    // =========================================================================
    // STAGE 3: Egress Dispatcher (Event-driven, ZERO busy-spinning)
    // =========================================================================
    tokio::spawn(async move {
        let mut client_senders: HashMap<u64, mpsc::Sender<ServerMessage>> = HashMap::new();

        loop {
            tokio::select! {
                Some(event) = conn_rx.recv() => {
                    match event {
                        ConnectionEvent::Connected { client_id, sender } => {
                            client_senders.insert(client_id, sender);
                        }
                        ConnectionEvent::Disconnected { client_id } => {
                            client_senders.remove(&client_id);
                        }
                    }
                }
                Some(event) = outbound_rx.recv() => {
                    match event {
                        OutboundEvent::ClientAck { client_id, msg }
                        | OutboundEvent::ClientReject { client_id, msg }
                        | OutboundEvent::ClientCancelOk { client_id, msg } => {
                            if let Some(tx) = client_senders.get(&client_id) {
                                match tx.try_send(msg) {
                                    Ok(_) => {}
                                    Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                                        eprintln!("[EGRESS] Slow consumer client #{} buffer full. Evicting to prevent HoL blocking.", client_id);
                                        client_senders.remove(&client_id);
                                    }
                                    Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                                        client_senders.remove(&client_id);
                                    }
                                }
                            }
                        }
                        OutboundEvent::TradeFill {
                            taker_client_id,
                            maker_client_id,
                            taker_msg,
                            maker_msg,
                        } => {
                            if let Some(tx) = client_senders.get(&taker_client_id) {
                                match tx.try_send(taker_msg) {
                                    Ok(_) => {}
                                    Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                                        eprintln!("[EGRESS] Slow consumer client #{} buffer full. Evicting.", taker_client_id);
                                        client_senders.remove(&taker_client_id);
                                    }
                                    Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                                        client_senders.remove(&taker_client_id);
                                    }
                                }
                            }
                            if let Some(tx) = client_senders.get(&maker_client_id) {
                                match tx.try_send(maker_msg) {
                                    Ok(_) => {}
                                    Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                                        eprintln!("[EGRESS] Slow consumer client #{} buffer full. Evicting.", maker_client_id);
                                        client_senders.remove(&maker_client_id);
                                    }
                                    Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                                        client_senders.remove(&maker_client_id);
                                    }
                                }
                            }
                        }
                        OutboundEvent::BboBroadcast { msg } => {
                            let mut dead_clients = Vec::new();
                            for (&client_id, tx) in &client_senders {
                                match tx.try_send(msg) {
                                    Ok(_) => {}
                                    Err(_) => {
                                        dead_clients.push(client_id);
                                    }
                                }
                            }
                            for client_id in dead_clients {
                                client_senders.remove(&client_id);
                            }
                        }
                    }
                }
                else => break,
            }
        }
    });

    // =========================================================================
    // STAGE 1: Ingress Gateway (TCP Socket Inbound)
    // =========================================================================
    println!("Starting Bidirectional Gateway on 127.0.0.1:4000...");
    run_gateway("127.0.0.1:4000", order_tx, conn_tx).await?;

    Ok(())
}
