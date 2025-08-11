use clap::Parser;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::sync::Arc;
use tokio::sync::Mutex;
use ultra_low_latency_order_book::protocol::*;
use ultra_low_latency_order_book::book::OrderBook;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value_t = 4000)]
    port: u16,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let listener = TcpListener::bind(("0.0.0.0", args.port)).await?;
    println!("Listening on 0.0.0.0:{}", args.port);

    // For the skeleton we keep a single shared OrderBook per instrument map protected by a Mutex
    // (easy to reason about). Later you'll split/shard to single-writer-per-book for performance.
    let books = Arc::new(Mutex::new(std::collections::HashMap::<u32, OrderBook>::new()));

    loop {
        let (mut socket, addr) = listener.accept().await?;
        let books = books.clone();
        tokio::spawn(async move {
            println!("Connection from {}", addr);
            if let Err(e) = handle_client(&mut socket, books).await {
                eprintln!("client error: {}", e);
            }
        });
    }
}

async fn handle_client(stream: &mut TcpStream, books: Arc<Mutex<std::collections::HashMap<u32, OrderBook>>>) -> anyhow::Result<()> {
    let mut buf = vec![0u8; 65536];
    loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 { break; }
        // We accept newline-delimited JSON messages for the skeleton.
        let slice = &buf[..n];
        if let Ok(s) = std::str::from_utf8(slice) {
            for line in s.lines() {
                if line.trim().is_empty() { continue; }
                match serde_json::from_str::<Message>(line) {
                    Ok(msg) => {
                        match msg {
                            Message::NewOrder(no) => {
                                let mut map = books.lock().await;
                                let book = map.entry(no.instrument).or_insert_with(OrderBook::new);
                                let trades = book.on_new_order(no);
                                if !trades.is_empty() {
                                    // In a real engine, you'd send trade callbacks back; here we just log
                                    for t in trades {
                                        println!("TRADE price={} qty={} resting={:?} incoming={}", t.0, t.1, t.2, t.3);
                                    }
                                }
                            }
                            Message::Cancel(c) => {
                                let mut map = books.lock().await;
                                if let Some(book) = map.get_mut(&c.instrument) {
                                    let removed = book.on_cancel(c);
                                    if removed { println!("Cancelled"); }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("parse error: {}", e);
                    }
                }
            }
        }
    }
    Ok(())
}