use crate::types::{Order, OrderType, RejectReason, Side, Trade, NUM_SYMBOLS};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Account {
    pub user_id: u64,
    pub usd_available: u64,
    pub usd_locked: u64,
    pub asset_available: [u64; NUM_SYMBOLS],
    pub asset_locked: [u64; NUM_SYMBOLS],
}

impl Account {
    pub fn new(user_id: u64, initial_usd: u64) -> Self {
        Self {
            user_id,
            usd_available: initial_usd,
            usd_locked: 0,
            asset_available: [0; NUM_SYMBOLS],
            asset_locked: [0; NUM_SYMBOLS],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RiskEngine {
    pub accounts: HashMap<u64, Account>,
}

impl RiskEngine {
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
        }
    }

    pub fn deposit(&mut self, user_id: u64, usd: u64, symbol_id: u16, asset_qty: u64) {
        let sym_idx = symbol_id as usize;
        if sym_idx >= NUM_SYMBOLS {
            return;
        }

        let account = self
            .accounts
            .entry(user_id)
            .or_insert_with(|| Account::new(user_id, 0));
        account.usd_available += usd;
        account.asset_available[sym_idx] += asset_qty;
    }

    pub fn get_account(&self, user_id: u64) -> Option<&Account> {
        self.accounts.get(&user_id)
    }

    pub fn get_account_mut(&mut self, user_id: u64) -> Option<&mut Account> {
        self.accounts.get_mut(&user_id)
    }

    /// Pre-trade margin reservation.
    /// Locks USD for buys and assets for sells. Rejects immediately if funds are insufficient.
    pub fn lock_order_funds(&mut self, order: &Order) -> Result<(), RejectReason> {
        let order_type = order.get_order_type().unwrap_or(OrderType::Limit);
        if order_type == OrderType::Cancel {
            return Ok(());
        }

        if order.qty == 0 {
            return Err(RejectReason::InvalidOrder);
        }

        let sym_idx = order.symbol_id as usize;
        if sym_idx >= NUM_SYMBOLS {
            return Err(RejectReason::InvalidOrder);
        }

        let account = self
            .accounts
            .get_mut(&order.user_id)
            .ok_or(RejectReason::Unauthorized)?;

        let is_buy = order.side == Side::Buy as u8;

        if is_buy {
            // Market buys without an explicit budget/max price are forbidden
            if order.price == 0 {
                return Err(RejectReason::InvalidOrder);
            }

            let required_usd = order
                .price
                .checked_mul(order.qty)
                .ok_or(RejectReason::InvalidOrder)?;

            if account.usd_available < required_usd {
                return Err(RejectReason::InsufficientFunds);
            }

            account.usd_available -= required_usd;
            account.usd_locked += required_usd;
        } else {
            if account.asset_available[sym_idx] < order.qty {
                return Err(RejectReason::InsufficientFunds);
            }

            account.asset_available[sym_idx] -= order.qty;
            account.asset_locked[sym_idx] += order.qty;
        }

        Ok(())
    }

    /// Unlocks locked margin when an order is cancelled or expires unfilled.
    pub fn unlock_order_funds(
        &mut self,
        user_id: u64,
        symbol_id: u16,
        side: Side,
        price: u64,
        unexecuted_qty: u64,
    ) {
        if unexecuted_qty == 0 {
            return;
        }

        let sym_idx = symbol_id as usize;
        if sym_idx >= NUM_SYMBOLS {
            return;
        }

        if let Some(account) = self.accounts.get_mut(&user_id) {
            if side == Side::Buy {
                let locked_usd = price.saturating_mul(unexecuted_qty);
                account.usd_locked = account.usd_locked.saturating_sub(locked_usd);
                account.usd_available += locked_usd;
            } else {
                account.asset_locked[sym_idx] = account.asset_locked[sym_idx].saturating_sub(unexecuted_qty);
                account.asset_available[sym_idx] += unexecuted_qty;
            }
        }
    }

    /// Convenience wrapper to unlock margin for an order.
    pub fn unlock_cancelled_order_margin(&mut self, order: &Order, unexecuted_qty: u64) {
        let side = order.get_side().unwrap_or(Side::Buy);
        self.unlock_order_funds(order.user_id, order.symbol_id, side, order.price, unexecuted_qty);
    }

    /// Unlocks unexecuted budget remainder from a market order.
    /// Completely prevents funds from remaining trapped in usd_locked or asset_locked.
    pub fn refund_market_order_remainder(
        &mut self,
        user_id: u64,
        symbol_id: u16,
        side: Side,
        budget_price: u64,
        total_requested_qty: u64,
        total_executed_qty: u64,
        total_executed_cost: u64,
    ) {
        let sym_idx = symbol_id as usize;
        if sym_idx >= NUM_SYMBOLS {
            return;
        }

        if let Some(account) = self.accounts.get_mut(&user_id) {
            if side == Side::Buy {
                let locked_budget = budget_price.saturating_mul(total_requested_qty);
                if locked_budget > total_executed_cost {
                    let surplus = locked_budget - total_executed_cost;
                    account.usd_locked = account.usd_locked.saturating_sub(surplus);
                    account.usd_available += surplus;
                }
            } else {
                let unexecuted_qty = total_requested_qty.saturating_sub(total_executed_qty);
                if unexecuted_qty > 0 {
                    account.asset_locked[sym_idx] = account.asset_locked[sym_idx].saturating_sub(unexecuted_qty);
                    account.asset_available[sym_idx] += unexecuted_qty;
                }
            }
        }
    }

    /// Settles a matched trade between Maker and Taker accounts.
    /// Handles price improvements: refunds the difference if a buyer locked at higher price.
    pub fn settle_trade(
        &mut self,
        trade: &Trade,
        taker_limit_price: u64,
    ) {
        let total_cost = trade.price.saturating_mul(trade.qty);
        let sym_idx = trade.symbol_id as usize;
        if sym_idx >= NUM_SYMBOLS {
            return;
        }

        // 1. Settle Buyer
        if let Some(buyer) = self.accounts.get_mut(&trade.buyer_id) {
            let locked_price = if trade.buyer_id == trade.maker_user_id {
                trade.price
            } else {
                taker_limit_price
            };

            let locked_usd = locked_price.saturating_mul(trade.qty);
            buyer.usd_locked = buyer.usd_locked.saturating_sub(locked_usd);

            // Price improvement refund
            if locked_usd > total_cost {
                buyer.usd_available += locked_usd - total_cost;
            }

            buyer.asset_available[sym_idx] += trade.qty;
        }

        // 2. Settle Seller
        if let Some(seller) = self.accounts.get_mut(&trade.seller_id) {
            seller.asset_locked[sym_idx] = seller.asset_locked[sym_idx].saturating_sub(trade.qty);
            seller.usd_available += total_cost;
        }
    }
}
