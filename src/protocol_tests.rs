use ultra_low_latency_order_book::protocol::*;

#[test]
fn test_binary_round_trip_new_order() {
    let order = NewOrder {
        order_id: 42,
        instrument: 7,
        side: Side::Buy,
        price: 12345,
        qty: 100,
        ts_ns: 999,
    };
    let msg = Message::NewOrder(order.clone());
    let bin = msg.to_binary();
    let decoded = Message::from_binary(&bin);
    match decoded {
        Message::NewOrder(no) => assert_eq!(no, order),
        _ => panic!("Decoded message is not NewOrder"),
    }
}

#[test]
fn test_binary_round_trip_cancel() {
    let cancel = Cancel {
        order_id: 99,
        instrument: 3,
        ts_ns: 888,
    };
    let msg = Message::Cancel(cancel.clone());
    let bin = msg.to_binary();
    let decoded = Message::from_binary(&bin);
    match decoded {
        Message::Cancel(c) => assert_eq!(c, cancel),
        _ => panic!("Decoded message is not Cancel"),
    }
}
