use std::sync::mpsc;
use crate::api::Order;


struct Scheduler {
    btc_tx: mpsc::Sender<Order>,
    eth_tx: mpsc::Sender<Order>,
    sol_tx: mpsc::Sender<Order>,
}

impl Scheduler {
    pub fn submit(&self, order: Order) {
      let tx = match order.letter {
            'B' => &self.btc_tx,
            'E' => &self.eth_tx,
            'S' => &self.sol_tx,
            _ => return,
        };

        tx.send(order).unwrap();
    }
}