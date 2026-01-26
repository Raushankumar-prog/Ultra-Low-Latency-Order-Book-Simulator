use std::collections::HashMap;
use api::Order;

#[derive(Debug, Default, Clone)]
pub struct Account {
    pub usd: u64,
    pub btc: u64,
}

pub struct RiskEngine {
    accounts: HashMap<u64, Account>,
}

impl RiskEngine {
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
        }
    }

    pub fn deposit(&mut self, user_id: u64, usd: u64, btc: u64) {
        let account = self.accounts.entry(user_id).or_default();
        account.usd += usd;
        account.btc += btc;
    }

    pub fn check_risk(&mut self, order: &Order) -> bool {
        let account = self.accounts.entry(order.user_id).or_default();

        if order.side == 0 {
            let cost = order.price * order.qty;
            if account.usd >= cost {
                account.usd -= cost;
                return true;
            }
        } else {
            if account.btc >= order.qty {
                account.btc -= order.qty;
                return true;
            }
        }

        false
    }

    pub fn settle_trade(&mut self, buy_user_id: u64, sell_user_id: u64, price: u64, qty: u64) {
        if buy_user_id == sell_user_id { return; }

        {
             let buyer_acct = self.accounts.entry(buy_user_id).or_default();
             buyer_acct.btc += qty;
        } 

        {
             let seller_acct = self.accounts.entry(sell_user_id).or_default();
             seller_acct.usd += price * qty;
        } 
    }
}
