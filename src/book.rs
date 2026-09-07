use crate::types::{MarketDepthLevel, Order, OrderType, RejectReason, Side, Symbol, Trade, NUM_SYMBOLS};
use serde::{Deserialize, Serialize};

pub const NULL_IDX: u32 = u32::MAX;
pub const MAX_ARENA_ORDERS: usize = 131_072;
pub const MAX_PRICE_LEVELS: usize = 4_096;
const FAST_MAP_CAPACITY: usize = 262_144;
const FAST_MAP_MASK: usize = FAST_MAP_CAPACITY - 1;
const FAST_PRICE_CAPACITY: usize = 8_192;
const FAST_PRICE_MASK: usize = FAST_PRICE_CAPACITY - 1;
const FAST_CLIENT_CAPACITY: usize = 262_144;
const FAST_CLIENT_MASK: usize = FAST_CLIENT_CAPACITY - 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct OrderNode {
    pub order: Order,
    pub prev: u32,
    pub next: u32,
    pub client_id: u64,
    pub active: bool,
}

impl Default for OrderNode {
    fn default() -> Self {
        Self {
            order: Order::default(),
            prev: NULL_IDX,
            next: NULL_IDX,
            client_id: 0,
            active: false,
        }
    }
}

/// Fixed-capacity, pre-allocated Order Arena.
/// Zero dynamic reallocations, zero heap allocations on the hot path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderArena {
    pub nodes: Vec<OrderNode>,
    pub free_head: u32,
    pub active_count: usize,
    pub capacity: usize,
}

impl OrderArena {
    pub fn new(capacity: usize) -> Self {
        let mut nodes = Vec::with_capacity(capacity);
        for i in 0..capacity {
            nodes.push(OrderNode {
                prev: NULL_IDX,
                next: if i + 1 < capacity { (i + 1) as u32 } else { NULL_IDX },
                client_id: 0,
                active: false,
                ..Default::default()
            });
        }
        Self {
            nodes,
            free_head: 0,
            active_count: 0,
            capacity,
        }
    }

    #[inline(always)]
    pub fn allocate(&mut self, order: Order, client_id: u64) -> Result<u32, RejectReason> {
        if self.free_head == NULL_IDX {
            return Err(RejectReason::EngineFull);
        }

        let idx = self.free_head;
        self.free_head = self.nodes[idx as usize].next;
        self.nodes[idx as usize] = OrderNode {
            order,
            prev: NULL_IDX,
            next: NULL_IDX,
            client_id,
            active: true,
        };
        self.active_count += 1;
        Ok(idx)
    }

    #[inline(always)]
    pub fn deallocate(&mut self, idx: u32) {
        if idx == NULL_IDX || idx as usize >= self.nodes.len() {
            return;
        }
        self.nodes[idx as usize].active = false;
        self.nodes[idx as usize].next = self.free_head;
        self.nodes[idx as usize].prev = NULL_IDX;
        self.free_head = idx;
        self.active_count = self.active_count.saturating_sub(1);
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PriceLevelNode {
    pub price: u64,
    pub head_order: u32,
    pub tail_order: u32,
    pub prev_level: u32, // Pointer to next better price level (or NULL_IDX)
    pub next_level: u32, // Pointer to next worse price level (or NULL_IDX)
    pub total_qty: u64,
    pub order_count: u32,
    pub next_free: u32,
    pub active: bool,
}

impl Default for PriceLevelNode {
    fn default() -> Self {
        Self {
            price: 0,
            head_order: NULL_IDX,
            tail_order: NULL_IDX,
            prev_level: NULL_IDX,
            next_level: NULL_IDX,
            total_qty: 0,
            order_count: 0,
            next_free: NULL_IDX,
            active: false,
        }
    }
}

/// Pre-allocated arena for PriceLevelNodes to eliminate all heap allocations for price levels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevelArena {
    pub nodes: Vec<PriceLevelNode>,
    pub free_head: u32,
}

impl PriceLevelArena {
    pub fn new(capacity: usize) -> Self {
        let mut nodes = Vec::with_capacity(capacity);
        for i in 0..capacity {
            nodes.push(PriceLevelNode {
                next_free: if i + 1 < capacity { (i + 1) as u32 } else { NULL_IDX },
                active: false,
                ..Default::default()
            });
        }
        Self {
            nodes,
            free_head: 0,
        }
    }

    #[inline(always)]
    pub fn allocate(&mut self, price: u64) -> Result<u32, RejectReason> {
        if self.free_head == NULL_IDX {
            return Err(RejectReason::EngineFull);
        }
        let idx = self.free_head;
        self.free_head = self.nodes[idx as usize].next_free;
        self.nodes[idx as usize] = PriceLevelNode {
            price,
            head_order: NULL_IDX,
            tail_order: NULL_IDX,
            prev_level: NULL_IDX,
            next_level: NULL_IDX,
            total_qty: 0,
            order_count: 0,
            next_free: NULL_IDX,
            active: true,
        };
        Ok(idx)
    }

