#![allow(clippy::unwrap_used)]

use codewalk::{CodeWalker, WalkConfig};
use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;

#[test]
fn test_concurrent_stress() {
    let num_threads = 32;
    let barrier = Arc::new(Barrier::new(num_threads + 1));

    // Set up a shared directory with some files
    let dir = Arc::new(tempfile::tempdir().unwrap());

    // Create an initial tree
    for i in 0..10 {
        let nested_dir = dir.path().join(format!("dir_{i}"));
        fs::create_dir_all(&nested_dir).unwrap();
        for j in 0..10 {
            let file_path = nested_dir.join(format!("file_{j}.txt"));
            fs::write(&file_path, format!("Content of {i}-{j}")).unwrap();
        }
    }

    let mut handles = vec![];

    for t_id in 0..num_threads {
        let b = Arc::clone(&barrier);
        let d = Arc::clone(&dir);

        handles.push(thread::spawn(move || {
            // Wait for all threads to be ready
            b.wait();

            // Depending on the thread, do different things to stress the engine
            if t_id % 4 == 0 {
                // Mutator thread: continually creates and deletes a file
                let path = d.path().join(format!("volatile_{t_id}.txt"));
                for _ in 0..50 {
                    if let Err(e) = fs::write(&path, "volatile data") {
                        assert!(
                            matches!(
                                e.kind(),
                                std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                            ),
                            "Expected valid IO error, got {:?}",
                            e
                        );
                    }
                    if let Err(e) = fs::remove_file(&path) {
                        assert!(
                            matches!(
                                e.kind(),
                                std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                            ),
                            "Expected valid IO error, got {:?}",
                            e
                        );
                    }
                }
            } else if t_id % 4 == 1 {
                // Mutator thread: deep nesting
                let path = d.path().join(format!("deep_nest_{t_id}"));
                if let Err(e) = fs::create_dir(&path) {
                    assert!(
                        matches!(
                            e.kind(),
                            std::io::ErrorKind::AlreadyExists | std::io::ErrorKind::NotFound
                        ),
                        "Expected valid IO error, got {:?}",
                        e
                    );
                }
                if let Err(e) = fs::write(path.join("deep.txt"), "deep") {
                    assert!(
                        matches!(
                            e.kind(),
                            std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                        ),
                        "Expected valid IO error, got {:?}",
                        e
                    );
                }
                if let Err(e) = fs::remove_file(path.join("deep.txt")) {
                    assert!(
                        matches!(
                            e.kind(),
                            std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                        ),
                        "Expected valid IO error, got {:?}",
                        e
                    );
                }
                if let Err(e) = fs::remove_dir(&path) {
                    assert!(
                        matches!(
                            e.kind(),
                            std::io::ErrorKind::NotFound
                                | std::io::ErrorKind::DirectoryNotEmpty
                                | std::io::ErrorKind::PermissionDenied
                        ),
                        "Expected valid IO error, got {:?}",
                        e
                    );
                }
            } else {
                // Reader thread: walk the tree
                let config = WalkConfig::default();
                let walker = CodeWalker::new(d.path(), config);
                let walk_result = walker.walk();

                // Concurrent mutation might cause walk to fail with a NotFound IO error.
                // We handle it gracefully.
                if let Err(e) = walk_result {
                    assert!(
                        matches!(
                            e,
                            codewalk::error::CodewalkError::Io(_)
                                | codewalk::error::CodewalkError::Ignore(_)
                        ),
                        "Expected IO or Ignore error during concurrent walk, got {:?}",
                        e
                    );
                    return;
                }
                let entries = walk_result.unwrap();

                // Assert something meaningful
                assert!(
                    !entries.is_empty(),
                    "Thread {} found 0 entries, expected at least the static ones",
                    t_id
                );

                // Stress the content reader
                for entry in entries.iter().take(5) {
                    let content_res = entry.content();
                    // We don't use `let _ = ...`, we explicitly match. It can fail if mutator thread deleted it.
                    match content_res {
                        Ok(content) => {
                            assert!(!content.is_empty(), "Read empty content where not expected");
                        }
                        Err(e) => {
                            assert!(
                                matches!(e, codewalk::error::CodewalkError::Io(_)),
                                "Expected IO error for volatile file, got {:?}",
                                e
                            );
                        }
                    }
                }
            }
        }));
    }

    // Release the threads!
    barrier.wait();

    // Wait for all threads to finish
    for handle in handles {
        let res = handle.join();
        assert!(res.is_ok(), "Thread panicked during stress test");
    }
}
