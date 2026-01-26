use api::{Order, Trade};
use std::collections::BTreeMap;

pub struct MatchingEngine {
    pub bids: BTreeMap<u64, Vec<Order>>,
    pub asks: BTreeMap<u64, Vec<Order>>,
}

impl MatchingEngine {
    pub fn new() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
        }
    }

    pub fn process(&mut self, mut order: Order) -> Vec<Trade> {
        let mut trades = Vec::new();

        if order.side == 1 {
            while order.qty > 0 {
                match self.bids.iter_mut().next_back() {
                    Some((&best_bid_price, bid_queue)) => {
                        if best_bid_price < order.price {
                            break;
                        }

                        Self::match_against_queue(&mut order, bid_queue, &mut trades);

                        if bid_queue.is_empty() {
                            self.bids.remove(&best_bid_price);
                        }
                    }
                    None => break,
                }
            }
        } else {
            while order.qty > 0 {
                match self.asks.iter_mut().next() {
                    Some((&best_ask_price, ask_queue)) => {
                        if best_ask_price > order.price {
                            break;
                        }

                        Self::match_against_queue(&mut order, ask_queue, &mut trades);

                        if ask_queue.is_empty() {
                            self.asks.remove(&best_ask_price);
                        }
                    }
                    None => break,
                }
            }
        }

        if order.qty > 0 {
            let book = if order.side == 1 { &mut self.asks } else { &mut self.bids };
            book.entry(order.price).or_insert(Vec::new()).push(order);
        }

        trades
    }

    
    fn match_against_queue(incoming: &mut Order, queue: &mut Vec<Order>, trades: &mut Vec<Trade>) {
        while incoming.qty > 0 && !queue.is_empty() {
            let resting_order = &mut queue[0]; 

            let trade_qty = std::cmp::min(incoming.qty, resting_order.qty);
            
            let (buyer_id, seller_id) = if incoming.side == 0 {
                (incoming.user_id, resting_order.user_id)
            } else {
                (resting_order.user_id, incoming.user_id)
            };

            trades.push(Trade {
                buyer_id,
                seller_id,
                price: resting_order.price,
                qty: trade_qty,
            });

            incoming.qty -= trade_qty;
            resting_order.qty -= trade_qty;

            if resting_order.qty == 0 {
                queue.remove(0); 
            }
        }
    }
}
