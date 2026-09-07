use orderbook::book::{FastOrderIndex, MultiAssetEngine, OrderBook};
use orderbook::metrics::LatencyHistogram;
use orderbook::risk::RiskEngine;
use orderbook::types::{Order, RejectReason, Side, Symbol, Trade};
use orderbook::wal::{WalReader, WalWriter};
use std::fs;
use std::path::PathBuf;

#[test]
fn test_limit_order_matching_price_time_priority() {
    let mut book = OrderBook::new(Symbol::Btc);
    let mut trades = Vec::new();

    // User 1: Buy 10 lots @ $100
    let o1 = Order::new_limit(1, 1, Symbol::Btc, Side::Buy, 100_00, 10);
    let res1 = book.process(o1, 1, &mut trades).unwrap();
    assert_eq!(res1.executed_qty, 0);
    assert_eq!(res1.remaining_qty, 10);
    assert_eq!(trades.len(), 0);

    // User 2: Buy 5 lots @ $100 (same price, later time)
    let o2 = Order::new_limit(2, 2, Symbol::Btc, Side::Buy, 100_00, 5);
    let res2 = book.process(o2, 2, &mut trades).unwrap();
    assert_eq!(res2.executed_qty, 0);
    assert_eq!(res2.remaining_qty, 5);
    assert_eq!(trades.len(), 0);

    // User 3: Sell 12 lots @ $100 -> should fill User 1 first (10 lots), then User 2 (2 lots)
    let o3 = Order::new_limit(3, 3, Symbol::Btc, Side::Sell, 100_00, 12);
    let res3 = book.process(o3, 3, &mut trades).unwrap();
    assert_eq!(res3.executed_qty, 12);
    assert_eq!(res3.remaining_qty, 0);
    assert_eq!(trades.len(), 2);

    // First trade: Maker = User 1, 10 lots @ $100
    assert_eq!(trades[0].maker_order_id, 1);
    assert_eq!(trades[0].maker_user_id, 1);
    assert_eq!(trades[0].qty, 10);
    assert_eq!(trades[0].price, 100_00);

    // Second trade: Maker = User 2, 2 lots @ $100
    assert_eq!(trades[1].maker_order_id, 2);
    assert_eq!(trades[1].maker_user_id, 2);
    assert_eq!(trades[1].qty, 2);
    assert_eq!(trades[1].price, 100_00);

    // Remaining on book: User 2 should have 3 lots left at $100
    let (bbo_bid, bbo_ask) = book.get_bbo();
    assert_eq!(bbo_bid, Some((100_00, 3)));
    assert_eq!(bbo_ask, None);
}

#[test]
fn test_price_improvement_and_partial_fill() {
    let mut book = OrderBook::new(Symbol::Btc);
    let mut trades = Vec::new();

    // Maker: Sell 10 lots @ $95
    let o1 = Order::new_limit(1, 1, Symbol::Btc, Side::Sell, 95_00, 10);
    book.process(o1, 1, &mut trades).unwrap();

    // Taker: Buy 6 lots willing to pay up to $100
    let o2 = Order::new_limit(2, 2, Symbol::Btc, Side::Buy, 100_00, 6);
    let res = book.process(o2, 2, &mut trades).unwrap();

    assert_eq!(res.executed_qty, 6);
    assert_eq!(trades.len(), 1);
    // Trade executes at maker's price ($95), providing $5 price improvement to the buyer
    assert_eq!(trades[0].price, 95_00);
    assert_eq!(trades[0].qty, 6);
    assert_eq!(trades[0].buyer_id, 2);
    assert_eq!(trades[0].seller_id, 1);

    // Maker should have 4 lots remaining @ $95
    let (bbo_bid, bbo_ask) = book.get_bbo();
    assert_eq!(bbo_bid, None);
    assert_eq!(bbo_ask, Some((95_00, 4)));
}

