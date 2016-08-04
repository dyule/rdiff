//! Finds the difference between sequential versions of files.
//!
//! Based on the rsync algorithm.
//! The `BlockHashes` struct will find the differences between versions of the same file.
//! It does this through the [`diff_and_update()`](struct.BlockHashes.html#method.diff_and_update) method.
//!
//! # Example
//!
//! ```
//! use std::io::Cursor;
//! use rdiff::BlockHashes;
//!
//! let mut hash = BlockHashes::new(Cursor::new("The initial version"), 8).unwrap();
//! let diffs = hash.diff_and_update(Cursor::new("The next version")).unwrap();
//! println!("Diffs: {:?}", diffs);
//! // Outputs "Diffs: Diff{inserts: [Insert(0, The next vers)], deletes:[Delete(13, 16)]}"
//! ```
//!
//! This crate also contains methods relating to finding the differences between two strings, in the [hirschberg](hirschberg/index.html) module.
//! These methods can be used to refine the course differences found through the rsync method.

#![deny(missing_docs)]
extern crate crypto;
#[macro_use]
extern crate log;

mod window;
mod hashing;
pub mod hirschberg;

use std::collections::HashMap;
use std::io::Read;
use std::slice::Iter;
use std::fmt;
use std::mem;
use std::string::FromUtf8Error;

/// Used for calculating and re-calculating the differences between two versions of the same file
///
/// See the [module level documentation](index.html) for examples on how to use this
#[derive(Debug, PartialEq)]
pub struct BlockHashes {
    hashes: HashMap<u32, Vec<(usize, [u8; 16])>>,
    block_size: usize,
    file_size: usize
}

/// Represents an operation to insert bytes at a particular position into a file
#[derive(PartialEq)]
pub struct Insert {
    position: usize,
    data: Vec<u8>
}

/// Represents an operation to delete a certain number of bytes at a particular position in a file
#[derive(PartialEq)]
pub struct Delete {
    position: usize,
    len: usize
}

/// Represents a series of operations that were performed on a file to transform it into a new
/// version.
///
/// The operations are stored in file order, which means that every operation that affects
/// an earlier part of the file must be stored before an operation that affects a later part.
/// The diff also assumes that insert operations are performed prior to delete operations.
#[derive(Debug, PartialEq)]
pub struct Diff {
    inserts: Vec<Insert>,
    deletes: Vec<Delete>
}

/// A sliding window over a reader.  This monatins an internal buffer read from the file,
/// which can be read from at any time.
struct Window<R: Read> {
    front: Vec<u8>,
    back: Vec<u8>,
    block_size: usize,
    offset: usize,
    bytes_read: usize,
    reader: R
}

impl Diff {
    /// Creates a new `Diff`
    #[inline]
    pub fn new() -> Diff {
        Diff {
            inserts: Vec::new(),
            deletes: Vec::new()
        }
    }

    /// Adds an insert operation into this diff.  The operation must occur after
    /// all previously added insert operations in file order.  If the operation
    /// can be merged with the previous operation, then it is.
    ///
    /// Consumes the data that is passed in
    fn add_insert(&mut self, position: usize, mut data: Vec<u8>) {
        if let Some(tail) = self.inserts.last_mut() {
            if tail.position + tail.data.len() == position {
                tail.data.append(&mut data);
                return;
            }
        }
        self.inserts.push(Insert {
            position: position,
            data: data
        });
    }

    // Adds an delete operation into this diff.  The operation must occur after
    /// all previously added insert and delete operations in file order.  If the operation
    /// can be merged with the previous operation, then it is.
    fn add_delete(&mut self, position: usize, len: usize) {
        if let Some(tail) = self.deletes.last_mut() {
            if tail.position  == position {
                tail.len += len;
                return;
            }
        }
        self.deletes.push(Delete {
            position: position,
            len: len
        });
    }

    /// Gets an iterator over all insert operations
    pub fn inserts(&self) -> Iter<Insert> {
        self.inserts.iter()
    }

    /// Gets an iterator over all delete operations
    pub fn deletes(&self) -> Iter<Delete> {
        self.deletes.iter()
    }

    /// Applies all of the operations in the diff to the given string.
    /// Gives an error if the resulting string can't be represented by ut8.
    ///
    /// # Panics
    /// When the operations refer to positions that are not represented by the string.
    pub fn apply_to_string(&self, string: &str) -> Result<String, FromUtf8Error> {
        let mut old_bytes = string.bytes();
        let mut new_bytes = Vec::new();
        let mut index = 0;
        for insert in self.inserts() {
            while index < insert.position {
                new_bytes.push(old_bytes.next().unwrap().clone());
                index += 1;
            }
            new_bytes.append(&mut insert.data.clone());
            index += insert.data.len();
        }
        while let Some(byte) = old_bytes.next() {
            new_bytes.push(byte);
        }
        let old_bytes = mem::replace(&mut new_bytes, Vec::new());
        let mut  old_bytes = old_bytes.into_iter();
        index = 0;
        for delete in self.deletes() {
            while index < delete.position {
                new_bytes.push(old_bytes.next().unwrap());
                index += 1;
            }
            for _ in 0..delete.len {
                old_bytes.next();
            }
        }
        while let Some(byte) = old_bytes.next() {
            new_bytes.push(byte);
        }
        String::from_utf8(new_bytes)
    }
}

impl fmt::Debug for Insert {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Insert({}, '{}')", self.position, String::from_utf8_lossy(&self.data).replace('\r', "").replace('\n', "\\n"))
    }
}

impl fmt::Debug for Delete {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Delete({}, {})", self.position, self.len)
    }
}

#[cfg(test)]
mod test {
    use super::Diff;




    #[test]
    fn applying_diff_to_string() {
        let string = "Mr. and Mrs. Dursley, of number four, Privet Drive, were proud to say that they were perfectly normal, thank you very much. They were the last people you'd expect to be involved in anything strange or mysterious, because they just didn't hold with such nonsense.";
        let mut diff = Diff::new();
        diff.add_insert(2, vec![115]); // 's'
        diff.add_insert(37, vec![116, 121]); //'ty'
        diff.add_insert(98, vec![97, 98]); // ab
        diff.add_insert(253, vec![109]); // m
        diff.add_delete(35, 1); // 'u'
        diff.add_delete(181, 34);
        diff.add_delete(219, 1);
        let result = diff.apply_to_string(string).unwrap();
        assert_eq!(result, "Mrs. and Mrs. Dursley, of number forty, Privet Drive, were proud to say that they were perfectly abnormal, thank you very much. They were the last people you'd expect to be involved, because they just didn't hold with much nonsense.".to_string());
    }
}
