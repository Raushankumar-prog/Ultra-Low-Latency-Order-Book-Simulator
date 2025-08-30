use connection::connection;
use crate::connection::Order;


pub async fn engine() -> anyhow::Result<()> {
    let order = connection().await?;
    println!("Got order: type={}, price={}, quantity={}", order.order_type, order.price, order.quantity);

     match order.order_type.as_str() {
            "BID" => book.bid(order.price, order.quantity),
            "ASK" => book.ask(order.price, order.quantity),
            _ => println!("Unknown order type"),
        }

    
        book.print();
    Ok(())
}
