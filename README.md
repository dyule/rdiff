rdiff
=====
[![CC0](http://i.creativecommons.org/p/zero/1.0/88x31.png)](http://creativecommons.org/publicdomain/zero/1.0/)
[![Build Status](https://travis-ci.org/dyule/rdiff.svg?branch=master)](https://travis-ci.org/dyule/rdiff)
[![Crates.io](https://img.shields.io/crates/v/rdiff.svg?maxAge=2592000)](https://crates.io/crates/rdiff)

rdiff is a package for comparing versions of a file over time.  It is written is Rust, and expects version > 1.8.

To the extent possible under law, rdiff contributors have waived all copyright and related or neighboring rights to rdiff.

# Usage

in `Cargo.toml`:

``` toml
[dependencies]
rdiff = "0.1"
```

In your rust file (taken from [examples/predefined.rs]):

``` rust
extern crate rdiff;

use rdiff::BlockHashes;
use std::fs::File;

pub fn example() {
    let file = File::open("examples/filev1.txt").unwrap();
    let mut hashes = BlockHashes::new(file, 8).unwrap();
    let file = File::open("examples/filev2.txt").unwrap();
    let difference = hashes.diff_and_update(file).unwrap();
    println!("Inserts: {:?}", difference.inserts().collect::<Vec<_>>());
    println!("Deletes: {:?}", difference.deletes().collect::<Vec<_>>());
}
```

This will output
```
Inserts: [Insert(8, 'widely understood '), Insert(90, ' absolutely'), Insert(381, 'hters, or sons if the family was progressive.\n'), Insert(572, 'not, even though he had been following the news quite closely.\n\n'), Insert(734, '\nMr. Ben')]
Deletes: [Delete(34, 24), Delete(428, 8), Delete(638, 8), Delete(742, 8)]
```