#[test]
fn test_self_trade_prevention_immediate() {
    let mut book = OrderBook::new(Symbol::Btc);
    let mut trades = Vec::new();

    // User 1 rests a bid @ $100 for 5 lots
    let o1 = Order::new_limit(1, 1, Symbol::Btc, Side::Buy, 100_00, 5);
    book.process(o1, 1, &mut trades).unwrap();

    // User 1 places an ask @ $100 for 5 lots -> Immediate STP trigger
    let o2 = Order::new_limit(2, 1, Symbol::Btc, Side::Sell, 100_00, 5);
    let res = book.process(o2, 1, &mut trades).unwrap();

    assert_eq!(res.executed_qty, 0);
    assert_eq!(res.remaining_qty, 5);
    assert!(res.stp_triggered);
    assert_eq!(trades.len(), 0);

    // User 1's resting bid is preserved on the book
    let (bbo_bid, _) = book.get_bbo();
    assert_eq!(bbo_bid, Some((100_00, 5)));
}

#[test]
fn test_self_trade_prevention_after_partial_match() {
    let mut book = OrderBook::new(Symbol::Btc);
    let mut trades = Vec::new();

    // User 1 rests bid @ $100 for 5 lots
    let o1 = Order::new_limit(1, 1, Symbol::Btc, Side::Buy, 100_00, 5);
    book.process(o1, 1, &mut trades).unwrap();

    // User 2 rests bid @ $100 for 5 lots
    let o2 = Order::new_limit(2, 2, Symbol::Btc, Side::Buy, 100_00, 5);
    book.process(o2, 2, &mut trades).unwrap();

    // User 2 sends aggressive sell for 8 lots @ $100
    // Should match User 1 for 5 lots, then encounter User 2's own order and trigger STP
    let o3 = Order::new_limit(3, 2, Symbol::Btc, Side::Sell, 100_00, 8);
    let res = book.process(o3, 2, &mut trades).unwrap();

    assert_eq!(res.executed_qty, 5);
    assert_eq!(res.remaining_qty, 3);
    assert!(res.stp_triggered);

    // Exactly 1 trade matched with User 1
    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].maker_user_id, 1);
    assert_eq!(trades[0].taker_user_id, 2);
    assert_eq!(trades[0].qty, 5);

    // User 2's resting order of 5 lots remains intact on the book
    let (bbo_bid, _) = book.get_bbo();
    assert_eq!(bbo_bid, Some((100_00, 5)));
}

#[test]
fn test_market_order_with_slippage_protection() {
    let mut book = OrderBook::new(Symbol::Btc);
    let mut trades = Vec::new();

    // Asks at multiple levels
    book.process(Order::new_limit(1, 1, Symbol::Btc, Side::Sell, 100_00, 5), 1, &mut trades).unwrap();
    book.process(Order::new_limit(2, 1, Symbol::Btc, Side::Sell, 105_00, 5), 1, &mut trades).unwrap();
    trades.clear();

    // Buyer sends market order for 10 lots with slippage cap of $102
    // Must fill 5 lots @ $100, then STOP because next ask ($105) exceeds $102
    let market_order = Order::new_market(3, 2, Symbol::Btc, Side::Buy, 10, 102_00);
    let res = book.process(market_order, 2, &mut trades).unwrap();

    assert_eq!(res.executed_qty, 5);
    assert_eq!(res.remaining_qty, 5);
    assert!(!res.stp_triggered);
    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].price, 100_00);
    assert_eq!(trades[0].qty, 5);

    // Book still has the $105 ask
    let (_, bbo_ask) = book.get_bbo();
    assert_eq!(bbo_ask, Some((105_00, 5)));
}

