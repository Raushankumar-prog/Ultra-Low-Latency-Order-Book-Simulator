use orderbook::types::{Order, ServerMessage, ServerMsgType, Side, Symbol};
use rand::Rng;
use std::mem::size_of;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Intelligent Market Maker (Bidirectional Client) ===");
    println!("Connecting to exchange gateway at 127.0.0.1:4000...");

    let stream = loop {
        match TcpStream::connect("127.0.0.1:4000").await {
            Ok(s) => {
                let _ = s.set_nodelay(true);
                break s;
            }
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(500)).await;
                continue;
            }
        }
    };
    println!("Connected! Initiating two-way trading loop.");

    let (mut reader, mut writer) = stream.into_split();

    let total_fills = Arc::new(AtomicU64::new(0));
    let total_acks = Arc::new(AtomicU64::new(0));
    let total_cancels = Arc::new(AtomicU64::new(0));
    let total_rejects = Arc::new(AtomicU64::new(0));

    let fills_clone = total_fills.clone();
    let acks_clone = total_acks.clone();
    let cancels_clone = total_cancels.clone();
    let rejects_clone = total_rejects.clone();

    // Outbound server message listener task
    tokio::spawn(async move {
        let mut buffer = [0u8; size_of::<ServerMessage>()];
        while let Ok(_) = reader.read_exact(&mut buffer).await {
            let raw_msg: ServerMessage = *bytemuck::from_bytes(&buffer);
            let msg = raw_msg.from_le(); // Endianness safety

            if msg.magic != orderbook::types::ORDER_MAGIC {
                eprintln!("[CLIENT ERROR] Invalid protocol magic 0x{:04X}", msg.magic);
                break;
            }

            if msg.msg_type == ServerMsgType::Accepted as u8 {
                acks_clone.fetch_add(1, Ordering::Relaxed);
            } else if msg.msg_type == ServerMsgType::Filled as u8 {
                fills_clone.fetch_add(1, Ordering::Relaxed);
                let role = if msg.is_maker == 1 { "MAKER" } else { "TAKER" };
                println!(
                    "[{role} FILL] User #{}, ExchOrder #{}, ClientOrder #{}: Matched {} lots @ ${:.2} (Match #{})",
                    msg.user_id,
                    msg.order_id,
                    msg.client_order_id(),
                    msg.qty,
                    msg.price as f64 / 100.0,
                    msg.match_id
                );
            } else if msg.msg_type == ServerMsgType::Cancelled as u8 {
                cancels_clone.fetch_add(1, Ordering::Relaxed);
            } else if msg.msg_type == ServerMsgType::Rejected as u8 {
                rejects_clone.fetch_add(1, Ordering::Relaxed);
                eprintln!("[ORDER REJECTED] Order #{}: Reason Code {}", msg.order_id, msg.reject_code);
            }
        }
    });

    let mut rng = rand::thread_rng();
    let mut order_id = 1u64;
    let mut active_quotes: Vec<(u64, Symbol, u64)> = Vec::new();

    let symbols = [
        (Symbol::Btc, 6_500_000u64), // BTC: $65,000.00
        (Symbol::Eth, 350_000u64),   // ETH: $3,500.00
        (Symbol::Sol, 15_000u64),    // SOL: $150.00
    ];

    loop {
        let (symbol, base_price) = symbols[rng.gen_range(0..symbols.len())];
        let action = rng.gen_range(0..10);

        let order = if action < 7 {
            // 70% chance: Submit Limit Quote (User 1 = Market Maker)
            let side = if rng.gen_bool(0.5) { Side::Buy } else { Side::Sell };
            let spread_offset = rng.gen_range(1..50);
            let price = if side == Side::Buy {
                base_price.saturating_sub(spread_offset)
            } else {
                base_price.saturating_add(spread_offset)
            };
            let qty = rng.gen_range(1..5);
            let user_id = 1; // User 1 provides liquidity

            let ord = Order::new_limit(order_id, user_id, symbol, side, price, qty);
            active_quotes.push((order_id, symbol, user_id));
            order_id += 1;
            ord
        } else if action < 9 && !active_quotes.is_empty() {
            // 20% chance: Authenticated Cancel of own quote
            let cancel_idx = rng.gen_range(0..active_quotes.len());
            let (target_id, target_symbol, user_id) = active_quotes.swap_remove(cancel_idx);
            Order::new_cancel(target_id, user_id, target_symbol)
        } else {
            // 10% chance: Taker Market Order from User 2 (Prevents Self-Trade!)
            let side = if rng.gen_bool(0.5) { Side::Buy } else { Side::Sell };
            let qty = 1;
            let budget_price = base_price + 100;
            let user_id = 2; // Different user -> no self-trade conflicts!

            let ord = Order::new_market(order_id, user_id, symbol, side, qty, budget_price);
            order_id += 1;
            ord
        };

        let le_order = order.to_le();
        let bytes = bytemuck::bytes_of(&le_order);
        if let Err(e) = writer.write_all(bytes).await {
            eprintln!("Socket write error: {:?}", e);
            break;
        }

        if order_id % 100 == 0 {
            println!(
                "Dispatched: {} | Acks: {} | Fills: {} | Cancels: {} | Rejects: {}",
                order_id,
                total_acks.load(Ordering::Relaxed),
                total_fills.load(Ordering::Relaxed),
                total_cancels.load(Ordering::Relaxed),
                total_rejects.load(Ordering::Relaxed),
            );
        }

        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    Ok(())
}
