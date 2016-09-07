use super::{BlockHashes, Diff, Window};
use std::io::{Read, Write, Result};
use std::collections::HashMap;
use crypto::md5::Md5;
use crypto::digest::Digest;
use byteorder::{NetworkEndian, ByteOrder};

/// Implements a weak, but easy to calculate hash for a block of bytes
///
/// The hash is comprised of two bytes.  The first is the sum of the bytes
// in the block, the second is the sum of the sum of the bytes in the block
struct RollingHash {
    a: u16,
    b: u16,
    block_size: u16
}

impl RollingHash {

    /// Creates a new rolling hash over the bytes in `initial_data`.
    /// It will be assumed that the size of blocks will be the size of the initial data.
    pub fn new<'a, I: Iterator<Item=&'a u8>>(initial_data: I) -> RollingHash {

        let mut a:u16 = 0;
        let mut b:u16 = 0;
        let mut block_size: u16 = 0;
        for byte in initial_data {
            a = a.wrapping_add(*byte as u16);
            b = b.wrapping_add(a);
            block_size += 1;
        }
        RollingHash {
            a: a,
            b: b,
            block_size: block_size
        }
    }

    /// Gets the hash as it currently stands
    pub fn get_hash(&self) -> u32 {
        return (self.b as u32) << 16 | self.a as u32;
    }

    /// Roll the has forward one byte.  This function will remove `old_byte` from its calculation
    /// and add `new_byte` if it exists.
    /// To get the hash afterwards, use `get_hash()`.
    pub fn roll_hash(&mut self, new_byte: Option<u8>, old_byte: u8) {
        self.a = self.a.wrapping_sub(old_byte as u16);
        self.b = self.b.wrapping_sub(((old_byte as u16).wrapping_mul(self.block_size as u16)) as u16);
        if let Some(new_byte) = new_byte {
            self.a = self.a.wrapping_add(new_byte as u16);
            self.b = self.b.wrapping_add(self.a);
        } else {
            self.block_size -= 1
        }
    }

    /// Calculate the hash of a collection of bytes.
    pub fn hash_buffer(buffer: &[u8]) -> u32 {
        let mut a:u16 = 0;
        let mut b:u16 = 0;
        for byte in buffer {
            a = a.wrapping_add(*byte as u16);
            b = b.wrapping_add(a);

        }
        (b as u32) << 16 | a as u32
    }
}


impl BlockHashes {

    /// Create a new BlockHash based on the data in data_source.  This method
    /// will create a hash for every `block_size` set of bytes in `data_source`.
    ///
    /// To see the difference after `data_source` has been updated, use `diff_and_update()`
    ///
    /// This method returns an error when there is a problem reading from `data_source`.
    pub fn new<R: Read>(mut data_source: R, block_size: usize) -> Result<BlockHashes> {
        let mut block = vec![0;block_size];
        let mut hashes = HashMap::new();
        let mut block_index = 0;
        let mut strong_hasher = Md5::new();
        let mut total_size = 0;

        let mut read_size = try!(data_source.read(&mut block));
        while read_size > 0 {
            let weak_hash = RollingHash::hash_buffer(&block[..read_size]);

            let mut strong_hash:[u8;16] = [0;16];
            strong_hasher.reset();
            strong_hasher.input(&block[..read_size]);
            strong_hasher.result(&mut strong_hash);

            hashes.entry(weak_hash).or_insert(Vec::new()).push((block_index, strong_hash));

            block_index += 1;
            total_size += read_size;
            read_size = try!(data_source.read(&mut block));
        }
        Ok(BlockHashes {
            hashes: hashes,
            block_size: block_size,
            file_size: total_size
        })
    }

    /// Construct a new block hash for a file that was just created
    pub fn empty(block_size: usize) -> BlockHashes {
        BlockHashes {
            hashes: HashMap::new(),
            block_size: block_size,
            file_size: 0
        }
    }