#[test]
fn test_order_cancellation_and_unauthorized_rejection() {
    let mut book = OrderBook::new(Symbol::Btc);
    let mut trades = Vec::new();

    let o1 = Order::new_limit(10, 1, Symbol::Btc, Side::Buy, 100_00, 5);
    book.process(o1, 1, &mut trades).unwrap();

    // User 2 tries to cancel User 1's order -> Unauthorized
    let err = book.cancel_order(2, 10).unwrap_err();
    assert_eq!(err, RejectReason::Unauthorized);

    // User 1 cancels own order -> Success
    let cancelled = book.cancel_order(1, 10).unwrap();
    assert_eq!(cancelled.id, 10);
    assert_eq!(cancelled.qty, 5);

    // Book is now empty
    let (bbo_bid, bbo_ask) = book.get_bbo();
    assert_eq!(bbo_bid, None);
    assert_eq!(bbo_ask, None);

    // Subsequent cancel -> OrderNotFound
    let err2 = book.cancel_order(1, 10).unwrap_err();
    assert_eq!(err2, RejectReason::OrderNotFound);
}

#[test]
fn test_fast_order_index_collisions_and_deletions() {
    let mut index = FastOrderIndex::new();
    let num_orders = 1_000u64;

    for id in 1..=num_orders {
        index.insert(id, id, id * 10, 100_00 + id, id as u32, 0).unwrap();
    }
    assert_eq!(index.len(), num_orders as usize);

    // Verify all lookups succeed
    for id in 1..=num_orders {
        let entry = index.lookup(id).expect("Lookup must succeed");
        assert_eq!(entry.order_id, id);
        assert_eq!(entry.user_id, id * 10);
        assert_eq!(entry.price, 100_00 + id);
    }

    // Delete odd order IDs
    for id in (1..=num_orders).step_by(2) {
        let removed = index.remove(id).expect("Removal must succeed");
        assert_eq!(removed.order_id, id);
    }
    assert_eq!(index.len(), (num_orders / 2) as usize);

    // Verify deleted are None, even are still present
    for id in 1..=num_orders {
        if id % 2 == 1 {
            assert!(index.lookup(id).is_none());
        } else {
            let entry = index.lookup(id).expect("Even orders must remain");
            assert_eq!(entry.order_id, id);
        }
    }
}

#[test]
fn test_risk_engine_margin_lock_and_settlement() {
    let mut risk = RiskEngine::new();

    // User 1: $10,000 USD
    risk.deposit(1, 10_000_00, Symbol::Btc as u16, 0);
    // User 2: 5 BTC
    risk.deposit(2, 0, Symbol::Btc as u16, 5);

    // User 1 places buy order: 2 BTC @ $100 ($200 total)
    let buy_order = Order::new_limit(1, 1, Symbol::Btc, Side::Buy, 100_00, 2);
    risk.lock_order_funds(&buy_order).unwrap();

    let acc1 = risk.get_account(1).unwrap();
    assert_eq!(acc1.usd_available, 9_800_00);
    assert_eq!(acc1.usd_locked, 200_00);

    // User 2 places sell order: 2 BTC @ $90
    let sell_order = Order::new_limit(2, 2, Symbol::Btc, Side::Sell, 90_00, 2);
    risk.lock_order_funds(&sell_order).unwrap();

    let acc2 = risk.get_account(2).unwrap();
    assert_eq!(acc2.asset_available[Symbol::Btc as usize], 3);
    assert_eq!(acc2.asset_locked[Symbol::Btc as usize], 2);

    // Trade matches @ $90 (Maker = User 2, Taker = User 1 paying max $100)
    let trade = Trade {
        match_id: 1,
        maker_order_id: 2,
        taker_order_id: 1,
        maker_client_order_id: 2,
        taker_client_order_id: 1,
        maker_user_id: 2,
        taker_user_id: 1,
        maker_client_id: 2,
        taker_client_id: 1,
        buyer_id: 1,
        seller_id: 2,
        maker_side: Side::Sell as u8,
        symbol_id: Symbol::Btc as u16,
        price: 90_00,
        qty: 2,
    };

    risk.settle_trade(&trade, 100_00);

    // Buyer settlement check:
    // Locked was $200. Cost was 2 * $90 = $180.
    // Price improvement refund = $200 - $180 = $20 -> credited to available!
    let acc1_after = risk.get_account(1).unwrap();
    assert_eq!(acc1_after.usd_locked, 0);
    assert_eq!(acc1_after.usd_available, 9_820_00); // 9800 + 20
    assert_eq!(acc1_after.asset_available[Symbol::Btc as usize], 2);

    // Seller settlement check:
    let acc2_after = risk.get_account(2).unwrap();
    assert_eq!(acc2_after.asset_locked[Symbol::Btc as usize], 0);
    assert_eq!(acc2_after.usd_available, 180_00);
}

