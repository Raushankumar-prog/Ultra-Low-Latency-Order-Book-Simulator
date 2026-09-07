use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

pub const NUM_SYMBOLS: usize = 3;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Buy = 0,
    Sell = 1,
}

impl TryFrom<u8> for Side {
    type Error = ();
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Side::Buy),
            1 => Ok(Side::Sell),
            _ => Err(()),
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    Limit = 0,
    Market = 1,
    Cancel = 2,
    Deposit = 3,
}

impl TryFrom<u8> for OrderType {
    type Error = ();
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(OrderType::Limit),
            1 => Ok(OrderType::Market),
            2 => Ok(OrderType::Cancel),
            3 => Ok(OrderType::Deposit),
            _ => Err(()),
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Symbol {
    Btc = 0,
    Eth = 1,
    Sol = 2,
}

impl TryFrom<u16> for Symbol {
    type Error = ();
    fn try_from(v: u16) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Symbol::Btc),
            1 => Ok(Symbol::Eth),
            2 => Ok(Symbol::Sol),
            _ => Err(()),
        }
    }
}

impl TryFrom<u8> for Symbol {
    type Error = ();
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Symbol::Btc),
            1 => Ok(Symbol::Eth),
            2 => Ok(Symbol::Sol),
            _ => Err(()),
        }
    }
}

impl Symbol {
    pub fn as_str(&self) -> &'static str {
        match self {
            Symbol::Btc => "BTC/USD",
            Symbol::Eth => "ETH/USD",
            Symbol::Sol => "SOL/USD",
        }
    }
}

pub const ORDER_MAGIC: u16 = 0x4F42; // "OB"

/// Fixed 48-byte binary order message sent from Client to Exchange.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable, Serialize, Deserialize, PartialEq, Eq)]
pub struct Order {
    pub id: u64,               // Exchange-assigned order ID (or client ID for test/local use)
    pub price: u64,            // In cents / ticks ($100.00 = 10000)
    pub qty: u64,              // In base lots
    pub user_id: u64,
    pub client_order_id: u64,  // Client-assigned identifier
    pub side: u8,              // 0 = Buy, 1 = Sell
    pub order_type: u8,        // 0 = Limit, 1 = Market, 2 = Cancel
    pub symbol_id: u16,        // 0 = BTC, 1 = ETH, 2 = SOL
    pub magic: u16,            // Protocol framing magic (0x4F42)
    pub _padding: [u8; 2],
}

const _: () = assert!(std::mem::size_of::<Order>() == 48);

impl Default for Order {
    fn default() -> Self {
        Self {
            id: 0,
            price: 0,
            qty: 0,
            user_id: 0,
            client_order_id: 0,
            side: 0,
            order_type: 0,
            symbol_id: 0,
            magic: ORDER_MAGIC,
            _padding: [0; 2],
        }
    }
}

impl Order {
    pub fn new_limit(
        id: u64,
        user_id: u64,
        symbol: Symbol,
        side: Side,
        price: u64,
        qty: u64,
    ) -> Self {
        Self {
            id,
            price,
            qty,
            user_id,
            client_order_id: id,
            side: side as u8,
            order_type: OrderType::Limit as u8,
            symbol_id: symbol as u16,
            magic: ORDER_MAGIC,
            _padding: [0; 2],
        }
    }

    pub fn new_client_limit(
        client_order_id: u64,
        user_id: u64,
        symbol: Symbol,
        side: Side,
        price: u64,
        qty: u64,
    ) -> Self {
        Self {
            id: 0, // Assigned strictly by Exchange Matching Engine
            price,
            qty,
            user_id,
            client_order_id,
            side: side as u8,
            order_type: OrderType::Limit as u8,
            symbol_id: symbol as u16,
            magic: ORDER_MAGIC,
            _padding: [0; 2],
        }
    }

    pub fn new_market(
        id: u64,
        user_id: u64,
        symbol: Symbol,
        side: Side,
        qty: u64,
        max_spend_or_slippage_price: u64,
    ) -> Self {
        Self {
            id,
            price: max_spend_or_slippage_price,
            qty,
            user_id,
            client_order_id: id,
            side: side as u8,
            order_type: OrderType::Market as u8,
            symbol_id: symbol as u16,
            magic: ORDER_MAGIC,
            _padding: [0; 2],
        }
    }