    /// Compare the data in `new_data` with the hashes computed from either
    /// the most recent call to `diff_and_update()` or when this `BlockHashes` was updated
    ///
    /// # Example
    ///
    /// ```
    /// use rdiff::BlockHashes;
    /// use std::io::Cursor;
    /// let mut hashes = BlockHashes::new(Cursor::new("It was the best of times"), 6).unwrap();
    /// let diff = hashes.diff_and_update(Cursor::new("It was not the best of things")).unwrap();
    /// // prints (6, ' not') and (22, ' things'))
    /// for insert in diff.inserts() {
    ///     println!("{:?}", insert);
    /// }
    /// // prints (29, 6)
    /// for delete in diff.deletes() {
    ///     println!("{:?}", delete);
    /// }
    /// assert_eq!("It was not the best of things",
    ///             diff.apply_to_string("It was the best of times").unwrap());
    /// ```
    pub fn diff_and_update<R: Read>(&mut self, new_data: R) -> Result<Diff> {
        use std::mem;
        let mut diffs = Diff::new();
        let mut window = try!(Window::new(new_data, self.block_size));
        let mut weak_hasher = RollingHash::new(window.frame().0.iter());
        let mut strong_hasher = Md5::new();
        let mut last_matching_block_index = -1;
        let mut insert_buffer = Vec::new();
        let mut new_hashes = HashMap::new();
        let mut current_block_index = 0;
        while window.frame_size() > 0 {

            if let Some(other_block_index) = self.check_match(&weak_hasher, &mut strong_hasher, &mut window, &mut last_matching_block_index) {
                //create an insert if the insert buffer has anything in it
                if insert_buffer.len() > 0 {
                    // XXX with some work here, we could probably track the insert buffer as a piece of the window, which is then
                    // moved into the diff list.
                    diffs.add_insert(window.get_bytes_read() - insert_buffer.len(), mem::replace(&mut insert_buffer, Vec::new()));
                }
                //create a delete if the index is more than it should be
                if other_block_index as i32 > last_matching_block_index + 1 {
                    diffs.add_delete(window.get_bytes_read(), self.block_size * (other_block_index as i32 - last_matching_block_index - 1) as usize)
                }
                last_matching_block_index = other_block_index as i32;
                //advance forward an entire block's worth
                for i in 0..self.block_size {
                    if window.on_boundry() {
                        // This might iterate past the end of the data.  If so, bail out
                        if window.frame_size() == 0 {
                            break;
                        }
                        let mut strong_hash:[u8;16] = [0;16];
                        // If the boundry happened where we saw a match, we can skip the
                        // strong hashing, because it was already done during the
                        // match checking
                        if i != 0 {
                            let (front, back) = window.frame();
                            strong_hasher.reset();
                            strong_hasher.input(front);
                            strong_hasher.input(back);
                        }
                        strong_hasher.result(&mut strong_hash);

                        new_hashes.entry(weak_hasher.get_hash()).or_insert(Vec::new()).push((current_block_index, strong_hash));
                        current_block_index += 1;
                    }
                    let (tail, head) = try!(window.advance());
                    if let Some(tail) = tail {
                        weak_hasher.roll_hash(head, tail);
                    } else {
                        break;
                    }
                }
            } else {
                //advance forward one byte
                if window.on_boundry() {
                    // XXX There is a slight optimization possible here, where
                    // when the weak checksum matches, but the strong one doesn't
                    // we are re-computing the strong checksum here.
                    let mut strong_hash:[u8;16] = [0;16];
                    let (front, back) = window.frame();
                    strong_hasher.reset();
                    strong_hasher.input(front);
                    strong_hasher.input(back);
                    strong_hasher.result(&mut strong_hash);

                    new_hashes.entry(weak_hasher.get_hash()).or_insert(Vec::new()).push((current_block_index, strong_hash));
                    current_block_index += 1;
                }
                let (tail, head) = try!(window.advance());
                weak_hasher.roll_hash(head, tail.unwrap());
                insert_buffer.push(tail.unwrap());
            }
        }
        if insert_buffer.len() > 0 {
            diffs.add_insert(window.get_bytes_read() - insert_buffer.len(), insert_buffer);
        }
        let old_block_count = (self.file_size + self.block_size - 1) as i32 / self.block_size as i32;
        if last_matching_block_index + 1 < old_block_count {
            diffs.add_delete(window.get_bytes_read(), (self.file_size as i32 - (last_matching_block_index + 1) * self.block_size as i32) as usize);
        }
        self.hashes = new_hashes;
        self.file_size = window.get_bytes_read();
        Ok(diffs)
    }

    /// Checks if `data_source` has changed since the last time the hashes were updated.
    ///
    /// Returns true if `data_source` is identical to what it was when the hashes were generated, false otherwise
    pub fn verify_unchanged<R: Read>(&self, data_source: &mut R) -> Result<bool> {
        let mut block = vec![0;self.block_size];
        let mut block_index = 0;
        let mut strong_hasher = Md5::new();
        let mut total_size = 0;

        let mut read_size = try!(data_source.read(&mut block));
        while read_size > 0 {
            let weak_hash = RollingHash::hash_buffer(&block[..read_size]);
            if let Some(entry) = self.hashes.get(&weak_hash) {
                let mut strong_hash:[u8;16] = [0;16];
                strong_hasher.reset();
                strong_hasher.input(&block[..read_size]);
                strong_hasher.result(&mut strong_hash);
                if !entry.contains(&(block_index, strong_hash)) {
                    return Ok(false);
                }
            }


            block_index += 1;
            total_size += read_size;
            read_size = try!(data_source.read(&mut block));
        }
        Ok(total_size == self.file_size)
    }

