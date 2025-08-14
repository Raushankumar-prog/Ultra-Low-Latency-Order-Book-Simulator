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

    let books = Arc::new(Mutex::new(
        std::collections::HashMap::<u32, OrderBook>::new(),
    ));

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
