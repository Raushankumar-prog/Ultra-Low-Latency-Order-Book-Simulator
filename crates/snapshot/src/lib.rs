use crate::matching-engine;
use crate::api::Order;
use std::fs::File;
use std::io::{BufReader, BufWriter};

pub fn save_snapshot(
    engine: &MatchingEngine,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create("snapshot.bin")?;

    let writer = BufWriter::new(file);

    bincode::serialize_into(writer, engine)?;

    Ok(())
}

pub fn load_snapshot(
) -> Result<MatchingEngine, Box<dyn std::error::Error>> {
    let file = File::open("snapshot.bin")?;

    let reader = BufReader::new(file);

    let engine: MatchingEngine =
        bincode::deserialize_from(reader)?;

    Ok(engine)
}
