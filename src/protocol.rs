use bincode;
impl Message {
    pub fn to_binary(&self) -> Vec<u8> {
        bincode::serialize(self).expect("Failed to serialize Message to binary")
    }

    pub fn from_binary(bytes: &[u8]) -> Self {
        bincode::deserialize(bytes).expect("Failed to deserialize Message from binary")
    }
}
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Message {
    NewOrder(NewOrder),
    Cancel(Cancel),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NewOrder {
    pub order_id: u64,
    pub instrument: u32,
    pub side: Side,
    pub price: u64,
    pub qty: u64,
    pub ts_ns: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Cancel {
    pub order_id: u64,
    pub instrument: u32,
    pub ts_ns: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}
