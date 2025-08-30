use crate::connection::{connection, Order};
use crate::book::OrderBook;
use anyhow::Result;
use tokio::sync::mpsc;

pub async fn engine() -> Result<()> {
    let mut book = OrderBook::new();

    // channel between connection and engine
    let (tx, mut rx) = mpsc::channel::<Order>(100);

    // spawn server in background
    tokio::spawn(async move {
        if let Err(e) = connection(tx).await {
            eprintln!("Connection error: {:?}", e);
        }
    });

    // engine loop consumes orders
    while let Some(order) = rx.recv().await {
        println!(
            "Engine got order: type={}, price={}, quantity={}",
            order.order_type, order.price, order.quantity
        );

        match order.order_type.as_str() {
            "BID" => book.bid(order.price, order.quantity),
            "ASK" => book.ask(order.price, order.quantity),
            _ => println!("Unknown order type"),
        }

        book.print();
    }

    Ok(())
}
