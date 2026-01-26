use matching_engine::MatchingEngine;
use risk::RiskEngine;
use crossbeam_channel::unbounded;
use std::thread;

#[tokio::main]
async fn main() {
    let (tx, rx) = unbounded();

    thread::spawn(move || {
        let mut engine = MatchingEngine::new();
        let mut risk = RiskEngine::new();

        risk.deposit(1, 10_000_000, 10_000); 
        println!("Core Engine (Matching + Risk) started.");

        loop {
            match rx.recv() {
                Ok(order) => {
                    if risk.check_risk(&order) {
                        let trades = engine.process(order);
                        
                        for trade in trades {
                            println!("Trade Executed: {:?}", trade);
                            risk.settle_trade(
                                trade.buyer_id, 
                                trade.seller_id, 
                                trade.price, 
                                trade.qty
                            );
                        }
                    } else {
                        eprintln!("RISK REJECT: Order {} (User {})", order.id, order.user_id);
                    }
                },
                Err(_) => break,
            }
        }
    });

    println!("Starting Gateway on port 4000...");
    gateway::run(tx).await;
}