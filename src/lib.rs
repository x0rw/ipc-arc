mod mutex;
use mutex::*;
mod ipc_arc;
use ipc_arc::*;

#[cfg(test)]
mod tests {
    use std::{ops::Add, thread, time::Duration};

    use crate::ipc_arc::IpcArc;

    #[test]
    fn test_mutex_guard() {
        thread::spawn(|| {
            let mut shared = IpcArc::<i32>::new();
            shared.create_or_open("/opened_shared", 1).unwrap();
            let mut mu = shared.lock();
            *mu = *mu + 1;
            thread::sleep(Duration::from_secs(2));
            shared.unlink().unwrap();
        });

        thread::sleep(Duration::from_secs(1));
        let mut shared = IpcArc::<i32>::new();
        shared.open("/opened_shared").unwrap();
        {
            let mut mu = shared.lock();
            *mu = *mu + 1;
            assert_eq!(*mu, 3);
        }

        thread::sleep(Duration::from_secs(2));
        shared.unlink().unwrap();
        assert_eq!(shared.read_counter(), 0);
    }

    #[test]
    fn test_mutex_guard_explicit_unlink_all_instances() {
        const NAME: &str = "/ipcarc_unlink_all_test";

        let mut arc1 = IpcArc::<i32>::new();
        arc1.create_or_open(NAME, 1).unwrap();
        {
            let mut g1 = arc1.lock();
            *g1 += 1;
            assert_eq!(*g1, 2, "arc1 incremented 1->2");
        }

        let mut arc2 = IpcArc::<i32>::new();
        arc2.open(NAME).unwrap();
        {
            let mut g2 = arc2.lock();
            *g2 += 1;
            assert_eq!(*g2, 3, "arc2 incremented 2->3");
        }

        arc1.unlink().unwrap();
        arc2.unlink().unwrap();

        assert_eq!(
            arc1.read_counter(),
            0,
            "after unlinking ALL instances, counter must be 0"
        );
    }
    #[test]
    fn test_ipcarc_multiprocess_counter() {
        use super::*;

        use nix::unistd::{ForkResult, fork};
        use std::process::exit;

        const NUM_PROCESSES: usize = 50;
        const INCREMENTS_PER_PROCESS: usize = 13;

        // Setup in parent
        let mut shared = IpcArc::<u64>::new();

        // shared.force_unlink().unwrap();
        shared.create_or_open("/hellos", 42).unwrap();

        let mut children = vec![];

        for _ in 0..NUM_PROCESSES {
            match unsafe { fork() } {
                Ok(ForkResult::Child) => {
                    for _ in 0..INCREMENTS_PER_PROCESS {
                        let mut child = IpcArc::<u64>::new();
                        child.create_or_open("/hellos", 34).unwrap();
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

        shared.unlink().unwrap();
        // shared.force_unlink().unwrap();

        let final_counter = shared.read_counter();
        let expected = 0;
        // Expected: before RAII
        // let expected = (NUM_PROCESSES * INCREMENTS_PER_PROCESS + 20 + 1) as u32;

        println!("[parent] final counter: {}", final_counter);

        assert_eq!(
            final_counter, expected,
            "Expected {}, got {}",
            expected, final_counter
        );
    }
}
