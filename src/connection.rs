use tokio::net::TcpStream;


pub struct Order {
   pub  order_type: String,
   pub  price: u64,
   pub  quantity: u32,
}


fn parse_order(bytes:&[u8])->Option<order>{
       if bytes.len() < 13 { return None; }

       let order_type=match bytes[0]{
           0 => "ASK",
           1 =>  "BID",
          _  =>  "UNKNOWN"
       }.to_string();

        let price = u64::from_le_bytes(bytes[1..9].try_into().unwrap());
        let quantity = u32::from_le_bytes(bytes[9..13].try_into().unwrap());

       Some(Order { order_type, price, quantity })
}


pub async fn connection()->anyhow::Result<String,String>{
         let stream=TcpStream::bind("127.0.0.1:4000").await?;
         println!("Server listening on 127.0.0.1:4000");


         loop{
            let (mut socket,addr)=stream.accept().await?;
             println!("New client: {:?}", addr);

             tokio::spawn(async move{
                
                let mut buf=vec![0u8; 13];
                 loop{
                    match socket.read_exact(&buf).await{
                        ok(_)=>{
                        let order= parse_order(&buf);
                        }
                        Err(e)=>{
                      eprintln!("Connection closed or error: {:?}", e);
                        break;
                        }
                    }
                 }

             })
         }

        }