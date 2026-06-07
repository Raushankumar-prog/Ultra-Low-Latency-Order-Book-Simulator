use crate::account_db::AccountDb;
use crate::order::Order;

const MAX_QTY: u64 = 1_000_000;

pub fn verify_transaction(
    order: &Order,
    accounts: &AccountDb,
) -> bool {
    accounts.contains(order.user_id)
        && order.price > 0
        && order.qty > 0
        && order.qty <= MAX_QTY
}


