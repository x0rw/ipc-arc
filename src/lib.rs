mod mutex;
use mutex::*;
pub mod ipc_arc;
mod mem_handler;

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use crate::ipc_arc::IpcArc;

    #[test]
    fn test_mas_writes() {
        struct Sh {
            inner: [u8; 100],
        }

        let shared_struct = Sh { inner: [99; 100] };
        let shared =
            IpcArc::<Sh>::create_or_open(format!("/crit-bench-wr").as_str(), shared_struct)
                .unwrap();
        let lock_aq = shared.lock();
        // *lock_aq = [7u8; 100];
        drop(lock_aq);
        shared.unlink().unwrap();
    }
    #[test]
    fn test_mutex_guard() {
        thread::spawn(|| {
            let shared = IpcArc::<i32>::create_or_open("/opened_shared", 1).unwrap();
            // shared.create_or_open("/opened_shared", 1).unwrap();
            let mut mu = shared.lock();
            *mu = *mu + 1;
            assert_eq!(
                shared.read_counter(),
                1,
                "The counter should be 2, unless there is a delay"
            );
            drop(mu);
            thread::sleep(Duration::from_millis(20));

            shared.unlink().unwrap();
        });

        thread::sleep(Duration::from_millis(10));
        let shared = IpcArc::<i32>::open("/opened_shared").unwrap();
        // shared.open("/opened_shared").unwrap();
        {
            let mut mu = shared.lock();
            *mu = *mu + 1;
            assert_eq!(*mu, 3);

            assert_eq!(
                shared.read_counter(),
                2,
                "The counter should be 2, unless there is a delay"
            );
        }

        let shared_counter = shared.read_counter();
        shared.unlink().unwrap();
        assert_eq!(shared_counter, 2);
    }

    #[test]
    fn test_mutex_guard_explicit_unlink_all_instances() {
        const NAME: &str = "/ipcarc_unlink_all_test";

        let arc1 = IpcArc::<i32>::create_or_open(NAME, 1).unwrap();
        {
            let mut g1 = arc1.lock();
            *g1 += 1;
            assert_eq!(*g1, 2, "arc1 incremented 1->2");
        }

        let arc2 = IpcArc::<i32>::open(NAME).unwrap();
        {
            let mut g2 = arc2.lock();
            *g2 += 1;
            assert_eq!(*g2, 3, "arc2 incremented 2->3");
        }

        let arc1_final_counter = arc1.read_counter();
        let arc2_final_counter = arc2.read_counter();
        arc1.unlink().unwrap();
        arc2.unlink().unwrap();

        assert_eq!(
            arc1_final_counter, 2,
            "The counter should be 2 before the unlinking"
        );

        assert_eq!(
            arc1_final_counter, 2,
            "The counter should be 2 before the unlinking"
        );
    }
    #[test]
    fn test_ipcarc_multiprocess_counter() {
        use nix::unistd::{ForkResult, fork};
        use std::process::exit;

        const NUM_PROCESSES: usize = 500;
        const INCREMENTS_PER_PROCESS: usize = 13;

        // Setup in parent
        let shared = IpcArc::<u64>::create_or_open("/hellos", 43).unwrap();

        let mut children = vec![];

        for _ in 0..NUM_PROCESSES {
            match unsafe { fork() } {
                Ok(ForkResult::Child) => {
                    for _ in 0..INCREMENTS_PER_PROCESS {
                        let child = IpcArc::<u64>::create_or_open("/hellos", 34).unwrap();
                        child.unlink().unwrap();
                    }

                    // println!(
                    //     "[child {}] counter: {}",
                    //     std::process::id(),
                    //     child.read_counter()
                    // );

                    // Do NOT unlink in child
                    exit(0);
                }
                Ok(ForkResult::Parent { child, .. }) => {
                    children.push(child);
                }
                Err(e) => panic!("fork failed: {e}"),
            }
        }

        // Wait for children
        for pid in children {
            nix::sys::wait::waitpid(pid, None).expect("waitpid failed");
        }

        // shared.force_unlink().unwrap();

        let final_counter = shared.read_counter() - 1; // the final counter before the last
        // decrement
        shared.unlink().unwrap();
        let expected = 0;
        // let expected = (NUM_PROCESSES * INCREMENTS_PER_PROCESS + 20 + 1) as u32;

        println!("[parent] final counter: {}", final_counter);

        assert_eq!(
            final_counter, expected,
            "Expected {}, got {}",
            expected, final_counter
        );
    }
}
