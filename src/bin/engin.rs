use clap::Parser;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use ultra_low_latency_order_book::book::OrderBook;
use ultra_low_latency_order_book::protocol::*;

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

    // Sharding setup
    const NUM_SHARDS: usize = 4; // Set to number of CPU cores or desired shards
    use std::collections::HashMap;
    use tokio::sync::mpsc;
    let mut shard_senders = Vec::new();
    for shard_id in 0..NUM_SHARDS {
        let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
        shard_senders.push(tx);
        tokio::spawn(async move {
            let mut books: HashMap<u32, OrderBook> = HashMap::new();
            while let Some(msg) = rx.recv().await {
                match msg {
                    Message::NewOrder(no) => {
                        let book = books.entry(no.instrument).or_insert_with(OrderBook::new);
                        let trades = book.on_new_order(no);
                        if !trades.is_empty() {
                            for t in trades {
                                println!(
                                    "[shard {}] TRADE price={} qty={} resting={:?} incoming={}",
                                    shard_id, t.0, t.1, t.2, t.3
                                );
                            }
                        }
                    }
                    Message::Cancel(c) => {
                        if let Some(book) = books.get_mut(&c.instrument) {
                            let removed = book.on_cancel(c);
                            if removed {
                                println!("[shard {}] Cancelled", shard_id);
                            }
                        }
                    }
                }
            }
        });
    }

    loop {
        let (mut socket, addr) = listener.accept().await?;
        let shard_senders = shard_senders.clone();
        tokio::spawn(async move {
            println!("Connection from {}", addr);
            let mut buf = vec![0u8; 1024];
            let msg_size = 256; // Adjust as needed
            loop {
                let n = socket.read_exact(&mut buf[..msg_size]).await;
                if let Err(e) = n {
                    if e.kind() == std::io::ErrorKind::UnexpectedEof {
                        break;
                    }
                    eprintln!("read error: {}", e);
                    break;
                }
                let msg = Message::from_binary(&buf[..msg_size]);
                // Instrument-to-shard mapping
                let instrument = match &msg {
                    Message::NewOrder(no) => no.instrument,
                    Message::Cancel(c) => c.instrument,
                };
                let shard = (instrument as usize) % NUM_SHARDS;
                if let Err(e) = shard_senders[shard].send(msg) {
                    eprintln!("Failed to send to shard {}: {}", shard, e);
                }
            }
        });
    }
}

async fn handle_client(
    stream: &mut TcpStream,
    books: Arc<Mutex<std::collections::HashMap<u32, OrderBook>>>,
) -> anyhow::Result<()> {
    use std::mem::size_of;
    use ultra_low_latency_order_book::protocol::Message;
    let mut buf = vec![0u8; 1024];
    let msg_size = 256; // Adjust as needed for your max message size
    loop {
        let n = stream.read_exact(&mut buf[..msg_size]).await;
        if let Err(e) = n {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                break;
            }
            eprintln!("read error: {}", e);
            break;
        }
        let msg = Message::from_binary(&buf[..msg_size]);
        match msg {
            Message::NewOrder(no) => {
                let mut map = books.lock().await;
                let book = map.entry(no.instrument).or_insert_with(OrderBook::new);
                let trades = book.on_new_order(no);
                if !trades.is_empty() {
                    for t in trades {
                        println!(
                            "TRADE price={} qty={} resting={:?} incoming={}",
                            t.0, t.1, t.2, t.3
                        );
                    }
                }
            }
            Message::Cancel(c) => {
                let mut map = books.lock().await;
                if let Some(book) = map.get_mut(&c.instrument) {
                    let removed = book.on_cancel(c);
                    if removed {
                        println!("Cancelled");
                    }
                }
            }
        }
    }
    Ok(())
}
