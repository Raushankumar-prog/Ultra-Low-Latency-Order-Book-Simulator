use clap::Parser;
use tokio::net::TcpStream;
use tokio::io::AsyncWriteExt;
use ultra_low_latency_order_book::protocol::*;
use serde_json;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "127.0.0.1:4000")]
    addr: String,
    #[arg(long, default_value_t = 10000)]
    n: usize,
    #[arg(long, default_value_t = 1)]
    instrument: u32,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let mut stream = TcpStream::connect(&args.addr).await?;
    println!("Connected to {}", &args.addr);

    for i in 0..args.n {
        let msg = Message::NewOrder(NewOrder {
            order_id: i as u64 + 1,
            instrument: args.instrument,
            side: if i % 2 == 0 { Side::Buy } else { Side::Sell },
            price: 100 + (i as u64 % 10),
            qty: 1,
            ts_ns: 0,
        });
        let j = serde_json::to_string(&msg)? + "\n";
        stream.write_all(j.as_bytes()).await?;
    }

    println!("Sent {} messages", args.n);
    Ok(())
}