    pub fn new_cancel(target_id: u64, user_id: u64, symbol: Symbol) -> Self {
        Self {
            id: target_id, // If known (or 0)
            price: 0,
            qty: 0,
            user_id,
            client_order_id: target_id,
            side: 0,
            order_type: OrderType::Cancel as u8,
            symbol_id: symbol as u16,
            magic: ORDER_MAGIC,
            _padding: [0; 2],
        }
    }

    pub fn new_deposit(
        user_id: u64,
        usd_amount: u64,
        symbol_id: u16,
        asset_amount: u64,
    ) -> Self {
        Self {
            id: 0,
            price: usd_amount,
            qty: asset_amount,
            user_id,
            client_order_id: 0,
            side: 0,
            order_type: OrderType::Deposit as u8,
            symbol_id,
            magic: ORDER_MAGIC,
            _padding: [0; 2],
        }
    }

    pub fn to_le(self) -> Self {
        Self {
            id: self.id.to_le(),
            price: self.price.to_le(),
            qty: self.qty.to_le(),
            user_id: self.user_id.to_le(),
            client_order_id: self.client_order_id.to_le(),
            side: self.side,
            order_type: self.order_type,
            symbol_id: self.symbol_id.to_le(),
            magic: self.magic.to_le(),
            _padding: self._padding,
        }
    }

    pub fn from_le(self) -> Self {
        Self {
            id: u64::from_le(self.id),
            price: u64::from_le(self.price),
            qty: u64::from_le(self.qty),
            user_id: u64::from_le(self.user_id),
            client_order_id: u64::from_le(self.client_order_id),
            side: self.side,
            order_type: self.order_type,
            symbol_id: u16::from_le(self.symbol_id),
            magic: u16::from_le(self.magic),
            _padding: self._padding,
        }
    }

    pub fn get_side(&self) -> Option<Side> {
        Side::try_from(self.side).ok()
    }

    pub fn get_order_type(&self) -> Option<OrderType> {
        OrderType::try_from(self.order_type).ok()
    }

