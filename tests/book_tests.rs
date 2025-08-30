use orderbook::book::OrderBook; 

#[test]
fn test_bid_without_match() {
    let mut ob = OrderBook::new();
    ob.bid(100, 10);
    assert_eq!(ob.bids.get(&100), Some(&10));
    assert!(ob.asks.is_empty());
}

#[test]
fn test_ask_without_match() {
    let mut ob = OrderBook::new();
    ob.ask(105, 5);
    assert_eq!(ob.asks.get(&105), Some(&5));
    assert!(ob.bids.is_empty());
}

#[test]
fn test_bid_matches_existing_ask() {
    let mut ob = OrderBook::new();
    ob.ask(100, 5);
    ob.bid(100, 3);

    assert_eq!(ob.asks.get(&100), Some(&2));
    assert!(ob.bids.is_empty());
}

#[test]
fn test_bid_consumes_entire_ask_and_rest_goes_in_book() {
    let mut ob = OrderBook::new();
    ob.ask(100, 5);
    ob.bid(100, 10);

    assert!(ob.asks.get(&100).is_none());
    assert_eq!(ob.bids.get(&100), Some(&5));
}

#[test]
fn test_ask_matches_existing_bid() {
    let mut ob = OrderBook::new();
    ob.bid(105, 7);
    ob.ask(100, 3);

    assert_eq!(ob.bids.get(&105), Some(&4));
    assert!(ob.asks.is_empty());
}

#[test]
fn test_multiple_price_levels() {
    let mut ob = OrderBook::new();

    ob.bid(100, 5);
    ob.bid(101, 3);
    ob.ask(102, 4);
    ob.ask(103, 6);

    assert_eq!(ob.bids.get(&100), Some(&5));
    assert_eq!(ob.bids.get(&101), Some(&3));
    assert_eq!(ob.asks.get(&102), Some(&4));
    assert_eq!(ob.asks.get(&103), Some(&6));
}