    #[inline(always)]
    pub fn deallocate(&mut self, idx: u32) {
        if idx == NULL_IDX || idx as usize >= self.nodes.len() {
            return;
        }
        self.nodes[idx as usize].active = false;
        self.nodes[idx as usize].head_order = NULL_IDX;
        self.nodes[idx as usize].tail_order = NULL_IDX;
        self.nodes[idx as usize].prev_level = NULL_IDX;
        self.nodes[idx as usize].next_level = NULL_IDX;
        self.nodes[idx as usize].total_qty = 0;
        self.nodes[idx as usize].order_count = 0;
        self.nodes[idx as usize].next_free = self.free_head;
        self.free_head = idx;
    }
}

// Helpers on PriceLevelNode for order queue manipulation
impl PriceLevelNode {
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.head_order == NULL_IDX
    }

    #[inline(always)]
    pub fn push_order(&mut self, node_idx: u32, arena: &mut OrderArena) {
        arena.nodes[node_idx as usize].prev = self.tail_order;
        arena.nodes[node_idx as usize].next = NULL_IDX;

        if self.tail_order != NULL_IDX {
            arena.nodes[self.tail_order as usize].next = node_idx;
        } else {
            self.head_order = node_idx;
        }
        self.tail_order = node_idx;
        self.total_qty += arena.nodes[node_idx as usize].order.qty;
        self.order_count += 1;
    }

    #[inline(always)]
    pub fn pop_order(&mut self, arena: &mut OrderArena) -> Option<u32> {
        if self.head_order == NULL_IDX {
            return None;
        }
        let head_idx = self.head_order;
        let next_idx = arena.nodes[head_idx as usize].next;
        self.head_order = next_idx;
        if next_idx != NULL_IDX {
            arena.nodes[next_idx as usize].prev = NULL_IDX;
        } else {
            self.tail_order = NULL_IDX;
        }
        self.total_qty = self.total_qty.saturating_sub(arena.nodes[head_idx as usize].order.qty);
        self.order_count = self.order_count.saturating_sub(1);
        Some(head_idx)
    }

    #[inline(always)]
    pub fn unlink_order(&mut self, node_idx: u32, arena: &mut OrderArena) {
        let prev = arena.nodes[node_idx as usize].prev;
        let next = arena.nodes[node_idx as usize].next;

        if prev != NULL_IDX {
            arena.nodes[prev as usize].next = next;
        } else {
            self.head_order = next;
        }

        if next != NULL_IDX {
            arena.nodes[next as usize].prev = prev;
        } else {
            self.tail_order = prev;
        }

        self.total_qty = self.total_qty.saturating_sub(arena.nodes[node_idx as usize].order.qty);
        self.order_count = self.order_count.saturating_sub(1);
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FastIndexEntry {
    pub order_id: u64,
    pub client_order_id: u64,
    pub user_id: u64,
    pub price: u64,
    pub node_idx: u32,
    pub side: u8,
    pub occupied: bool,
}

impl Default for FastIndexEntry {
    fn default() -> Self {
        Self {
            order_id: 0,
            client_order_id: 0,
            user_id: 0,
            price: 0,
            node_idx: NULL_IDX,
            side: 0,
            occupied: false,
        }
    }
}

/// Open-Addressing Hash Table with backward-shift deletion.
/// Preserves probe chain continuity without tombstones.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FastOrderIndex {
    entries: Vec<FastIndexEntry>,
    count: usize,
}

impl Default for FastOrderIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl FastOrderIndex {
    pub fn new() -> Self {
        Self {
            entries: vec![FastIndexEntry::default(); FAST_MAP_CAPACITY],
            count: 0,
        }
    }

    #[inline(always)]
    fn hash(order_id: u64) -> usize {
        // Fast 64-bit integer mix
        let mut x = order_id.wrapping_mul(0x9E3779B97F4A7C15);
        x ^= x >> 30;
        x = x.wrapping_mul(0xBF58476D1CE4E5B9);
        x ^= x >> 27;
        (x as usize) & FAST_MAP_MASK
    }