    /// Compress these Hashes and write to `writer`.  The output can then be expanded
    /// back into an equivilent Hash collection using `expand_from()`
    pub fn compress_to<W: Write>(&self, writer: &mut W) -> Result<()> {

        let mut int_buf = [0;4];
        NetworkEndian::write_u32(&mut int_buf, self.file_size as u32);
        try!(writer.write(&int_buf));
        NetworkEndian::write_u32(&mut int_buf, self.block_size as u32);
        try!(writer.write(&int_buf));
        let block_count = (self.file_size + self.block_size - 1) / self.block_size;
        let dummy_hash = [0u8;16];
        let mut sequential_hashes = Vec::with_capacity(block_count);
        sequential_hashes.resize(block_count, (0, &dummy_hash));
        for (weak_hash, entry) in self.hashes.iter() {
            for &(index, ref strong_hash) in entry.iter() {
                sequential_hashes[index] = (*weak_hash, strong_hash);
            }
        }
        for (weak, strong) in sequential_hashes {
            NetworkEndian::write_u32(&mut int_buf, weak);
            try!(writer.write(&int_buf));
            try!(writer.write(strong));
        }
        Ok(())
    }

    /// Expand these hashes from previously compressed data in `reader`.  The data in reader
    /// should have been written using `compress_to()`
    pub fn expand_from<R: Read>(reader: &mut R) -> Result<BlockHashes> {
        let mut int_buf = [0;4];
        let mut strong_hash = [0u8;16];
        try!(reader.read(&mut int_buf));
        let file_size = NetworkEndian::read_u32(&mut int_buf) as usize;
        try!(reader.read(&mut int_buf));
        let block_size = NetworkEndian::read_u32(&mut int_buf) as usize;
        let block_count = (file_size + block_size - 1) / block_size;
        // Might be an overestimate, but not by more than a few
        let mut hashes = HashMap::with_capacity(block_count);

        for block_index in 0..block_count {
            try!(reader.read(&mut int_buf));
            let weak_hash = NetworkEndian::read_u32(&mut int_buf);
            try!(reader.read(&mut strong_hash));
            hashes.entry(weak_hash).or_insert(Vec::new()).push((block_index, strong_hash));
        }
        Ok(BlockHashes {
            file_size: file_size,
            block_size: block_size,
            hashes: hashes
        })
    }

    /// Checks if the current window frame matches any existing block with an index greater than the previously matched block.
    ///
    /// Returns the index of the matching block if it does
    fn check_match<R: Read>(&self, weak_hasher: &RollingHash, mut strong_hasher: &mut Md5, mut window: &Window<R>, last_matching_block_index: &mut i32) -> Option<usize> {
        if let Some(other_block_index) = self.hash_match(&weak_hasher, &mut strong_hasher, &mut window) {
            if other_block_index as i32 > *last_matching_block_index {
                return Some(other_block_index);
            }
        }
        None
    }

    /// Checks to see if the hash of the current window frame matches an existing hash.
    ///
    /// If so, returns the index of the matching block
    fn hash_match<R: Read>(&self, weak_hasher: &RollingHash,  strong_hasher: &mut Md5, window: &Window<R>) -> Option<usize> {
        let mut new_result = [0;16];
        if let Some(matches) = self.hashes.get(&weak_hasher.get_hash()) {
            for &(index, strong_hash) in matches.iter() {
                strong_hasher.reset();
                let (front, back) = window.frame();
                strong_hasher.input(front);
                strong_hasher.input(back);
                strong_hasher.result(&mut new_result);
                if new_result == strong_hash {
                    return Some(index)
                }
            }
        }
        return None
    }
}

#[cfg(test)]
mod test {
    use super::super::{BlockHashes, Diff, Insert, Delete};
    use super::{RollingHash};
    use std::io::{Cursor};
    use std::collections::HashMap;

    macro_rules! check_diff {
        ($start: tt | $block_size: tt | $new: tt | $(($insert_pos : tt, $insert_value: tt)),* | $(($delete_pos: tt, $delete_len: tt)),*) => {
            {
                check_diff_workaround!($start; $block_size; $new; $(($insert_pos, $insert_value)),*; $(($delete_pos, $delete_len)),*)
            }
        };
    }

