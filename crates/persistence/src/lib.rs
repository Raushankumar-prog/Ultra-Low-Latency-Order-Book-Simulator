use api::Order;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

pub struct Wal {
    file: File,
}

impl Wal {
    pub fn new(path: impl AsRef<Path>) -> Self {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .read(true)
            .open(path)
            .expect("Failed to open WAL");

        Self { file }
    }

    pub fn write(&mut self, order: &Order) {
        let bytes = bytemuck::bytes_of(order);
        self.file.write_all(bytes).expect("Failed to write to WAL");
    }

    pub fn replay(&mut self) -> Vec<Order> {
        let mut orders = Vec::new();
        let mut buf = [0u8; 48];

        let _ = std::io::Seek::seek(&mut self.file, std::io::SeekFrom::Start(0));

        loop {
            match self.file.read_exact(&mut buf) {
                Ok(_) => {
                    if let Ok(order) = bytemuck::try_from_bytes::<Order>(&buf) {
                        orders.push(*order);
                    }
                }
                Err(_) => break, 
            }
        }
        
        let _ = std::io::Seek::seek(&mut self.file, std::io::SeekFrom::End(0));

        orders
    }
}
