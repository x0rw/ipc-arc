use criterion::{Criterion, criterion_group, criterion_main};
use ipc_arc::{self, ipc_arc::IpcArc};
use std::hint::black_box;

fn run_ipc(n: u64) {
    let shared = IpcArc::<u64>::create_or_open(format!("/crit-bench").as_str(), n).unwrap();
    shared.unlink().unwrap();
}

fn run_ipc_with_lock(n: u64) {
    let shared = IpcArc::<u64>::create_or_open(format!("/crit-bench-1").as_str(), n).unwrap();
    let mut lock_aq = shared.lock();
    *lock_aq = 3003;
    drop(lock_aq);

    shared.unlink().unwrap();
}

fn ipc_writes() {
    let shared =
        IpcArc::<[u8; 10000]>::create_or_open(format!("/crit-bench-wr").as_str(), [8u8; 10000])
            .unwrap();
    let mut lock_aq = shared.lock();
    *lock_aq = [7u8; 10000];
    drop(lock_aq);

    shared.unlink().unwrap();
}
fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("create_ipc_run-1", |b| b.iter(|| run_ipc(black_box(30))));
    c.bench_function("ipc-write", |b| b.iter(|| ipc_writes()));
    c.bench_function("create_ipc_run-2", |b| {
        b.iter(|| run_ipc_with_lock(black_box(3000)))
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
