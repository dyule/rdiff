extern crate notify;
extern crate rdiff;

use notify::{RecommendedWatcher, Watcher, op};
use std::sync::mpsc::channel;
use std::fs;
use std::io;
use rdiff::BlockHashes;

macro_rules! try_io {
    ($e: expr) => ({
        match $e {
            Ok(v) => v,
            Err(e) => return Err(notify::Error::Io(e))
        }
    });
}

fn create_hashes(file: &str) -> io::Result<rdiff::BlockHashes> {
    let file = try!(fs::File::open(file));
    BlockHashes::new(file, 8)
}

fn update_hashes(hashes: &mut BlockHashes, file: &str) -> io::Result<()> {
    let file = try!(fs::File::open(file));
    let diffs = try!(hashes.diff_and_update(file));
    if diffs.inserts().len() != 0 || diffs.deletes().len() != 0 {
        println!("{:?}", diffs);
    }
    Ok(())
}

fn watch(file_name: &str) -> notify::Result<()> {

    let mut hashes = try_io!(create_hashes(file_name));
  // Create a channel to receive the events.
  let (tx, rx) = channel();

  // Automatically select the best implementation for your platform.
  // You can also access each implementation directly e.g. INotifyWatcher.
  let mut watcher: RecommendedWatcher = try!(Watcher::new(tx));

  // Add a path to be watched. All files and directories at that path and
  // below will be monitored for changes.
  try!(watcher.watch(file_name));

  // This is a simple loop, but you may want to use more complex logic here,
  // for example to handle I/O.
  loop {
      match rx.recv() {
        Ok(notify::Event{ path: Some(_),op:Ok(operation) }) => {
            if operation == op::WRITE {
                try_io!(update_hashes(&mut hashes, file_name));
            }
        },
        Err(e) => println!("watch error {}", e),
        _ => ()
      }
  }
}

fn main() {
    let args:Vec<_> = std::env::args().collect();
    if args.len() != 2 {
        println!("Usage: file_watcher <file_name>");
        return;
    }

  if let Err(err) = watch(&args[1]) {
    println!("Error! {:?}", err)
  }
}
