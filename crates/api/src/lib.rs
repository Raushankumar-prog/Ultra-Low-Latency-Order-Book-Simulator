
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable, Serialize, Deserialize)]
pub struct Order {
    pub id: u64,
    pub price: u64,
    pub qty: u64,
    pub user_id: u64,
    pub company_id: u64,
    pub side: u8,
    pub _padding:[u8;7]
}

const _: () = assert!(std::mem::size_of::<Order>() == 48);

#[derive(Debug, Clone, Copy)]
pub struct Trade {
    pub buyer_id: u64,
    pub seller_id: u64,
    pub price: u64,
    pub qty: u64,
}