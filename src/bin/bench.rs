use orderbook::book::MultiAssetEngine;
use orderbook::metrics::LatencyHistogram;
use orderbook::risk::RiskEngine;
use orderbook::types::{Order, OrderType, Side, Symbol};
use std::time::Instant;

struct FastRng(u64);

impl FastRng {
    fn new(seed: u64) -> Self {
        Self(if seed == 0 { 0xdeadbeefcafebabe } else { seed })
    }

    #[inline(always)]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    #[inline(always)]
    fn gen_range(&mut self, range: std::ops::Range<usize>) -> usize {
        range.start + (self.next_u64() as usize % (range.end - range.start))
    }

    #[inline(always)]
    fn gen_range_u64(&mut self, range: std::ops::Range<u64>) -> u64 {
        range.start + (self.next_u64() % (range.end - range.start))
    }

    #[inline(always)]
    fn gen_bool(&mut self) -> bool {
        (self.next_u64() & 1) == 1
    }
}

fn generate_order_stream(count: usize) -> Vec<Order> {
    println!("Pre-generating {} realistic market orders...", count);
    let mut rng = FastRng::new(42);
    let mut orders = Vec::with_capacity(count);
    let mut active_quotes: Vec<(u64, Symbol, u64)> = Vec::with_capacity(count / 2);

    let symbols = [Symbol::Btc, Symbol::Eth, Symbol::Sol];
    let base_prices = [65_000_00u64, 3_500_00u64, 150_00u64];

    let mut next_order_id = 1u64;

    for _ in 0..count {
        let sym_idx = rng.gen_range(0..3);
        let symbol = symbols[sym_idx];
        let base_price = base_prices[sym_idx];

        let action = rng.gen_range(0..100);

        if action < 60 {
            // 60%: Limit Quote Maker (User 1 or 2)
            let user_id = if rng.gen_bool() { 1 } else { 2 };
            let side = if rng.gen_bool() { Side::Buy } else { Side::Sell };
            let spread_offset = rng.gen_range_u64(5..100);
            let price = if side == Side::Buy {
                base_price.saturating_sub(spread_offset)
            } else {
                base_price.saturating_add(spread_offset)
            };
            let qty = rng.gen_range_u64(1..10);

            let ord = Order::new_limit(next_order_id, user_id, symbol, side, price, qty);
            active_quotes.push((next_order_id, symbol, user_id));
            next_order_id += 1;
            orders.push(ord);
        } else if action < 80 && !active_quotes.is_empty() {
            // 20%: Cancellation of resting quote via O(1) client index
            let idx = rng.gen_range(0..active_quotes.len());
            let (target_id, target_symbol, user_id) = active_quotes.swap_remove(idx);
            orders.push(Order::new_cancel(target_id, user_id, target_symbol));
        } else if action < 95 {
            // 15%: Aggressive Crossing Limit/Market Order (Taker User 3)
            let user_id = 3;
            let side = if rng.gen_bool() { Side::Buy } else { Side::Sell };
            let qty = rng.gen_range_u64(1..5);
            let price = if side == Side::Buy {
                base_price + 200 // Aggressive crossing bid
            } else {
                base_price.saturating_sub(200) // Aggressive crossing ask
            };

            let ord = Order::new_limit(next_order_id, user_id, symbol, side, price, qty);
            next_order_id += 1;
            orders.push(ord);
        } else {
            // 5%: Potential STP event (User 1 or 2 crosses own spread)
            let user_id = 1;
            let side = Side::Buy;
            let price = base_price + 50;
            let qty = 2;
            let ord = Order::new_limit(next_order_id, user_id, symbol, side, price, qty);
            next_order_id += 1;
            orders.push(ord);
        }
    }

    println!("Generated {} orders in memory.", orders.len());
    orders
}

fn prefund(risk: &mut RiskEngine) {
    for uid in 1..=10 {
        risk.deposit(uid, 1_000_000_000_00, Symbol::Btc as u16, 1_000_000);
        risk.deposit(uid, 0, Symbol::Eth as u16, 1_000_000);
        risk.deposit(uid, 0, Symbol::Sol as u16, 10_000_000);
    }
}

