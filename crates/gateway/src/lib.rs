use api::Order;
use crossbeam_channel::Sender;
use tokio::net::TcpListener;
use tokio::io::AsyncReadExt;

pub async fn run(tx: Sender<Order>) {
    let listener = TcpListener::bind("0.0.0.0:4000").await.unwrap();

    loop {
        let (mut socket, _) = listener.accept().await.unwrap();
        let tx_clone = tx.clone(); 

        tokio::spawn(async move {
            let mut buffer = [0u8; 48]; 
            loop {
                match socket.read_exact(&mut buffer).await {
                    Ok(0) => return,
                    Ok(_) => {
                        if let Ok(order) = bytemuck::try_from_bytes::<Order>(&buffer) {
                             let _ = tx_clone.send(*order);
                             println!("sent order: {:?}", order.id);
                        }
                    }
                    Err(_) => return,
                }
            }
        });
    }
}