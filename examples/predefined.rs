extern crate rdiff;

use rdiff::BlockHashes;
use std::fs::File;

pub fn main() {
    let file = File::open("examples/filev1.txt").unwrap();
    let mut hashes = BlockHashes::new(file, 8).unwrap();
    let file = File::open("examples/filev2.txt").unwrap();
    let difference = hashes.diff_and_update(file).unwrap();
    println!("Inserts: {:?}", difference.inserts().collect::<Vec<_>>());
    println!("Deletes: {:?}", difference.deletes().collect::<Vec<_>>());
}