    #[inline(always)]
    pub fn insert(
        &mut self,
        order_id: u64,
        client_order_id: u64,
        user_id: u64,
        price: u64,
        node_idx: u32,
        side: u8,
    ) -> Result<(), RejectReason> {
        if self.count >= (FAST_MAP_CAPACITY * 8) / 10 {
            return Err(RejectReason::EngineFull);
        }
        let mut idx = Self::hash(order_id);
        loop {
            let entry = &mut self.entries[idx];
            if !entry.occupied || entry.order_id == order_id {
                if !entry.occupied {
                    self.count += 1;
                }
                *entry = FastIndexEntry {
                    order_id,
                    client_order_id,
                    user_id,
                    price,
                    node_idx,
                    side,
                    occupied: true,
                };
                return Ok(());
            }
            idx = (idx + 1) & FAST_MAP_MASK;
        }
    }

    #[inline(always)]
    pub fn lookup(&self, order_id: u64) -> Option<FastIndexEntry> {
        let mut idx = Self::hash(order_id);
        loop {
            let entry = &self.entries[idx];
            if !entry.occupied {
                return None;
            }
            if entry.order_id == order_id {
                return Some(*entry);
            }
            idx = (idx + 1) & FAST_MAP_MASK;
        }
    }

    /// Backward-shift deletion preserves probe chain continuity when deleting an entry.
    pub fn remove(&mut self, order_id: u64) -> Option<FastIndexEntry> {
        let mut i = Self::hash(order_id);
        loop {
            let entry = self.entries[i];
            if !entry.occupied {
                return None;
            }
            if entry.order_id == order_id {
                let removed = entry;
                self.count = self.count.saturating_sub(1);

                // Backward-shift deletion
                let mut curr = i;
                let mut next = (curr + 1) & FAST_MAP_MASK;

                while self.entries[next].occupied {
                    let ideal = Self::hash(self.entries[next].order_id);
                    let can_shift = if curr < next {
                        ideal <= curr || ideal > next
                    } else {
                        ideal <= curr && ideal > next
                    };

                    if can_shift {
                        self.entries[curr] = self.entries[next];
                        curr = next;
                    }
                    next = (next + 1) & FAST_MAP_MASK;
                }

                self.entries[curr] = FastIndexEntry::default();
                return Some(removed);
            }
            i = (i + 1) & FAST_MAP_MASK;
        }
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

/// Open-Addressing Hash Table for fast O(1) Price -> PriceLevelNode lookup.
/// Completely eliminates BTreeMap heap allocations.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PriceIndexEntry {
    pub price: u64,
    pub level_idx: u32,
    pub occupied: bool,
}

impl Default for PriceIndexEntry {
    fn default() -> Self {
        Self {
            price: 0,
            level_idx: NULL_IDX,
            occupied: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FastPriceIndex {
    entries: Vec<PriceIndexEntry>,
    count: usize,
}

impl Default for FastPriceIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl FastPriceIndex {
    pub fn new() -> Self {
        Self {
            entries: vec![PriceIndexEntry::default(); FAST_PRICE_CAPACITY],
            count: 0,
        }
    }

    #[inline(always)]
    fn hash(price: u64) -> usize {
        let mut x = price.wrapping_mul(0x9E3779B97F4A7C15);
        x ^= x >> 30;
        x = x.wrapping_mul(0xBF58476D1CE4E5B9);
        x ^= x >> 27;
        (x as usize) & FAST_PRICE_MASK
    }

    #[inline(always)]
    pub fn insert(&mut self, price: u64, level_idx: u32) -> Result<(), RejectReason> {
        if self.count >= (FAST_PRICE_CAPACITY * 8) / 10 {
            return Err(RejectReason::EngineFull);
        }
        let mut idx = Self::hash(price);
        loop {
            let entry = &mut self.entries[idx];
            if !entry.occupied || entry.price == price {
                if !entry.occupied {
                    self.count += 1;
                }
                *entry = PriceIndexEntry {
                    price,
                    level_idx,
                    occupied: true,
                };
                return Ok(());
            }
            idx = (idx + 1) & FAST_PRICE_MASK;
        }
    }

    #[inline(always)]
    pub fn lookup(&self, price: u64) -> Option<u32> {
        let mut idx = Self::hash(price);
        loop {
            let entry = &self.entries[idx];
            if !entry.occupied {
                return None;
            }
            if entry.price == price {
                return Some(entry.level_idx);
            }
            idx = (idx + 1) & FAST_PRICE_MASK;
        }
    }

    pub fn remove(&mut self, price: u64) -> Option<u32> {
        let mut i = Self::hash(price);
        loop {
            let entry = self.entries[i];
            if !entry.occupied {
                return None;
            }
            if entry.price == price {
                let removed = entry.level_idx;
                self.count = self.count.saturating_sub(1);

                let mut curr = i;
                let mut next = (curr + 1) & FAST_PRICE_MASK;

                while self.entries[next].occupied {
                    let ideal = Self::hash(self.entries[next].price);
                    let can_shift = if curr < next {
                        ideal <= curr || ideal > next
                    } else {
                        ideal <= curr && ideal > next
                    };

                    if can_shift {
                        self.entries[curr] = self.entries[next];
                        curr = next;
                    }
                    next = (next + 1) & FAST_PRICE_MASK;
                }

                self.entries[curr] = PriceIndexEntry::default();
                return Some(removed);
            }
            i = (i + 1) & FAST_PRICE_MASK;
        }
    }
}

/// Composite Open-Addressing Table for strict O(1) (user_id, client_order_id) -> exchange_order_id.
/// Completely eliminates the fatal O(N) linear scan over 262,144 entries on cancellations.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ClientIndexEntry {
    pub user_id: u64,
    pub client_order_id: u64,
    pub exchange_order_id: u64,
    pub occupied: bool,
}

impl Default for ClientIndexEntry {
    fn default() -> Self {
        Self {
            user_id: 0,
            client_order_id: 0,
            exchange_order_id: 0,
            occupied: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FastClientIndex {
    entries: Vec<ClientIndexEntry>,
    count: usize,
}

impl Default for FastClientIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl FastClientIndex {
    pub fn new() -> Self {
        Self {
            entries: vec![ClientIndexEntry::default(); FAST_CLIENT_CAPACITY],
            count: 0,
        }
    }

    #[inline(always)]
    fn hash(user_id: u64, client_order_id: u64) -> usize {
        let mut x = user_id ^ client_order_id.rotate_left(32);
        x = x.wrapping_mul(0x9E3779B97F4A7C15);
        x ^= x >> 30;
        x = x.wrapping_mul(0xBF58476D1CE4E5B9);
        x ^= x >> 27;
        (x as usize) & FAST_CLIENT_MASK
    }

    #[inline(always)]
    pub fn insert(&mut self, user_id: u64, client_order_id: u64, exchange_order_id: u64) -> Result<(), RejectReason> {
        if self.count >= (FAST_CLIENT_CAPACITY * 8) / 10 {
            return Err(RejectReason::EngineFull);
        }
        let mut idx = Self::hash(user_id, client_order_id);
        loop {
            let entry = &mut self.entries[idx];
            if !entry.occupied || (entry.user_id == user_id && entry.client_order_id == client_order_id) {
                if !entry.occupied {
                    self.count += 1;
                }
                *entry = ClientIndexEntry {
                    user_id,
                    client_order_id,
                    exchange_order_id,
                    occupied: true,
                };
                return Ok(());
            }
            idx = (idx + 1) & FAST_CLIENT_MASK;
        }
    }

    #[inline(always)]
    pub fn lookup(&self, user_id: u64, client_order_id: u64) -> Option<u64> {
        let mut idx = Self::hash(user_id, client_order_id);
        loop {
            let entry = &self.entries[idx];
            if !entry.occupied {
                return None;
            }
            if entry.user_id == user_id && entry.client_order_id == client_order_id {
                return Some(entry.exchange_order_id);
            }
            idx = (idx + 1) & FAST_CLIENT_MASK;
        }
    }

    pub fn remove(&mut self, user_id: u64, client_order_id: u64) -> Option<u64> {
        let mut i = Self::hash(user_id, client_order_id);
        loop {
            let entry = self.entries[i];
            if !entry.occupied {
                return None;
            }
            if entry.user_id == user_id && entry.client_order_id == client_order_id {
                let removed = entry.exchange_order_id;
                self.count = self.count.saturating_sub(1);

                let mut curr = i;
                let mut next = (curr + 1) & FAST_CLIENT_MASK;

                while self.entries[next].occupied {
                    let ideal = Self::hash(self.entries[next].user_id, self.entries[next].client_order_id);
                    let can_shift = if curr < next {
                        ideal <= curr || ideal > next
                    } else {
                        ideal <= curr && ideal > next
                    };

                    if can_shift {
                        self.entries[curr] = self.entries[next];
                        curr = next;
                    }
                    next = (next + 1) & FAST_CLIENT_MASK;
                }

                self.entries[curr] = ClientIndexEntry::default();
                return Some(removed);
            }
            i = (i + 1) & FAST_CLIENT_MASK;
        }
    }
}

/// Structured report returned from matching execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionReport {
    pub executed_qty: u64,
    pub remaining_qty: u64,
    pub stp_triggered: bool,
}

/// Genuinely zero-heap-allocation OrderBook.
/// Intrusive doubly-linked price ladder inside PriceLevelArena + FastPriceIndex.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    pub symbol: Symbol,
    pub best_bid: u32,
    pub best_ask: u32,
    pub price_index: FastPriceIndex,
    pub level_arena: PriceLevelArena,
    pub order_arena: OrderArena,
    pub order_index: FastOrderIndex,
    pub client_order_index: FastClientIndex,
    trade_id_counter: u64,
    order_id_counter: u64,
}

impl OrderBook {
    pub fn new(symbol: Symbol) -> Self {
        Self {
            symbol,
            best_bid: NULL_IDX,
            best_ask: NULL_IDX,
            price_index: FastPriceIndex::new(),
            level_arena: PriceLevelArena::new(MAX_PRICE_LEVELS),
            order_arena: OrderArena::new(MAX_ARENA_ORDERS),
            order_index: FastOrderIndex::new(),
            client_order_index: FastClientIndex::new(),
            trade_id_counter: 1,
            order_id_counter: 1,
        }
    }

    pub fn next_order_id(&mut self) -> u64 {
        let id = self.order_id_counter;
        self.order_id_counter += 1;
        id
    }

    #[inline(always)]
    fn link_price_level(&mut self, level_idx: u32, is_sell: bool) {
        let price = self.level_arena.nodes[level_idx as usize].price;
        if is_sell {
            // Asks: sorted ascending (lowest price at head: best_ask)
            if self.best_ask == NULL_IDX {
                self.best_ask = level_idx;
                self.level_arena.nodes[level_idx as usize].prev_level = NULL_IDX;
                self.level_arena.nodes[level_idx as usize].next_level = NULL_IDX;
            } else if price < self.level_arena.nodes[self.best_ask as usize].price {
                let old_head = self.best_ask;
                self.level_arena.nodes[level_idx as usize].next_level = old_head;
                self.level_arena.nodes[level_idx as usize].prev_level = NULL_IDX;
                self.level_arena.nodes[old_head as usize].prev_level = level_idx;
                self.best_ask = level_idx;
            } else {
                let mut curr = self.best_ask;
                while self.level_arena.nodes[curr as usize].next_level != NULL_IDX
                    && self.level_arena.nodes[self.level_arena.nodes[curr as usize].next_level as usize].price < price
                {
                    curr = self.level_arena.nodes[curr as usize].next_level;
                }
                let next = self.level_arena.nodes[curr as usize].next_level;
                self.level_arena.nodes[curr as usize].next_level = level_idx;
                self.level_arena.nodes[level_idx as usize].prev_level = curr;
                self.level_arena.nodes[level_idx as usize].next_level = next;
                if next != NULL_IDX {
                    self.level_arena.nodes[next as usize].prev_level = level_idx;
                }
            }
        } else {
            // Bids: sorted descending (highest price at head: best_bid)
            if self.best_bid == NULL_IDX {
                self.best_bid = level_idx;
                self.level_arena.nodes[level_idx as usize].prev_level = NULL_IDX;
                self.level_arena.nodes[level_idx as usize].next_level = NULL_IDX;
            } else if price > self.level_arena.nodes[self.best_bid as usize].price {
                let old_head = self.best_bid;
                self.level_arena.nodes[level_idx as usize].next_level = old_head;
                self.level_arena.nodes[level_idx as usize].prev_level = NULL_IDX;
                self.level_arena.nodes[old_head as usize].prev_level = level_idx;
                self.best_bid = level_idx;
            } else {
                let mut curr = self.best_bid;
                while self.level_arena.nodes[curr as usize].next_level != NULL_IDX
                    && self.level_arena.nodes[self.level_arena.nodes[curr as usize].next_level as usize].price > price
                {
                    curr = self.level_arena.nodes[curr as usize].next_level;
                }
                let next = self.level_arena.nodes[curr as usize].next_level;
                self.level_arena.nodes[curr as usize].next_level = level_idx;
                self.level_arena.nodes[level_idx as usize].prev_level = curr;
                self.level_arena.nodes[level_idx as usize].next_level = next;
                if next != NULL_IDX {
                    self.level_arena.nodes[next as usize].prev_level = level_idx;
                }
            }
        }
    }

    #[inline(always)]
    fn unlink_price_level(&mut self, level_idx: u32, is_sell: bool) {
        let prev = self.level_arena.nodes[level_idx as usize].prev_level;
        let next = self.level_arena.nodes[level_idx as usize].next_level;

        if prev != NULL_IDX {
            self.level_arena.nodes[prev as usize].next_level = next;
        } else {
            if is_sell {
                self.best_ask = next;
            } else {
                self.best_bid = next;
            }
        }

        if next != NULL_IDX {
            self.level_arena.nodes[next as usize].prev_level = prev;
        }

        self.level_arena.nodes[level_idx as usize].prev_level = NULL_IDX;
        self.level_arena.nodes[level_idx as usize].next_level = NULL_IDX;
    }

    pub fn process(
        &mut self,
        mut order: Order,
        client_id: u64,
        trades: &mut Vec<Trade>,
    ) -> Result<ExecutionReport, RejectReason> {
        let order_type = order.get_order_type().unwrap_or(OrderType::Limit);

        if order_type == OrderType::Cancel {
            let target_id = if order.id != 0 { order.id } else { order.client_order_id };
            let cancelled = self.cancel_order(order.user_id, target_id)?;
            return Ok(ExecutionReport {
                executed_qty: 0,
                remaining_qty: cancelled.qty,
                stp_triggered: false,
            });
        }

        if order.id == 0 {
            order.id = self.order_id_counter;
            self.order_id_counter += 1;
        } else {
            self.order_id_counter = self.order_id_counter.max(order.id + 1);
        }

        let is_sell = order.side == Side::Sell as u8;
        let original_qty = order.qty;
        let mut stp_triggered = false;

        if is_sell {
            // Match against Bids (highest price first: self.best_bid)
            while order.qty > 0 && self.best_bid != NULL_IDX {
                let best_idx = self.best_bid;
                let best_price = self.level_arena.nodes[best_idx as usize].price;

                // Limit order price boundary
                if order_type == OrderType::Limit && best_price < order.price {
                    break;
                }

                // Market order slippage protection boundary (order.price is min acceptable price)
                if order_type == OrderType::Market && order.price > 0 && best_price < order.price {
                    break;
                }

                let stp = Self::match_level(
                    &mut order,
                    client_id,
                    best_idx,
                    &mut self.level_arena,
                    &mut self.order_arena,
                    &mut self.order_index,
                    &mut self.client_order_index,
                    trades,
                    &mut self.trade_id_counter,
                );

                if self.level_arena.nodes[best_idx as usize].is_empty() {
                    self.unlink_price_level(best_idx, false);
                    self.price_index.remove(best_price);
                    self.level_arena.deallocate(best_idx);
                }

                if stp {
                    stp_triggered = true;
                    break;
                }
            }
        } else {
            // Match against Asks (lowest price first: self.best_ask)
            while order.qty > 0 && self.best_ask != NULL_IDX {
                let best_idx = self.best_ask;
                let best_price = self.level_arena.nodes[best_idx as usize].price;

                // Limit order price boundary
                if order_type == OrderType::Limit && best_price > order.price {
                    break;
                }

                // Market order slippage protection boundary (order.price is max acceptable price)
                if order_type == OrderType::Market && order.price > 0 && best_price > order.price {
                    break;
                }

                let stp = Self::match_level(
                    &mut order,
                    client_id,
                    best_idx,
                    &mut self.level_arena,
                    &mut self.order_arena,
                    &mut self.order_index,
                    &mut self.client_order_index,
                    trades,
                    &mut self.trade_id_counter,
                );

                if self.level_arena.nodes[best_idx as usize].is_empty() {
                    self.unlink_price_level(best_idx, true);
                    self.price_index.remove(best_price);
                    self.level_arena.deallocate(best_idx);
                }

                if stp {
                    stp_triggered = true;
                    break;
                }
            }
        }

        let executed_qty = original_qty.saturating_sub(order.qty);

        // If STP triggered, the aggressive order's remaining quantity is cancelled (not rested)
        if stp_triggered {
            return Ok(ExecutionReport {
                executed_qty,
                remaining_qty: order.qty,
                stp_triggered: true,
            });
        }

        // Unfilled market orders terminate (IOC behavior)
        if order_type == OrderType::Market {
            return Ok(ExecutionReport {
                executed_qty,
                remaining_qty: order.qty,
                stp_triggered: false,
            });
        }

        // Resting Limit Order insertion
        if order.qty > 0 && order_type == OrderType::Limit {
            let node_idx = self.order_arena.allocate(order, client_id)?;
            let price = order.price;
            let side = order.side;
            let user_id = order.user_id;
            let client_order_id = order.client_order_id;

            let level_idx = if let Some(existing_idx) = self.price_index.lookup(price) {
                existing_idx
            } else {
                let new_idx = self.level_arena.allocate(price)?;
                self.price_index.insert(price, new_idx)?;
                self.link_price_level(new_idx, is_sell);
                new_idx
            };

            self.level_arena.nodes[level_idx as usize].push_order(node_idx, &mut self.order_arena);
            self.order_index.insert(order.id, client_order_id, user_id, price, node_idx, side)?;
            let _ = self.client_order_index.insert(user_id, client_order_id, order.id);
        }

        Ok(ExecutionReport {
            executed_qty,
            remaining_qty: order.qty,
            stp_triggered: false,
        })
    }

    #[inline(always)]
    fn match_level(
        incoming: &mut Order,
        taker_client_id: u64,
        level_idx: u32,
        level_arena: &mut PriceLevelArena,
        order_arena: &mut OrderArena,
        order_index: &mut FastOrderIndex,
        client_order_index: &mut FastClientIndex,
        trades: &mut Vec<Trade>,
        trade_id_counter: &mut u64,
    ) -> bool {
        let level = &mut level_arena.nodes[level_idx as usize];

        while incoming.qty > 0 && !level.is_empty() {
            let resting_idx = level.head_order;
            let resting_node = &mut order_arena.nodes[resting_idx as usize];

            // Self-Trade Prevention: If resting order belongs to same user, halt matching this level
            if resting_node.order.user_id == incoming.user_id {
                return true;
            }

            let trade_qty = std::cmp::min(incoming.qty, resting_node.order.qty);
            let trade_price = resting_node.order.price;

            let (buyer_id, seller_id) = if incoming.side == Side::Buy as u8 {
                (incoming.user_id, resting_node.order.user_id)
            } else {
                (resting_node.order.user_id, incoming.user_id)
            };

            let match_id = *trade_id_counter;
            *trade_id_counter += 1;

            trades.push(Trade {
                match_id,
                maker_order_id: resting_node.order.id,
                taker_order_id: incoming.id,
                maker_client_order_id: resting_node.order.client_order_id,
                taker_client_order_id: incoming.client_order_id,
                maker_user_id: resting_node.order.user_id,
                taker_user_id: incoming.user_id,
                maker_client_id: resting_node.client_id,
                taker_client_id,
                buyer_id,
                seller_id,
                maker_side: resting_node.order.side,
                symbol_id: incoming.symbol_id,
                price: trade_price,
                qty: trade_qty,
            });

            incoming.qty -= trade_qty;
            resting_node.order.qty -= trade_qty;
            level.total_qty -= trade_qty;

            if resting_node.order.qty == 0 {
                let filled_id = resting_node.order.id;
                let filled_user_id = resting_node.order.user_id;
                let filled_client_id = resting_node.order.client_order_id;
                level.pop_order(order_arena);
                order_arena.deallocate(resting_idx);
                order_index.remove(filled_id);
                client_order_index.remove(filled_user_id, filled_client_id);
            }
        }
        false
    }

    pub fn cancel_order(
        &mut self,
        requesting_user_id: u64,
        order_id: u64,
    ) -> Result<Order, RejectReason> {
        let entry = if let Some(e) = self.order_index.lookup(order_id) {
            e
        } else if let Some(exchange_id) = self.client_order_index.lookup(requesting_user_id, order_id) {
            self.order_index.lookup(exchange_id).ok_or(RejectReason::OrderNotFound)?
        } else {
            return Err(RejectReason::OrderNotFound);
        };

        if entry.user_id != requesting_user_id {
            return Err(RejectReason::Unauthorized);
        }

        let node = &self.order_arena.nodes[entry.node_idx as usize];
        if !node.active || node.order.id != entry.order_id {
            self.order_index.remove(entry.order_id);
            self.client_order_index.remove(entry.user_id, entry.client_order_id);
            return Err(RejectReason::OrderNotFound);
        }

        let cancelled_order = node.order;
        let is_sell = entry.side == Side::Sell as u8;

        if let Some(level_idx) = self.price_index.lookup(entry.price) {
            self.level_arena.nodes[level_idx as usize].unlink_order(entry.node_idx, &mut self.order_arena);
            if self.level_arena.nodes[level_idx as usize].is_empty() {
                self.unlink_price_level(level_idx, is_sell);
                self.price_index.remove(entry.price);
                self.level_arena.deallocate(level_idx);
            }
        }

        self.order_arena.deallocate(entry.node_idx);
        self.order_index.remove(entry.order_id);
        self.client_order_index.remove(entry.user_id, entry.client_order_id);

        Ok(cancelled_order)
    }

    #[inline(always)]
    pub fn get_bbo(&self) -> (Option<(u64, u64)>, Option<(u64, u64)>) {
        let best_bid = if self.best_bid != NULL_IDX {
            let node = &self.level_arena.nodes[self.best_bid as usize];
            Some((node.price, node.total_qty))
        } else {
            None
        };
        let best_ask = if self.best_ask != NULL_IDX {
            let node = &self.level_arena.nodes[self.best_ask as usize];
            Some((node.price, node.total_qty))
        } else {
            None
        };
        (best_bid, best_ask)
    }

    pub fn get_depth(&self, max_levels: usize) -> (Vec<MarketDepthLevel>, Vec<MarketDepthLevel>) {
        let mut bids = Vec::with_capacity(max_levels);
        let mut curr = self.best_bid;
        while curr != NULL_IDX && bids.len() < max_levels {
            let node = &self.level_arena.nodes[curr as usize];
            bids.push(MarketDepthLevel {
                price: node.price,
                total_qty: node.total_qty,
                order_count: node.order_count,
            });
            curr = node.next_level;
        }

        let mut asks = Vec::with_capacity(max_levels);
        let mut curr = self.best_ask;
        while curr != NULL_IDX && asks.len() < max_levels {
            let node = &self.level_arena.nodes[curr as usize];
            asks.push(MarketDepthLevel {
                price: node.price,
                total_qty: node.total_qty,
                order_count: node.order_count,
            });
            curr = node.next_level;
        }

        (bids, asks)
    }
}

/// Multi-asset matching engine.
/// Flat array of 3 books: BTC/USD (0), ETH/USD (1), SOL/USD (2).
pub struct MultiAssetEngine {
    books: [OrderBook; NUM_SYMBOLS],
    global_order_id_counter: u64,
}

impl Default for MultiAssetEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiAssetEngine {
    pub fn new() -> Self {
        Self {
            books: [
                OrderBook::new(Symbol::Btc),
                OrderBook::new(Symbol::Eth),
                OrderBook::new(Symbol::Sol),
            ],
            global_order_id_counter: 1,
        }
    }

    pub fn next_order_id(&mut self) -> u64 {
        let id = self.global_order_id_counter;
        self.global_order_id_counter += 1;
        id
    }

    #[inline(always)]
    pub fn process(
        &mut self,
        mut order: Order,
        client_id: u64,
        trades: &mut Vec<Trade>,
    ) -> Result<ExecutionReport, RejectReason> {
        let sym_idx = order.symbol_id as usize;
        if sym_idx >= NUM_SYMBOLS {
            return Err(RejectReason::InvalidOrder);
        }
        if order.id == 0 {
            order.id = self.global_order_id_counter;
            self.global_order_id_counter += 1;
        } else {
            self.global_order_id_counter = self.global_order_id_counter.max(order.id + 1);
        }
        self.books[sym_idx].process(order, client_id, trades)
    }

    #[inline(always)]
    pub fn replay_order(
        &mut self,
        order: Order,
        trades: &mut Vec<Trade>,
        risk: &mut crate::risk::RiskEngine,
    ) -> Result<(), RejectReason> {
        let order_type = order.get_order_type().unwrap_or(OrderType::Limit);
        if order_type == OrderType::Deposit {
            risk.deposit(order.user_id, order.price, order.symbol_id, order.qty);
            return Ok(());
        }

        let sym_idx = order.symbol_id as usize;
        if sym_idx >= NUM_SYMBOLS {
            return Err(RejectReason::InvalidOrder);
        }
        if order_type == OrderType::Cancel {
            if let Ok(cancelled) = self.cancel_order(order.symbol_id, order.user_id, order.id) {
                risk.unlock_cancelled_order_margin(&cancelled, cancelled.qty);
            }
        } else {
            if risk.lock_order_funds(&order).is_ok() {
                trades.clear();
                if let Ok(report) = self.process(order, 0, trades) {
                    let mut total_executed_qty = 0u64;
                    let mut total_executed_cost = 0u64;
                    for trade in trades.iter() {
                        risk.settle_trade(trade, order.price);
                        total_executed_qty += trade.qty;
                        total_executed_cost += trade.price.saturating_mul(trade.qty);
                    }
                    if report.stp_triggered && report.remaining_qty > 0 {
                        risk.unlock_cancelled_order_margin(&order, report.remaining_qty);
                    }
                    if order_type == OrderType::Market && !report.stp_triggered {
                        let side = order.get_side().unwrap_or(Side::Buy);
                        risk.refund_market_order_remainder(
                            order.user_id,
                            order.symbol_id,
                            side,
                            order.price,
                            order.qty,
                            total_executed_qty,
                            total_executed_cost,
                        );
                    }
                }
            }
        }
        Ok(())
    }

    #[inline(always)]
    pub fn cancel_order(
        &mut self,
        symbol_id: u16,
        user_id: u64,
        order_id: u64,
    ) -> Result<Order, RejectReason> {
        let sym_idx = symbol_id as usize;
        if sym_idx >= NUM_SYMBOLS {
            return Err(RejectReason::InvalidOrder);
        }
        self.books[sym_idx].cancel_order(user_id, order_id)
    }

    #[inline(always)]
    pub fn get_book(&self, symbol_id: u16) -> Option<&OrderBook> {
        let sym_idx = symbol_id as usize;
        if sym_idx < NUM_SYMBOLS {
            Some(&self.books[sym_idx])
        } else {
            None
        }
    }

    #[inline(always)]
    pub fn get_book_mut(&mut self, symbol_id: u16) -> Option<&mut OrderBook> {
        let sym_idx = symbol_id as usize;
        if sym_idx < NUM_SYMBOLS {
            Some(&mut self.books[sym_idx])
        } else {
            None
        }
    }
}
