use anyhow::Result;
use tokio::net::TcpListener;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;

pub struct Order {
    pub order_type: String,
    pub price: u64,
    pub quantity: u32,
}

fn parse_order(bytes: &[u8]) -> Option<Order> {
    if bytes.len() < 13 {
        return None;
    }

    let order_type = match bytes[0] {
        0 => "ASK",
        1 => "BID",
        _ => "UNKNOWN",
    }
    .to_string();

    let price = u64::from_le_bytes(bytes[1..9].try_into().unwrap());
    let quantity = u32::from_le_bytes(bytes[9..13].try_into().unwrap());

    Some(Order {
        order_type,
        price,
        quantity,
    })
}

/// Spawn server that pushes orders into a channel
pub async fn connection(tx: mpsc::Sender<Order>) -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:4000").await?;
    println!("Server listening on 127.0.0.1:4000");

    loop {
        let (mut socket, addr) = listener.accept().await?;
        println!("New client: {:?}", addr);

        let tx = tx.clone();

        tokio::spawn(async move {
            let mut buf = vec![0u8; 13];

            loop {
                match socket.read_exact(&mut buf).await {
                    Ok(_) => {
                        if let Some(order) = parse_order(&buf) {
                            println!(
                                "Got order from {:?}: type={}, price={}, qty={}",
                                addr, order.order_type, order.price, order.quantity
                            );

                            // send to engine
                            if tx.send(order).await.is_err() {
                                eprintln!("Engine dropped receiver, stopping client task.");
                                break;
                            }
                        } else {
                            eprintln!("Failed to parse order from {:?}", addr);
                        }
                    }
                    Err(e) => {
                        eprintln!("Client {:?} disconnected: {:?}", addr, e);
                        break;
                    }
                }
            }
        });
    }
}