    // Caused by a bug in the implementation of the tt macro type.  It currently has to be passed as an expr into another macro
    // or it throws a fit for no reason.  See https://github.com/rust-lang/rust/issues/5846
    macro_rules! check_diff_workaround {
        ($start: expr ; $block_size: expr ; $new: expr ; $(($insert_pos : tt, $insert_value: tt)),* ; $(($delete_pos: tt, $delete_len: tt)),*) => {
            {
                let mut hashes = BlockHashes::new(Cursor::new($start), $block_size).unwrap();
                let diff = hashes.diff_and_update(Cursor::new($new)).unwrap();
                assert_eq!(Diff {
                    inserts: vec![$(Insert{position: $insert_pos, data: $insert_value.bytes().collect()}),*],
                    deletes: vec![$(Delete{position: $delete_pos, len: $delete_len}),*]
                }, diff);
                check_hashes(&hashes, $new);
            }
        };
    }

    fn check_hashes(hashes: &BlockHashes, starting_data: &'static str) {
        let expected_hashes = BlockHashes::new(Cursor::new(starting_data), hashes.block_size).unwrap();
        assert_eq!(hashes, &expected_hashes);
    }

    #[test]
    fn rolling_hash_small() {
        let mut hash = RollingHash::new(vec![7, 2, 9, 1, 7, 8].iter());
        assert_eq!(hash.get_hash(), 0x710022); // a: 34 b: 113
        hash.roll_hash(Some(12), 7); // [2, 9, 1, 7, 8, 12]
        assert_eq!(hash.get_hash(), 0x6E0027); // a: 39 b:110
        hash.roll_hash(Some(1), 2); // [9, 1, 7, 8, 12, 1]
        assert_eq!(hash.get_hash(), 0x880026); // a: 38 b:136
        hash.roll_hash(None, 9); // [1, 7, 8, 12, 1]
        assert_eq!(hash.get_hash(), 0x52001D); // a: 29 b:82
        hash.roll_hash(None, 1); // [7, 8, 12, 1]
        assert_eq!(hash.get_hash(), 0x4D001C); // a: 28 b: 77
        hash.roll_hash(None, 7); // [8, 12, 1]
        assert_eq!(hash.get_hash(), 0x310015); // a: 21 b: 49
        hash.roll_hash(None, 8); // [12, 1]
        assert_eq!(hash.get_hash(), 0x19000D); // a: 13 b: 25
        hash.roll_hash(None, 12); // [1]
        assert_eq!(hash.get_hash(), 0x10001); // a: 1 b: 1
        hash.roll_hash(None, 1); // []
        assert_eq!(hash.get_hash(), 0x0); // a: 0 b: 0
    }
    #[test]
    fn rolling_hash_big() {
        let mut numbers = Vec::new();
        for i in 0..4000 {
            numbers.push((200 + i * i) as u8);
        }
        let mut hash = RollingHash::new(numbers.iter());
        assert_eq!(hash.get_hash(), 0x1880A9F0); // a: A9f0 b: 1880
        hash.roll_hash(Some(237), 200);
        assert_eq!(hash.get_hash(), 0x8D95AA15); // a: AA15 b: 8D95
        hash.roll_hash(None, 201);
        assert_eq!(hash.get_hash(), 0x48F5A94C) // a: A94C b: 48F5

    }