#[test]
fn test_risk_engine_insufficient_funds() {
    let mut risk = RiskEngine::new();
    risk.deposit(1, 50_00, Symbol::Btc as u16, 0); // $50

    // Try to buy 1 BTC @ $100 -> InsufficientFunds
    let o = Order::new_limit(1, 1, Symbol::Btc, Side::Buy, 100_00, 1);
    let res = risk.lock_order_funds(&o);
    assert_eq!(res, Err(RejectReason::InsufficientFunds));
}

#[test]
fn test_wal_write_and_recovery_replay() {
    let temp_wal_path = PathBuf::from("test_wal_temp.bin");
    let _ = fs::remove_file(&temp_wal_path);

    {
        let mut writer = WalWriter::new(&temp_wal_path).expect("Create WAL writer");
        for i in 1..=50 {
            let order = Order::new_limit(i, i * 10, Symbol::Btc, Side::Buy, 100_00 + i, 1);
            let seq = writer.write_order(&order, 1_000_000 + i).unwrap();
            assert_eq!(seq, i);
        }
        writer.sync().unwrap();
    }

    // Read back and verify integrity
    let records = WalReader::read_all(&temp_wal_path).expect("Read WAL records");
    assert_eq!(records.len(), 50);

    for (idx, (header, order)) in records.iter().enumerate() {
        let expected_id = (idx + 1) as u64;
        assert_eq!(header.seq_id, expected_id);
        assert_eq!(order.id, expected_id);
        assert_eq!(order.price, 100_00 + expected_id);
    }

    let _ = fs::remove_file(&temp_wal_path);
}

#[test]
fn test_latency_histogram_precision() {
    let mut hist = LatencyHistogram::new();

    hist.record(10);     // 10 ns
    hist.record(100);    // 100 ns
    hist.record(1_000);  // 1 µs
    hist.record(10_000); // 10 µs

    let report = hist.stats(1.0);
    assert_eq!(report.total_orders, 4);
    assert_eq!(report.min_ns, 10);
    assert_eq!(report.max_ns, 10_000);
    assert!(report.p50_ns > 0);
    assert!(report.p99_ns >= report.p50_ns);
}

#[test]
fn test_multi_asset_engine() {
    let mut engine = MultiAssetEngine::new();
    let mut trades = Vec::new();

    // Place BTC ask
    let o1 = Order::new_limit(1, 1, Symbol::Btc, Side::Sell, 65_000_00, 1);
    engine.process(o1, 1, &mut trades).unwrap();

    // Place ETH ask
    let o2 = Order::new_limit(2, 1, Symbol::Eth, Side::Sell, 3_500_00, 10);
    engine.process(o2, 1, &mut trades).unwrap();

    // Place SOL ask
    let o3 = Order::new_limit(3, 1, Symbol::Sol, Side::Sell, 150_00, 100);
    engine.process(o3, 1, &mut trades).unwrap();

    // Cross ETH book with Buy order
    let o4 = Order::new_limit(4, 2, Symbol::Eth, Side::Buy, 3_500_00, 4);
    let res = engine.process(o4, 2, &mut trades).unwrap();
    assert_eq!(res.executed_qty, 4);
    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].symbol_id, Symbol::Eth as u16);
    assert_eq!(trades[0].qty, 4);

    // Verify BTC and SOL books remain unaffected
    let btc_book = engine.get_book(Symbol::Btc as u16).unwrap();
    let (_, btc_ask) = btc_book.get_bbo();
    assert_eq!(btc_ask, Some((65_000_00, 1)));

    let sol_book = engine.get_book(Symbol::Sol as u16).unwrap();
    let (_, sol_ask) = sol_book.get_bbo();
    assert_eq!(sol_ask, Some((150_00, 100)));
}

