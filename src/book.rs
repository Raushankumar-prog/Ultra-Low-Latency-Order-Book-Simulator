use std::collections::BTreeMap;


#[derive(Debug, Default)]
pub struct OrderBook {
    pub bids: BTreeMap<u64, u32>, 
   pub  asks: BTreeMap<u64, u32>, 
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
        }
    }

    pub fn bid(&mut self, price: u64, mut qty: u32) {
        while qty > 0 {
            if let Some((&ask_price, &ask_qty)) = self.asks.iter().next() {
                if price >= ask_price {
                    let trade_qty = qty.min(ask_qty);
                    println!("Trade: {} @ {}", trade_qty, ask_price);

                    if ask_qty > trade_qty {
                        self.asks.insert(ask_price, ask_qty - trade_qty);
                    } else {
                        self.asks.remove(&ask_price);
                    }

                    qty -= trade_qty;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        if qty > 0 {
            *self.bids.entry(price).or_insert(0) += qty;
        }
    }

    pub fn ask(&mut self, price: u64, mut qty: u32) {
        while qty > 0 {
            if let Some((&bid_price, &bid_qty)) = self.bids.iter().rev().next() {
                if price <= bid_price {
                    let trade_qty = qty.min(bid_qty);
                    println!("Trade: {} @ {}", trade_qty, bid_price);

                    if bid_qty > trade_qty {
                        self.bids.insert(bid_price, bid_qty - trade_qty);
                    } else {
                        self.bids.remove(&bid_price);
                    }

                    qty -= trade_qty;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        if qty > 0 {
            *self.asks.entry(price).or_insert(0) += qty;
        }
    }

    pub fn print(&self) {
        println!("Bids: {:?}", self.bids);
        println!("Asks: {:?}", self.asks);
    }
}
