use crate::types::{Order, ServerMessage, ORDER_MAGIC};
use std::mem::size_of;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy)]
pub struct InboundMessage {
    pub order: Order,
    pub ingress_instant: Instant,
    pub client_id: u64,
}

#[derive(Debug, Clone)]
pub enum OutboundEvent {
    ClientAck {
        client_id: u64,
        msg: ServerMessage,
    },
    ClientReject {
        client_id: u64,
        msg: ServerMessage,
    },
    ClientCancelOk {
        client_id: u64,
        msg: ServerMessage,
    },
    TradeFill {
        taker_client_id: u64,
        maker_client_id: u64,
        taker_msg: ServerMessage,
        maker_msg: ServerMessage,
    },
    BboBroadcast {
        msg: ServerMessage,
    },
}

#[derive(Debug)]
pub enum ConnectionEvent {
    Connected {
        client_id: u64,
        sender: mpsc::Sender<ServerMessage>,
    },
    Disconnected {
        client_id: u64,
    },
}

static NEXT_CLIENT_ID: AtomicU64 = AtomicU64::new(1);

/// Ingress Gateway: Accepts TCP connections, frames binary orders, and dispatches to the matching pipeline.
pub async fn run_gateway(
    addr: &str,
    order_tx: mpsc::Sender<InboundMessage>,
    conn_tx: mpsc::Sender<ConnectionEvent>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind(addr).await?;
    println!("Bidirectional Gateway listening on {}", addr);

    loop {
        match listener.accept().await {
            Ok((socket, _peer_addr)) => {
                let _ = socket.set_nodelay(true);
                let client_id = NEXT_CLIENT_ID.fetch_add(1, Ordering::Relaxed);
                let tx = order_tx.clone();
                let c_tx = conn_tx.clone();

                tokio::spawn(async move {
                    let _ = handle_client(socket, client_id, tx, c_tx).await;
                });
            }
            Err(e) => {
                eprintln!("Failed to accept incoming connection: {:?}", e);
            }
        }
    }
}

async fn handle_client(
    stream: TcpStream,
    client_id: u64,
    order_tx: mpsc::Sender<InboundMessage>,
    conn_tx: mpsc::Sender<ConnectionEvent>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (mut reader, mut writer) = stream.into_split();

    // Bounded channel to enforce backpressure and prevent OOM on slow consumers
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<ServerMessage>(8192);

    // Notify egress dispatcher of new connection
    let _ = conn_tx.send(ConnectionEvent::Connected {
        client_id,
        sender: outbound_tx,
    }).await;

    // Outbound writer task
    let writer_task = tokio::spawn(async move {
        while let Some(msg) = outbound_rx.recv().await {
            let le_msg = msg.to_le();
            let bytes = bytemuck::bytes_of(&le_msg);
            if writer.write_all(bytes).await.is_err() {
                break;
            }
        }
    });

    // Inbound reader loop with monotonic high-precision timestamping
    let mut buffer = [0u8; size_of::<Order>()];
    loop {
        match reader.read_exact(&mut buffer).await {
            Ok(_) => {
                let raw_order: Order = *bytemuck::from_bytes(&buffer);
                let mut order = raw_order.from_le(); // Endianness safety

                if order.magic != ORDER_MAGIC {
                    eprintln!(
                        "[GATEWAY] Invalid protocol magic 0x{:04X} from client #{}. Terminating connection.",
                        order.magic, client_id
                    );
                    break;
                }

                // Strict ingress quarantine: client IDs cannot dictate internal exchange order IDs
                use crate::types::OrderType;
                let order_type = order.get_order_type().unwrap_or(OrderType::Limit);
                if order_type != OrderType::Cancel {
                    if order.client_order_id == 0 {
                        order.client_order_id = order.id;
                    }
                    order.id = 0; // Forced to 0 so matching engine stamps official exchange order ID
                }

                let inbound = InboundMessage {
                    order,
                    ingress_instant: Instant::now(),
                    client_id,
                };

                if order_tx.send(inbound).await.is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    writer_task.abort();

    // Notify egress dispatcher of disconnect
    let _ = conn_tx.send(ConnectionEvent::Disconnected { client_id }).await;

    Ok(())
}