    #[test]
    fn hash_blocks_init() {
        let test_string = "It was the best of times, it was the worst of times";
        // Blocks:
        // It was t : 202900156 - ad721d63c3dabb32cc9096824071a919
        // he best  : 211944123 - 2712A22DDA5585758AEBC4D298142F8B
        // of times : 225313559 - 3160523454fa59e4c14badf9435d6212
        // , it was : 169083540 - 5fa8fa659adc38997bb365f17648ea8a
        //  the wor : 197788377 - d7aad88e1f5098bdae1da2e564749322
        // st of ti : 217580249 - 1c64811671e43ea5f82da6ffc4a5bbee
        // mes      : 42205509  - d2db8a610f8c7c0785d2d92a6e8c450e
        let hashes = BlockHashes::new(Cursor::new(test_string), 8).unwrap();

        let mut expected_hashes:HashMap<u32, Vec<(usize, [u8;16])>> = HashMap::new();
        expected_hashes.insert(202900156, vec![(0, [0xad, 0x72, 0x1d, 0x63, 0xc3, 0xda, 0xbb, 0x32, 0xcc, 0x90, 0x96, 0x82, 0x40, 0x71, 0xa9, 0x19])]);
        expected_hashes.insert(211944123, vec![(1, [0x27, 0x12, 0xA2, 0x2D, 0xDA, 0x55, 0x85, 0x75, 0x8A, 0xEB, 0xC4, 0xD2, 0x98, 0x14, 0x2F, 0x8B])]);
        expected_hashes.insert(225313559, vec![(2, [0x31, 0x60, 0x52, 0x34, 0x54, 0xfa, 0x59, 0xe4, 0xc1, 0x4b, 0xad, 0xf9, 0x43, 0x5d, 0x62, 0x12])]);
        expected_hashes.insert(169083540, vec![(3, [0x5f, 0xa8, 0xfa, 0x65, 0x9a, 0xdc, 0x38, 0x99, 0x7b, 0xb3, 0x65, 0xf1, 0x76, 0x48, 0xea, 0x8a])]);
        expected_hashes.insert(197788377, vec![(4, [0x6B, 0xF2, 0x9B, 0x2C, 0xD5, 0x03, 0x3E, 0xFC, 0x07, 0x9C, 0x2E, 0xA1, 0x27, 0xFD, 0x7B, 0x13])]);
        expected_hashes.insert(217580249, vec![(5, [0x1c, 0x64, 0x81, 0x16, 0x71, 0xe4, 0x3e, 0xa5, 0xf8, 0x2d, 0xa6, 0xff, 0xc4, 0xa5, 0xbb, 0xee])]);
        expected_hashes.insert(42205509,  vec![(6, [0xd2, 0xdb, 0x8a, 0x61, 0x0f, 0x8c, 0x7c, 0x07, 0x85, 0xd2, 0xd9, 0x2a, 0x6e, 0x8c, 0x45, 0x0e])]);

        assert_eq!(hashes, BlockHashes {
            hashes: expected_hashes,
            block_size: 8,
            file_size: 51
        });
    }


    #[test]
    fn empty_hashes() {
        check_diff!("" |
                    16 |
                    "The New Data" |
                    (0, "The New Data") |

                );
    }

    #[test]
    fn no_change() {
        check_diff!("Same Data" |
                    8 |
                    "Same Data" |
                    |

                );
    }

    #[test]
    fn multiple_overwrites() {
        check_diff!("" |
                    8 |
                    "New Data" |
                    (0, "New Data")|

                );
        check_diff!("New Data" |
                    8 |
                    "Other Stuff" |
                    (0, "Other Stuff")|
                    (11, 8)
                );
        check_diff!("Other Stuff" |
                    8 |
                    "More Things" |
                    (0, "More Things")|
                    (11, 11)
                );
    }

    #[test]
    fn insertions() {
        check_diff!("Starting data is a long sentence" |
                    8 |
                    "Starting data is now a long sentence" |
                    (16, " now") |

                );
        check_diff!("Starting data is a long sentence" |
                    8 |
                    "This Starting data is a long sentence" |
                    (0, "This ") |

                );
        check_diff!("Starting data is a long sentence" |
                    8 |
                    "Starting data is a long sentence. With more" |
                    (32, ". With more") |

                );
        check_diff!("Starting data is a long sentence" |
                    8 |
                    "This Starting data is now a long sentence. With more" |
                    (0, "This "),
                    (21, " now"),
                    (41, ". With more") |

                );
    }

    #[test]
    fn delete_on_boundry() {
        check_diff!("13 chars long, no longer" |
                    13 |
                    "13 chars long" |
                    |
                    (13, 11)
                );
    }

    #[test]
    fn deletions() {
        check_diff!("Starting data is a long sentence" |
                    8 |
                    "Starting a long sentence" |
                    |
                    (8, 8)
                );
        check_diff!("Starting data is a long sentence" |
                    8 |
                    "Starting data is a long " |
                    |
                    (24, 8)
                );
        check_diff!("Starting data is a long sentence" |
                    8 |
                    " data is a long sentence" |
                    |
                    (0, 8)
                );
        check_diff!("Starting data is a long sentence" |
                    8 |
                    " a long " |
                    |
                    (0, 16), (8, 8)
                );

    }

    #[test]
    fn insertions_and_deletions() {
        check_diff!("Starting data is a long sentence" |
                    8 |
                    "Starting data a long sentence" |
                    (8, " data") |
                    (13, 8)
                );
        check_diff!("Starting data is a long sentence" |
                    8 |
                    "Starting data is a long sentenc" |
                    (24, "sentenc")|
                    (31, 8)
                );
        check_diff!("Starting data is a long sentence" |
                    8 |
                    "This Starting data a very long sentence" |
                    (0, "This "), (13, " data a very long ") |
                    (31, 16)
                );

    }
}
