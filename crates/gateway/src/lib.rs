use quinn::{ Endpoint, ServerConfig, Connection, };
use std::{  net::SocketAddr};
use anyhow::Result;
use crossbeam_queue::ArrayQueue;


const BUFFER_SIZE: usize = 4096;
const RING_SIZE: usize = 1_000_000;

pub type RingBuffer = Arc<ArrayQueue<Vec<u8>>>;

pub fn create_ring_buffer() -> RingBuffer {
    Arc::new(ArrayQueue::new(RING_SIZE))
}

pub fn receiver( 
    config: ServerConfig,
    ring:RingBuffer
)-> Result<()>{
    let addrs:SocketAddr= "127.0.0.1:5001".parse()?;
    let endpoint=Endpoint.server(config, addr)?;

    loop{
        let some(connecting)=endpoint.accept().await
                             else{
                                continue;
                             }
       tokio::spawn(async move {
        match connecting.await{
            ok(Connection)=>{if let Err(err) =
                        handle_connection(
                            connection,
                            ring,
                        )
                        .await
                      {
                        eprintln!(
                            "connection error: {}",
                            err
                        );
                    }
                 Err(err) => {
                    eprintln!(
                        "handshake error: {}",
                        err
                    );
                }
            }
        });
    }
}


pub handle_connection(
    conn:connection,
    ring: RingBuffer
)->Result<()>{
    loop{
        let stream = match  conn.accept_uni().await {
             Ok(stream) => stream,
                Err(_) => break,
    };
    let buffer= vec![0u8; BUFFER_SIZE];

    loop{
        match stream.read(&mut buffer).await{
                  Ok(Some(n)) => {

                    let packet =
                        buffer[..n].to_vec();

                    if ring.push(packet).is_err()
                    {
                        eprintln!(
                            "ring buffer full"
                        );
                    }
                }

                Ok(None) => {
                    break;
                }

                Err(_) => {
                    break;
                }
        }
    }

}
}