    pub fn get_symbol(&self) -> Option<Symbol> {
        Symbol::try_from(self.symbol_id).ok()
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerMsgType {
    Accepted = 0,
    Rejected = 1,
    Filled = 2,
    Cancelled = 3,
    Bbo = 4,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejectReason {
    None = 0,
    InvalidOrder = 1,
    InsufficientFunds = 2,
    OrderNotFound = 3,
    Unauthorized = 4,
    EngineFull = 5,
    NoLiquidity = 6,
    SelfTradePrevented = 7,
}

/// Cacheline-aligned (64 bytes) binary message sent from Exchange to Client.
/// Clean, dedicated fields for fills, rejects, cancels, and BBO.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerMessage {
    pub msg_type: u8,
    pub symbol_id: u8,
    pub reject_code: u8,
    pub is_maker: u8,
    pub magic: u16,       // Protocol framing magic (0x4F42)
    pub _padding: [u8; 2],
    pub order_id: u64,    // Exchange-assigned order ID
    pub user_id: u64,
    pub match_id: u64,
    pub price: u64,       // Executed price or BBO bid price
    pub qty: u64,         // Executed qty or BBO bid qty
    pub extra_price: u64, // BBO ask price OR client_order_id
    pub extra_qty: u64,   // BBO ask qty
}

const _: () = assert!(std::mem::size_of::<ServerMessage>() == 64);

impl ServerMessage {
    #[inline(always)]
    pub fn client_order_id(&self) -> u64 {
        self.extra_price
    }

    #[inline(always)]
    pub fn ask_price(&self) -> u64 {
        self.extra_price
    }

    #[inline(always)]
    pub fn ask_qty(&self) -> u64 {
        self.extra_qty
    }

    pub fn ack(order_id: u64, client_order_id: u64, user_id: u64, symbol_id: u8) -> Self {
        Self {
            msg_type: ServerMsgType::Accepted as u8,
            symbol_id,
            reject_code: 0,
            is_maker: 0,
            magic: ORDER_MAGIC,
            _padding: [0; 2],
            order_id,
            user_id,
            match_id: 0,
            price: 0,
            qty: 0,
            extra_price: client_order_id,
            extra_qty: 0,
        }
    }

    pub fn reject(order_id: u64, client_order_id: u64, user_id: u64, symbol_id: u8, reason: RejectReason) -> Self {
        Self {
            msg_type: ServerMsgType::Rejected as u8,
            symbol_id,
            reject_code: reason as u8,
            is_maker: 0,
            magic: ORDER_MAGIC,
            _padding: [0; 2],
            order_id,
            user_id,
            match_id: 0,
            price: 0,
            qty: 0,
            extra_price: client_order_id,
            extra_qty: 0,
        }
    }

    pub fn fill(
        order_id: u64,
        client_order_id: u64,
        user_id: u64,
        symbol_id: u8,
        match_id: u64,
        price: u64,
        qty: u64,
        is_maker: bool,
    ) -> Self {
        Self {
            msg_type: ServerMsgType::Filled as u8,
            symbol_id,
            reject_code: 0,
            is_maker: if is_maker { 1 } else { 0 },
            magic: ORDER_MAGIC,
            _padding: [0; 2],
            order_id,
            user_id,
            match_id,
            price,
            qty,
            extra_price: client_order_id,
            extra_qty: 0,
        }
    }

    pub fn cancel_ok(order_id: u64, client_order_id: u64, user_id: u64, symbol_id: u8, unexecuted_qty: u64) -> Self {
        Self {
            msg_type: ServerMsgType::Cancelled as u8,
            symbol_id,
            reject_code: 0,
            is_maker: 0,
            magic: ORDER_MAGIC,
            _padding: [0; 2],
            order_id,
            user_id,
            match_id: 0,
            price: 0,
            qty: unexecuted_qty,
            extra_price: client_order_id,
            extra_qty: 0,
        }
    }

    pub fn bbo(
        symbol_id: u8,
        bid_price: u64,
        bid_qty: u64,
        ask_price: u64,
        ask_qty: u64,
    ) -> Self {
        Self {
            msg_type: ServerMsgType::Bbo as u8,
            symbol_id,
            reject_code: 0,
            is_maker: 0,
            magic: ORDER_MAGIC,
            _padding: [0; 2],
            order_id: 0,
            user_id: 0,
            match_id: 0,
            price: bid_price,
            qty: bid_qty,
            extra_price: ask_price,
            extra_qty: ask_qty,
        }
    }

    pub fn to_le(self) -> Self {
        Self {
            msg_type: self.msg_type,
            symbol_id: self.symbol_id,
            reject_code: self.reject_code,
            is_maker: self.is_maker,
            magic: self.magic.to_le(),
            _padding: self._padding,
            order_id: self.order_id.to_le(),
            user_id: self.user_id.to_le(),
            match_id: self.match_id.to_le(),
            price: self.price.to_le(),
            qty: self.qty.to_le(),
            extra_price: self.extra_price.to_le(),
            extra_qty: self.extra_qty.to_le(),
        }
    }

    pub fn from_le(self) -> Self {
        Self {
            msg_type: self.msg_type,
            symbol_id: self.symbol_id,
            reject_code: self.reject_code,
            is_maker: self.is_maker,
            magic: u16::from_le(self.magic),
            _padding: self._padding,
            order_id: u64::from_le(self.order_id),
            user_id: u64::from_le(self.user_id),
            match_id: u64::from_le(self.match_id),
            price: u64::from_le(self.price),
            qty: u64::from_le(self.qty),
            extra_price: u64::from_le(self.extra_price),
            extra_qty: u64::from_le(self.extra_qty),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trade {
    pub match_id: u64,
    pub maker_order_id: u64,
    pub taker_order_id: u64,
    pub maker_client_order_id: u64,
    pub taker_client_order_id: u64,
    pub maker_user_id: u64,
    pub taker_user_id: u64,
    pub maker_client_id: u64,
    pub taker_client_id: u64,
    pub buyer_id: u64,
    pub seller_id: u64,
    pub maker_side: u8,
    pub symbol_id: u16,
    pub price: u64,
    pub qty: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketDepthLevel {
    pub price: u64,
    pub total_qty: u64,
    pub order_count: u32,
}