#[test]
fn test_client_order_id_isolation_multiple_users() {
    let mut engine = MultiAssetEngine::new();
    let mut trades = Vec::new();

    // User 1 sends order with client_order_id = 100
    let mut o1 = Order::new_limit(100, 1, Symbol::Btc, Side::Buy, 50_000_00, 1);
    o1.id = 0; // Simulate raw network ingress where exchange assigns ID
    let res1 = engine.process(o1, 1, &mut trades).unwrap();
    assert_eq!(res1.executed_qty, 0);

    // User 2 ALSO sends order with identical client_order_id = 100
    let mut o2 = Order::new_limit(100, 2, Symbol::Btc, Side::Buy, 50_000_00, 2);
    o2.id = 0; // Simulate raw network ingress where exchange assigns ID
    let res2 = engine.process(o2, 2, &mut trades).unwrap();
    assert_eq!(res2.executed_qty, 0);

    // Both orders should rest on the book: total 3 BTC @ $50,000
    let btc_book = engine.get_book(Symbol::Btc as u16).unwrap();
    let (best_bid, _) = btc_book.get_bbo();
    assert_eq!(best_bid, Some((50_000_00, 3)));

    // User 1 cancels their order using client_order_id 100
    let cancelled1 = engine.cancel_order(Symbol::Btc as u16, 1, 100).expect("User 1 cancel must succeed");
    assert_eq!(cancelled1.user_id, 1);
    assert_eq!(cancelled1.qty, 1);

    // User 2's order of 2 BTC should still remain intact on the book!
    let btc_book = engine.get_book(Symbol::Btc as u16).unwrap();
    let (best_bid, _) = btc_book.get_bbo();
    assert_eq!(best_bid, Some((50_000_00, 2)));

    // User 2 cancels their order using client_order_id 100
    let cancelled2 = engine.cancel_order(Symbol::Btc as u16, 2, 100).expect("User 2 cancel must succeed");
    assert_eq!(cancelled2.user_id, 2);
    assert_eq!(cancelled2.qty, 2);

    // Book is now completely clear
    let btc_book = engine.get_book(Symbol::Btc as u16).unwrap();
    let (best_bid, _) = btc_book.get_bbo();
    assert_eq!(best_bid, None);
}

#[test]
fn test_engine_wal_replay_full_state_recovery() {
    let wal_path = PathBuf::from("test_recovery_wal.bin");
    let _ = fs::remove_file(&wal_path);

    // 1. Write an initial stream of orders and trades to WAL
    {
        let mut writer = WalWriter::new(&wal_path).expect("Create recovery WAL");

        // User 1 deposits and asks 5 BTC @ $60,000
        let ask = Order::new_limit(1, 1, Symbol::Btc, Side::Sell, 60_000_00, 5);
        writer.write_order(&ask, 1000).unwrap();

        // User 2 asks 10 ETH @ $3,000
        let eth_ask = Order::new_limit(2, 2, Symbol::Eth, Side::Sell, 3_000_00, 10);
        writer.write_order(&eth_ask, 2000).unwrap();

        // User 3 buys 2 BTC @ $60,000 (partially fills User 1)
        let buy = Order::new_limit(3, 3, Symbol::Btc, Side::Buy, 60_000_00, 2);
        writer.write_order(&buy, 3000).unwrap();

        writer.sync().unwrap();
    }

    // 2. Fresh startup: create brand-new engine and pre-fund accounts
    let mut fresh_engine = MultiAssetEngine::new();
    let mut fresh_risk = RiskEngine::new();

    fresh_risk.deposit(1, 0, Symbol::Btc as u16, 10);
    fresh_risk.deposit(2, 0, Symbol::Eth as u16, 20);
    fresh_risk.deposit(3, 500_000_00, Symbol::Btc as u16, 0);

    // 3. Replay WAL records into fresh state
    let records = WalReader::read_all(&wal_path).expect("Read recovery WAL");
    assert_eq!(records.len(), 3);

    let mut scratch_trades = Vec::new();
    for (_header, order) in records {
        fresh_engine.replay_order(order, &mut scratch_trades, &mut fresh_risk).unwrap();
    }

    // 4. Verify reconstructed state:
    // BTC book should have 3 lots remaining @ $60,000 (5 - 2 = 3)
    let btc_book = fresh_engine.get_book(Symbol::Btc as u16).unwrap();
    let (_, btc_ask) = btc_book.get_bbo();
    assert_eq!(btc_ask, Some((60_000_00, 3)));

    // ETH book should have 10 lots @ $3,000
    let eth_book = fresh_engine.get_book(Symbol::Eth as u16).unwrap();
    let (_, eth_ask) = eth_book.get_bbo();
    assert_eq!(eth_ask, Some((3_000_00, 10)));

    // Risk settlement verification:
    // User 3 bought 2 BTC for $120,000 -> has 2 BTC available, $500,000 - $120,000 = $380,000 available
    let acc3 = fresh_risk.get_account(3).unwrap();
    assert_eq!(acc3.asset_available[Symbol::Btc as usize], 2);
    assert_eq!(acc3.usd_available, 380_000_00);

    // Clean up temporary WAL
    let _ = fs::remove_file(&wal_path);
}

