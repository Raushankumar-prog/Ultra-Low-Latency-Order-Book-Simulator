use tokio::net::TcpStream;
use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() {
    let mut stream = TcpStream::connect("127.0.0.1:4000").await.unwrap();

    
    let mut msg = vec![0u8; 13];
    msg[0] = 1; // BID
    msg[1..9].copy_from_slice(&500u64.to_le_bytes());
    msg[9..13].copy_from_slice(&100u32.to_le_bytes());

    stream.write_all(&msg).await.unwrap();
    println!("Sent order");
}
