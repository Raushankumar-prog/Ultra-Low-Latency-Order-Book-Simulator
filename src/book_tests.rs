use orderbook::book::OrderBook;
use orderbook::protocol::{Cancel, NewOrder, Side};

// ...existing code...
fn test_limit_order_match() {
    let mut ob = OrderBook::new();
    let sell = NewOrder {
        order_id: 1,
        instrument: 0,
        side: Side::Sell,
        price: 100,
        qty: 10,
        ts_ns: 0,
    };
    ob.on_new_order(sell);
    let buy = NewOrder {
        order_id: 2,
        instrument: 0,
        side: Side::Buy,
        price: 100,
        qty: 5,
        ts_ns: 1,
    };
    let trades = ob.on_new_order(buy);
    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0], (100, 5, Some(1), 2));
}

// ...existing code...
fn test_cancel_order() {
    let mut ob = OrderBook::new();
    let order = NewOrder {
        order_id: 1,
        instrument: 0,
        side: Side::Buy,
        price: 101,
        qty: 10,
        ts_ns: 0,
    };
    ob.on_new_order(order);
    let cancel = Cancel {
        order_id: 1,
        instrument: 0,
        ts_ns: 1,
    };
    let cancelled = ob.on_cancel(cancel);
    assert!(cancelled);
}

// ...existing code...
fn test_partial_fill() {
    let mut ob = OrderBook::new();
    let sell = NewOrder {
        order_id: 1,
        instrument: 0,
        side: Side::Sell,
        price: 100,
        qty: 10,
        ts_ns: 0,
    };
    ob.on_new_order(sell);
    let buy = NewOrder {
        order_id: 2,
        instrument: 0,
        side: Side::Buy,
        price: 100,
        qty: 15,
        ts_ns: 1,
    };
    let trades = ob.on_new_order(buy);
    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0], (100, 10, Some(1), 2));
}
