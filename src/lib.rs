pub mod advise;
pub mod db;
pub mod patch;
pub mod read;
pub mod save;

pub const HEADER_SIZE: usize = 0x300;
pub const SLOT_CHECKSUM_SIZE: usize = 0x10;
pub const SLOT_DATA_SIZE: usize = 0x280000;
pub const SLOT_STRIDE: usize = SLOT_CHECKSUM_SIZE + SLOT_DATA_SIZE;
pub const NUM_SLOTS: usize = 10;

pub fn slot_data_range(slot_idx: usize) -> (usize, usize) {
    let start = HEADER_SIZE + slot_idx * SLOT_STRIDE + SLOT_CHECKSUM_SIZE;
    (start, start + SLOT_DATA_SIZE)
}

pub fn slot_checksum_start(slot_idx: usize) -> usize {
    HEADER_SIZE + slot_idx * SLOT_STRIDE
}
