use once_cell::sync::OnceCell;
use std::sync::RwLock;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use aliasable::boxed::AliasableBox;
use std::panic::catch_unwind;

pub type SourceId = usize;

#[derive(Debug,PartialEq,Eq,Hash)]
pub struct Loc {
    pub src:SourceId,
    pub start:usize,
    pub end:usize,
}

#[derive(Debug,PartialEq,Eq,Hash)]
pub enum SourceType {
    Macro(Loc),
    File(Arc<Path>),//name
    Repl,
}

pub type FileRes<T> = Result<T,Arc<std::io::Error>>;

type FileWaiter = OnceCell<FileRes<String>>;

#[derive(Default)]
pub struct FileArena(RwLock<HashMap<Arc<Path>,AliasableBox<FileWaiter>>>);
impl FileArena {
    pub fn new()->Self{
        Self::default()
    }

    fn get_waiter(&self,path:Arc<Path>)->&FileWaiter {
        let mut map = self.0.write().unwrap();
        let b:&AliasableBox<_>= map.entry(path).or_insert_with(||{
            Box::new(OnceCell::new()).into()
        });
        let p : *const FileWaiter = &**b;
        unsafe{&*p}
    }

    pub fn get<'a>(&'a self,path:Arc<Path>)->FileRes<&'a str>{
        let waiter = self.get_waiter(path.clone());
        let r = waiter.get_or_init(||{Ok(std::fs::read_to_string(path)?)});
        match r{
            Err(e)=>Err(e.clone()),
            Ok(s) => {
                let p:*const str = s.as_str();
                Ok(unsafe{&*p})
            }
        }

    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs::{self, OpenOptions},
        io::Write,
        path::Path,
        sync::{Arc, Barrier},
        thread,
        time::{Duration, Instant},
    };
    use tempfile::tempdir;

    #[test]
    fn file_read_arena() {
        const THREADS: usize = 8;
        const READERS_ON_SLOW: usize = 3; // some readers must hit the slow file

        let arena = Arc::new(FileArena::new());
        let dir = tempdir().unwrap();

        let fast1: Arc<Path> = dir.path().join("fast1.txt").into();
        let fast2: Arc<Path> = dir.path().join("fast2.txt").into();
        let slow: Arc<Path> = dir.path().join("slow.txt").into();

        fs::write(&fast1, "fast data").unwrap();
        fs::write(&fast2, "fast data").unwrap();
        fs::write(&slow, "slow data").unwrap();

        let start_barrier = Arc::new(Barrier::new(THREADS + 1));

        // writer holds slow file open *and locked* briefly
        let slow_writer = {
            let slow = Arc::clone(&slow);
            thread::spawn(move || {
                use fs2::FileExt;
                let mut file = OpenOptions::new().write(true).open(&*slow).unwrap();

                // Acquire exclusive lock (blocks others with shared or exclusive requests)
                file.lock_exclusive().unwrap();

                file.write_all(b"writing...").unwrap();
                file.flush().unwrap();
                thread::sleep(Duration::from_millis(300));

                // Unlock before drop to be explicit
                file.unlock().unwrap();
            })
        };

        let mut threads = Vec::new();
        for t in 0..THREADS {
            let arena = Arc::clone(&arena);
            let start_barrier = Arc::clone(&start_barrier);
            let path = if t < READERS_ON_SLOW {
                Arc::clone(&slow)
            } else if t % 2 == 0 {
                Arc::clone(&fast1)
            } else {
                Arc::clone(&fast2)
            };

            threads.push(thread::spawn(move || {
                start_barrier.wait();
                let start = Instant::now();
                for _ in 0..10 {
                    let res = arena.get(Arc::clone(&path));
                    assert!(res.is_ok(), "get() failed on {:?}", path);
                }
                start.elapsed()
            }));
        }

        start_barrier.wait();

        for j in threads {
            let elapsed = j.join().unwrap();
            assert!(elapsed < Duration::from_secs(2), "Thread took too long");
        }

        slow_writer.join().unwrap();
    }
}




// #[derive(Debug, PartialEq, Eq)]
// pub struct FileExists(pub String);

// #[derive(Default)]
// pub struct FileArena(RwLock<HashMap<Arc<Path>,String>>);
// impl FileArena {
//     pub fn new()->Self{
//         Self::default()
//     }

//     pub fn alloc(&self,path:Arc<Path>,s:String)->Result<&str,FileExists>{
//         use std::collections::hash_map::Entry;

//         let mut map = self.0.write().unwrap();
//         let p :*const str = match map.entry(path) {
//             Entry::Occupied(_) => return Err(FileExists(s)),
//             Entry::Vacant(v) => v.insert(s).as_str(),
//         };

//         Ok(unsafe{&*p})
//     }

//     pub fn get<'a>(&'a self,path:&Path)->Option<&'a str>{
//         let map = self.0.read().unwrap();
//         let p :*const str = map.get(path)?.as_str();
//         Some(unsafe{&*p})
//     }
// }
// #[cfg(test)]
// mod tests {
//     use super::*;
//     use std::{
//         path::Path,
//         sync::{Arc, Barrier},
//         thread,
//     };

//     #[test]
//     fn arena_multithread_basic() {
//         const THREADS: usize = 8;
//         const ITERS: usize = 32;

//         let arena = Arc::new(FileArena::new());
//         let shared: Arc<Path> = Arc::from(Path::new("/tmp/shared.txt"));
//         let barrier = Arc::new(Barrier::new(THREADS));

//         // Initial insert succeeds
//         assert_eq!(arena.alloc(shared.clone(), "initial".into()).unwrap(), "initial");

//         // Spawn several threads doing concurrent inserts + gets
//         let mut threads = Vec::new();
//         for tid in 0..THREADS {
//             let arena = Arc::clone(&arena);
//             let shared = Arc::clone(&shared);
//             let barrier = Arc::clone(&barrier);

//             threads.push(thread::spawn(move || {
//                 // Wait until all threads are ready, start together
//                 barrier.wait();

//                 for iter in 0..ITERS {
//                     // 50% chance: read shared value
//                     if iter % 2 == 0 {
//                         let got = arena.get(&shared).unwrap();
//                         assert_eq!(got, "initial");
//                     } else {
//                         // Try re-inserting same shared key (must fail)
//                         let e = arena
//                             .alloc(shared.clone(), format!("dupe-{tid}-{iter}"))
//                             .unwrap_err();
//                         assert_eq!(e.0, format!("dupe-{tid}-{iter}"));
//                     }

//                     // Every few iterations: insert a unique path
//                     if iter % 8 == 0 {
//                         let unique: Arc<Path> =
//                             Arc::from(Path::new(&format!("/tmp/uniq_{tid}_{iter}.txt")));
//                         let got = arena
//                             .alloc(unique.clone(), format!("ok-{tid}-{iter}"))
//                             .unwrap();
//                         assert_eq!(got, format!("ok-{tid}-{iter}"));
//                         // Verify it can be read back immediately
//                         assert_eq!(arena.get(&unique).unwrap(), format!("ok-{tid}-{iter}"));
//                     }
//                 }
//             }));
//         }

//         for t in threads {
//             t.join().unwrap();
//         }

//         // Reinserting shared should still fail
//         let e = arena.alloc(shared, "ignored".into()).unwrap_err();
//         assert_eq!(e.0, "ignored");
//     }
// }
