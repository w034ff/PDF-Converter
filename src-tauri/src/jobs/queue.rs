//! The queue that the threads of a conversion take their items from (design §6.5).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};

use crate::worker_pool::default_worker_limit;

/// How a thread left the item it took from the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TaskOutcome {
    Succeeded,
    Failed,
    NoPages,
    /// The item was not done because of a cancel or a write failure. The stop check at the top
    /// of the thread's loop takes the items still in the queue.
    Unprocessed,
}

/// How many items ended in each [`TaskOutcome`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct QueueTally {
    pub succeeded: u32,
    pub failed: u32,
    pub no_pages: u32,
    pub unprocessed: u32,
}

#[derive(Default)]
struct Counters {
    succeeded: AtomicU32,
    failed: AtomicU32,
    no_pages: AtomicU32,
    unprocessed: AtomicU32,
}

/// Runs `work` on every item of `tasks`, on as many threads as the worker pool has workers, and
/// returns how the items ended.
///
/// A thread that sees `cancel_flag` or `write_failed` raised at the top of its loop gives up
/// the items still in the queue as unprocessed and stops, so `work` only has to return
/// [`TaskOutcome::Unprocessed`] for the item it was on. An item whose thread panicked is counted
/// as failed; the panic itself is not passed on.
pub(super) fn run_queue<Task, Work>(
    cancel_flag: &AtomicBool,
    write_failed: &AtomicBool,
    tasks: VecDeque<Task>,
    work: Work,
) -> QueueTally
where
    Task: Send,
    Work: Fn(Task) -> TaskOutcome + Sync,
{
    let total_items = tasks.len() as u32;
    let queue = Mutex::new(tasks);
    let counters = Counters::default();

    let num_threads = default_worker_limit().min(total_items as usize).max(1);
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..num_threads)
            .map(|_| {
                scope.spawn(|| take_tasks(cancel_flag, write_failed, &queue, &counters, &work))
            })
            .collect();
        // Joined here, so that the scope does not panic again for a thread that panicked.
        for handle in handles {
            let _ = handle.join();
        }
    });

    let mut tally = QueueTally {
        succeeded: counters.succeeded.load(Ordering::SeqCst),
        failed: counters.failed.load(Ordering::SeqCst),
        no_pages: counters.no_pages.load(Ordering::SeqCst),
        unprocessed: counters.unprocessed.load(Ordering::SeqCst),
    };
    let accounted = tally.succeeded + tally.failed + tally.no_pages + tally.unprocessed;
    if accounted < total_items {
        tally.failed += total_items - accounted;
    }
    tally
}

fn take_tasks<Task, Work>(
    cancel_flag: &AtomicBool,
    write_failed: &AtomicBool,
    queue: &Mutex<VecDeque<Task>>,
    counters: &Counters,
    work: &Work,
) where
    Work: Fn(Task) -> TaskOutcome,
{
    loop {
        if cancel_flag.load(Ordering::SeqCst) || write_failed.load(Ordering::SeqCst) {
            let mut q = lock(queue);
            counters
                .unprocessed
                .fetch_add(q.len() as u32, Ordering::SeqCst);
            q.clear();
            break;
        }

        let task = lock(queue).pop_front();
        let Some(task) = task else {
            break;
        };

        let counter = match work(task) {
            TaskOutcome::Succeeded => &counters.succeeded,
            TaskOutcome::Failed => &counters.failed,
            TaskOutcome::NoPages => &counters.no_pages,
            TaskOutcome::Unprocessed => &counters.unprocessed,
        };
        counter.fetch_add(1, Ordering::SeqCst);
    }
}

/// The queue holds plain items, so one that a panicking thread left locked is still sound.
fn lock<Task>(queue: &Mutex<VecDeque<Task>>) -> MutexGuard<'_, VecDeque<Task>> {
    match queue.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
