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

   
}