#[test]
fn test_engine_wal_deposit_and_self_contained_recovery() {
    let wal_path = PathBuf::from("test_deposit_wal.bin");
    let _ = fs::remove_file(&wal_path);

    // 1. Write deposits AND orders to WAL
    {
        let mut writer = WalWriter::new(&wal_path).expect("Create recovery WAL");

        // Record deposits directly in the WAL
        let dep1 = Order::new_deposit(1, 100_000_00, Symbol::Btc as u16, 10);
        writer.write_order(&dep1, 100).unwrap();

        let dep2 = Order::new_deposit(2, 500_000_00, Symbol::Btc as u16, 0);
        writer.write_order(&dep2, 200).unwrap();

        // User 1 sells 5 BTC @ 60,000
        let sell = Order::new_limit(1, 1, Symbol::Btc, Side::Sell, 60_000_00, 5);
        writer.write_order(&sell, 300).unwrap();

        // User 2 buys 2 BTC @ 60,000
        let buy = Order::new_limit(2, 2, Symbol::Btc, Side::Buy, 60_000_00, 2);
        writer.write_order(&buy, 400).unwrap();

        writer.sync().unwrap();
    }

    // 2. Self-contained recovery without ANY manual pre-funding!
    let mut recovered_engine = MultiAssetEngine::new();
    let mut recovered_risk = RiskEngine::new();

    let records = WalReader::read_all(&wal_path).expect("Read recovery WAL");
    assert_eq!(records.len(), 4);

    let mut scratch = Vec::new();
    for (_header, order) in records {
        recovered_engine.replay_order(order, &mut scratch, &mut recovered_risk).unwrap();
    }

    // 3. Verify state:
    // User 1 had 10 BTC, sold 2 BTC -> 8 BTC left (3 resting in ask level, 5 available), USD: +120,000 + 100,000 initial = 220,000
    let acc1 = recovered_risk.get_account(1).unwrap();
    assert_eq!(acc1.usd_available, 220_000_00);
    assert_eq!(acc1.asset_available[Symbol::Btc as usize], 5);
    assert_eq!(acc1.asset_locked[Symbol::Btc as usize], 3);

    // User 2 had $500,000, bought 2 BTC for $120,000 -> $380,000 left, 2 BTC available
    let acc2 = recovered_risk.get_account(2).unwrap();
    assert_eq!(acc2.usd_available, 380_000_00);
    assert_eq!(acc2.asset_available[Symbol::Btc as usize], 2);

    let _ = fs::remove_file(&wal_path);
}