fn main() {
    println!("==========================================================================");
    println!("   ULTRA-LOW-LATENCY MATCHING ENGINE MICROBENCHMARK (1,000,000 ORDERS)    ");
    println!("==========================================================================");

    let order_count = 1_000_000;
    let orders = generate_order_stream(order_count);

    // Warmup phase (50,000 orders)
    println!("\nWarming up instruction cache and branch predictors (50,000 orders)...");
    {
        let mut engine = MultiAssetEngine::new();
        let mut risk = RiskEngine::new();
        prefund(&mut risk);
        let mut scratch_trades = Vec::with_capacity(128);

        for order in orders.iter().take(50_000) {
            let order_type = order.get_order_type().unwrap_or(OrderType::Limit);
            if order_type == OrderType::Cancel {
                let _ = engine.cancel_order(order.symbol_id, order.user_id, order.id);
            } else {
                if risk.lock_order_funds(order).is_ok() {
                    scratch_trades.clear();
                    let _ = engine.process(*order, 0, &mut scratch_trades);
                }
            }
        }
    }
    println!("Warmup complete.");

    // Benchmark Run
    println!("\nExecuting 1,000,000 Headless Orders (Lock-Free In-Memory Hot Path)...");
    let mut engine = MultiAssetEngine::new();
    let mut risk = RiskEngine::new();
    prefund(&mut risk);

    let mut hist = LatencyHistogram::new();
    let mut scratch_trades = Vec::with_capacity(128);

    let mut limit_orders = 0u64;
    let mut cancels = 0u64;
    let mut total_trades = 0u64;
    let mut stp_count = 0u64;

    let total_start = Instant::now();

    for order in &orders {
        let t0 = Instant::now();
        let order_type = order.get_order_type().unwrap_or(OrderType::Limit);

        if order_type == OrderType::Cancel {
            match engine.cancel_order(order.symbol_id, order.user_id, order.id) {
                Ok(cancelled) => {
                    risk.unlock_cancelled_order_margin(&cancelled, cancelled.qty);
                    cancels += 1;
                }
                Err(_) => {}
            }
        } else {
            if risk.lock_order_funds(order).is_ok() {
                scratch_trades.clear();
                match engine.process(*order, 0, &mut scratch_trades) {
                    Ok(report) => {
                        limit_orders += 1;
                        if report.stp_triggered {
                            stp_count += 1;
                            if report.remaining_qty > 0 {
                                risk.unlock_cancelled_order_margin(order, report.remaining_qty);
                            }
                        }
                        for trade in &scratch_trades {
                            risk.settle_trade(trade, order.price);
                            total_trades += 1;
                        }
                    }
                    Err(_) => {}
                }
            }
        }

        let lat_ns = t0.elapsed().as_nanos() as u64;
        hist.record(lat_ns);
    }

    let total_elapsed = total_start.elapsed();
    let total_secs = total_elapsed.as_secs_f64();
    let throughput = (orders.len() as f64) / total_secs;

    let p50 = hist.percentile(50.0);
    let p90 = hist.percentile(90.0);
    let p95 = hist.percentile(95.0);
    let p99 = hist.percentile(99.0);
    let p99_9 = hist.percentile(99.9);
    let p99_99 = hist.percentile(99.99);

    println!("\n========================= BENCHMARK RESULTS =========================");
    println!("Total Orders Processed : {:>12}", orders.len());
    println!("Total Wall-Clock Time  : {:>12.4} s", total_secs);
    println!("Sustained Throughput   : {:>12.0} orders/sec ({:.2}M ops/sec)", throughput, throughput / 1_000_000.0);
    println!("---------------------------------------------------------------------");
    println!("Operations Breakdown:");
    println!("  - Limit Orders       : {:>12}", limit_orders);
    println!("  - Cancellations      : {:>12}", cancels);
    println!("  - Matched Trades     : {:>12}", total_trades);
    println!("  - STP Events         : {:>12}", stp_count);
    println!("---------------------------------------------------------------------");
    println!("HDR Latency Percentiles (Pre-Trade Margin + Matching + Settlement):");
    println!("  - Min Latency        : {:>8} ns   ({:.3} µs)", hist.stats(total_secs).min_ns, hist.stats(total_secs).min_ns as f64 / 1000.0);
    println!("  - p50 (Median)       : {:>8} ns   ({:.3} µs)", p50, p50 as f64 / 1000.0);
    println!("  - p90                : {:>8} ns   ({:.3} µs)", p90, p90 as f64 / 1000.0);
    println!("  - p95                : {:>8} ns   ({:.3} µs)", p95, p95 as f64 / 1000.0);
    println!("  - p99                : {:>8} ns   ({:.3} µs)", p99, p99 as f64 / 1000.0);
    println!("  - p99.9              : {:>8} ns   ({:.3} µs)", p99_9, p99_9 as f64 / 1000.0);
    println!("  - p99.99             : {:>8} ns   ({:.3} µs)", p99_99, p99_99 as f64 / 1000.0);
    println!("  - Max Latency        : {:>8} ns   ({:.3} µs)", hist.stats(total_secs).max_ns, hist.stats(total_secs).max_ns as f64 / 1000.0);
    println!("=====================================================================");
}
