use api::Order;
use tokio::net::TcpStream;
use tokio::io::AsyncWriteExt;
use std::time::Duration;

#[tokio::main]
async fn main() {
    println!("Connecting to exchange...");
    let mut stream = loop {
        match TcpStream::connect("127.0.0.1:4000").await {
            Ok(s) => break s,
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(500)).await;
                continue;
            }
        }
    };
    println!("Connected!");

    let mut id = 1;
    loop {
        let order = Order {
            id,
            price: 100,
            qty: 10,
            user_id: 1,
            company_id: 1,
            side: (id % 2) as u8,
            _padding: [0; 7],
        };

        let bytes = bytemuck::bytes_of(&order);
        stream.write_all(bytes).await.unwrap();
        println!("Sent Order #{}", id);
        id += 1;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
