use tokio::net::TcpStream;
use tokio::io::AsyncWriteExt;
use tokio::time::{sleep, Duration};

async fn send_order(stream: &mut TcpStream, order_type: u8, price: u64, qty: u32) {
    let mut msg = vec![0u8; 13];
    msg[0] = order_type; // 0 = ASK, 1 = BID
    msg[1..9].copy_from_slice(&price.to_le_bytes());
    msg[9..13].copy_from_slice(&qty.to_le_bytes());

    stream.write_all(&msg).await.unwrap();
}

#[tokio::main]
async fn main() {
    let mut stream = TcpStream::connect("127.0.0.1:4000").await.unwrap();

    loop {
        // send a BID
        send_order(&mut stream, 1, 500, 10).await;
        println!("Sent BID 10 @ 500");

        // send an ASK at same price to match
        send_order(&mut stream, 0, 500, 10).await;
        println!("Sent ASK 10 @ 500");

        
        sleep(Duration::from_millis(500)).await;
    }
}
