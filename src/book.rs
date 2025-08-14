use crate::protocol::*;
use std::collections::{BTreeMap, HashMap, VecDeque};

#[repr(align(64))]
#[derive(Debug)]
pub struct Order {
    pub order_id: u64,
    pub side: Side,
    pub price: u64,
    pub qty: u64,
}

#[repr(align(64))]
#[derive(Debug, Default)]
pub struct OrderBook {
    bids: BTreeMap<u64, VecDeque<Order>>,
    asks: BTreeMap<u64, VecDeque<Order>>,
    id_index: HashMap<u64, (Side, u64)>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            id_index: HashMap::new(),
        }
    }

    pub fn on_new_order(&mut self, no: NewOrder) -> Vec<(u64, u64, Option<u64>, u64)> {
        let mut trades = Vec::new();
        let mut remaining = no.qty;
        match no.side {
            Side::Buy => {
                let mut to_remove_prices = Vec::new();
                let asks_iter: Vec<u64> = self.asks.keys().cloned().collect();
                for &ask_price in &asks_iter {
                    if remaining == 0 {
                        break;
                    }
                    if ask_price > no.price {
                        break;
                    }
                    if let Some(q) = self.asks.get_mut(&ask_price) {
                        while let Some(resting) = q.front_mut() {
                            if remaining == 0 {
                                break;
                            }
                            let trade_qty = remaining.min(resting.qty);
                            trades.push((
                                ask_price,
                                trade_qty,
                                Some(resting.order_id),
                                no.order_id,
                            ));
                            resting.qty -= trade_qty;
                            remaining -= trade_qty;
                            if resting.qty == 0 {
                                let finished = q.pop_front();
                                if let Some(f) = finished {
                                    self.id_index.remove(&f.order_id);
                                }
                            }
                        }
                        if q.is_empty() {
                            to_remove_prices.push(ask_price);
                        }
                    }
                }
                for p in to_remove_prices {
                    self.asks.remove(&p);
                }
                if remaining > 0 {
                    let order = Order {
                        order_id: no.order_id,
                        side: Side::Buy,
                        price: no.price,
                        qty: remaining,
                    };
                    self.bids
                        .entry(no.price)
                        .or_insert_with(VecDeque::new)
                        .push_back(order);
                    self.id_index.insert(no.order_id, (Side::Buy, no.price));
                }
            }
            Side::Sell => {
                let mut to_remove_prices = Vec::new();
                let mut bid_prices: Vec<u64> = self.bids.keys().cloned().collect();
                bid_prices.sort_unstable_by(|a, b| b.cmp(a));
                for &bid_price in &bid_prices {
                    if remaining == 0 {
                        break;
                    }
                    if bid_price < no.price {
                        break;
                    }
                    if let Some(q) = self.bids.get_mut(&bid_price) {
                        while let Some(resting) = q.front_mut() {
                            if remaining == 0 {
                                break;
                            }
                            let trade_qty = remaining.min(resting.qty);
                            trades.push((
                                bid_price,
                                trade_qty,
                                Some(resting.order_id),
                                no.order_id,
                            ));
                            resting.qty -= trade_qty;
                            remaining -= trade_qty;
                            if resting.qty == 0 {
                                let finished = q.pop_front();
                                if let Some(f) = finished {
                                    self.id_index.remove(&f.order_id);
                                }
                            }
                        }
                        if q.is_empty() {
                            to_remove_prices.push(bid_price);
                        }
                    }
                }
                for p in to_remove_prices {
                    self.bids.remove(&p);
                }
                if remaining > 0 {
                    let order = Order {
                        order_id: no.order_id,
                        side: Side::Sell,
                        price: no.price,
                        qty: remaining,
                    };
                    self.asks
                        .entry(no.price)
                        .or_insert_with(VecDeque::new)
                        .push_back(order);
                    self.id_index.insert(no.order_id, (Side::Sell, no.price));
                }
            }
        }
        trades
    }

    pub fn on_cancel(&mut self, cancel: Cancel) -> bool {
        if let Some((side, price)) = self.id_index.remove(&cancel.order_id) {
            let map = match side {
                Side::Buy => &mut self.bids,
                Side::Sell => &mut self.asks,
            };
            if let Some(q) = map.get_mut(&price) {
                let mut idx = 0usize;
                while idx < q.len() {
                    if q[idx].order_id == cancel.order_id {
                        q.remove(idx);
                        return true;
                    }
                    idx += 1;
                }
            }
        }
        false
    }
}
