//! The memory budget for reading image pixels in the main process (design §5.3).
//!
//! The main process has no memory limit like the workers', and a failed allocation ends the
//! app. Every read of image pixels (the PDF writer and the list thumbnails) therefore borrows
//! its estimated memory from one [`DecodeBudget`] before it reads the file and returns it when
//! it lets go of the pixels.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use pdfconv_worker::server::WORKER_MEMORY_LIMIT;

/// What one image-to-PDF write holds at its peak, as a multiple of the decoded pixel bytes:
/// the measured peak is 3.9 times, and 4 is the round value above it (design §5.3).
pub const DECODE_MEMORY_FACTOR_PDF: u64 = 4;

/// What one list thumbnail holds at its peak, as a multiple of the decoded pixel bytes: the
/// measured peak is 1.9 times, and 2 is the round value above it (design §5.3).
pub const DECODE_MEMORY_FACTOR_THUMBNAIL: u64 = 2;

/// The smallest budget: what one worker may use, for machines with little memory and for when
/// the installed memory cannot be read (design §5.3).
pub const DECODE_MEMORY_BUDGET_MIN: u64 = WORKER_MEMORY_LIMIT;

/// The largest budget: the size at which `MAX_WORKERS` images of the largest allowed size and
/// colour depth run side by side; a larger one would not let more of them run (design §5.3).
pub const DECODE_MEMORY_BUDGET_MAX: u64 = 16 * 1024 * 1024 * 1024;

/// The budget is this fraction (1/n) of the installed memory, which leaves the rest to the OS,
/// the other apps, the WebView and the workers (design §5.3).
const INSTALLED_MEMORY_DIVISOR: u64 = 3;

/// How often a request that waits for the budget asks whether it was cancelled (NFR-02).
const CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// The budget for a machine with `installed` bytes of memory, or the smallest budget if the
/// amount is unknown (design §5.3).
pub fn budget_for(installed: Option<u64>) -> u64 {
    installed.map_or(DECODE_MEMORY_BUDGET_MIN, |installed| {
        (installed / INSTALLED_MEMORY_DIVISOR)
            .clamp(DECODE_MEMORY_BUDGET_MIN, DECODE_MEMORY_BUDGET_MAX)
    })
}

/// Memory that image decoding may use at the same time, lent in the order it was asked for.
pub struct DecodeBudget {
    shared: Arc<Shared>,
}

struct Shared {
    budget: u64,
    state: Mutex<State>,
    changed: Condvar,
}

#[derive(Default)]
struct State {
    /// Bytes lent and not yet returned.
    used: u64,
    /// The ticket given to the next request.
    next_ticket: u64,
    /// Tickets of the requests that wait, in the order they were made.
    queue: VecDeque<u64>,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether `bytes` can be lent now: they fit in what is left, or they are larger than the
    /// whole budget and nothing else is lent.
    fn fits(&self, used: u64, bytes: u64) -> bool {
        used.saturating_add(bytes) <= self.budget || (bytes > self.budget && used == 0)
    }
}

impl DecodeBudget {
    /// A budget of `budget` bytes.
    pub fn new(budget: u64) -> Self {
        Self {
            shared: Arc::new(Shared {
                budget,
                state: Mutex::new(State::default()),
                changed: Condvar::new(),
            }),
        }
    }

    /// Lends `bytes`, waiting until it is this request's turn and they are free. The returned
    /// permit gives them back when it is dropped.
    ///
    /// Requests are served in the order they were made: while the first one waits, the later
    /// ones wait too, even if there is room for them. A request larger than the whole budget
    /// is not refused; it is served once nothing else is lent, and then it is the only one.
    ///
    /// While it waits, `should_cancel` is asked about every 100 ms and whenever the budget
    /// changes. If it answers `true`, the request leaves the queue, so that the ones behind it
    /// can go on, and `None` is returned. A request that can be served at once is never `None`.
    pub fn acquire(
        &self,
        bytes: u64,
        should_cancel: &dyn Fn() -> bool,
    ) -> Option<DecodeBudgetPermit> {
        let shared = &self.shared;
        let mut state = shared.lock();
        let ticket = state.next_ticket;
        state.next_ticket += 1;
        state.queue.push_back(ticket);

        loop {
            if state.queue.front() == Some(&ticket) && shared.fits(state.used, bytes) {
                state.queue.pop_front();
                state.used = state.used.saturating_add(bytes);
                // The next request is first in line now and may fit in what is left.
                shared.changed.notify_all();
                return Some(DecodeBudgetPermit {
                    shared: Arc::clone(shared),
                    bytes,
                });
            }
            if should_cancel() {
                state.queue.retain(|&queued| queued != ticket);
                shared.changed.notify_all();
                return None;
            }
            state = match shared.changed.wait_timeout(state, CANCEL_POLL_INTERVAL) {
                Ok((guard, _)) => guard,
                Err(poisoned) => poisoned.into_inner().0,
            };
        }
    }

    #[cfg(test)]
    fn used(&self) -> u64 {
        self.shared.lock().used
    }

    #[cfg(test)]
    fn waiting(&self) -> usize {
        self.shared.lock().queue.len()
    }
}

/// Bytes lent by a [`DecodeBudget`], given back when this is dropped.
pub struct DecodeBudgetPermit {
    shared: Arc<Shared>,
    bytes: u64,
}

impl Drop for DecodeBudgetPermit {
    fn drop(&mut self) {
        let mut state = self.shared.lock();
        state.used = state.used.saturating_sub(self.bytes);
        self.shared.changed.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::time::Instant;

    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    /// Long enough to see that a request is still waiting; a wait that should end does so
    /// within [`ENDS_WITHIN`] on any machine the tests run on.
    const STILL_WAITING: Duration = Duration::from_millis(250);
    const ENDS_WITHIN: Duration = Duration::from_secs(10);

    fn never() -> bool {
        false
    }

    /// Asks for `bytes` on another thread; a message arrives on the channel once it is served.
    fn acquire_in_thread(
        budget: &Arc<DecodeBudget>,
        bytes: u64,
    ) -> (
        mpsc::Receiver<DecodeBudgetPermit>,
        std::thread::JoinHandle<()>,
    ) {
        let (sender, receiver) = mpsc::channel();
        let budget = Arc::clone(budget);
        let handle = std::thread::spawn(move || {
            if let Some(permit) = budget.acquire(bytes, &never) {
                let _ = sender.send(permit);
            }
        });
        (receiver, handle)
    }

    /// Waits until `count` requests are queued, so that the next one is known to be behind them.
    fn wait_until_queued(budget: &DecodeBudget, count: usize) {
        let deadline = Instant::now() + ENDS_WITHIN;
        while budget.waiting() < count {
            assert!(Instant::now() < deadline, "the request never queued");
            std::thread::yield_now();
        }
    }

    #[test]
    fn budget_is_the_minimum_when_installed_memory_is_unknown() {
        assert_eq!(budget_for(None), DECODE_MEMORY_BUDGET_MIN);
    }

    #[test]
    fn budget_is_the_minimum_below_three_times_it() {
        assert_eq!(budget_for(Some(0)), DECODE_MEMORY_BUDGET_MIN);
        assert_eq!(budget_for(Some(4 * GIB)), DECODE_MEMORY_BUDGET_MIN);
        assert_eq!(
            budget_for(Some(DECODE_MEMORY_BUDGET_MIN * 3 - 1)),
            DECODE_MEMORY_BUDGET_MIN
        );
    }

    #[test]
    fn budget_is_a_third_of_installed_memory_in_between() {
        assert_eq!(budget_for(Some(24 * GIB)), 8 * GIB);
        assert_eq!(
            budget_for(Some(DECODE_MEMORY_BUDGET_MIN * 3)),
            DECODE_MEMORY_BUDGET_MIN
        );
        assert_eq!(budget_for(Some(32 * GIB)), 32 * GIB / 3);
    }

    #[test]
    fn budget_is_capped_for_a_lot_of_memory() {
        assert_eq!(budget_for(Some(48 * GIB)), DECODE_MEMORY_BUDGET_MAX);
        assert_eq!(budget_for(Some(64 * GIB)), DECODE_MEMORY_BUDGET_MAX);
        assert_eq!(budget_for(Some(u64::MAX)), DECODE_MEMORY_BUDGET_MAX);
    }

    #[test]
    fn requests_that_fit_together_are_served_together() {
        let budget = DecodeBudget::new(100);
        let first = budget.acquire(40, &never).expect("fits");
        let second = budget.acquire(60, &never).expect("fits in what is left");
        assert_eq!(budget.used(), 100);
        drop((first, second));
    }

    #[test]
    fn a_request_over_what_is_left_waits_for_a_permit_to_be_dropped() {
        let budget = Arc::new(DecodeBudget::new(100));
        let held = budget.acquire(60, &never).expect("fits");
        let (served, handle) = acquire_in_thread(&budget, 60);

        assert!(served.recv_timeout(STILL_WAITING).is_err());
        drop(held);
        let permit = served.recv_timeout(ENDS_WITHIN).expect("served after drop");
        assert_eq!(budget.used(), 60);
        drop(permit);
        handle.join().unwrap();
        assert_eq!(budget.used(), 0);
    }

    #[test]
    fn dropping_a_permit_gives_the_bytes_back() {
        let budget = DecodeBudget::new(100);
        let permit = budget.acquire(100, &never).expect("fits");
        assert_eq!(budget.used(), 100);
        drop(permit);
        assert_eq!(budget.used(), 0);
        assert!(budget.acquire(100, &never).is_some());
    }

    #[test]
    fn a_request_over_the_whole_budget_is_served_alone_once_the_rest_is_back() {
        let budget = Arc::new(DecodeBudget::new(100));
        let held = budget.acquire(10, &never).expect("fits");
        let (served, handle) = acquire_in_thread(&budget, 500);

        assert!(served.recv_timeout(STILL_WAITING).is_err());
        drop(held);
        let oversize = served
            .recv_timeout(ENDS_WITHIN)
            .expect("served once nothing else is lent");
        handle.join().unwrap();
        assert_eq!(budget.used(), 500);

        // Nothing else is served while it is held, however small.
        let deadline = Instant::now() + STILL_WAITING;
        let small = budget.acquire(1, &|| Instant::now() >= deadline);
        assert!(small.is_none());

        drop(oversize);
        assert!(budget.acquire(1, &never).is_some());
    }

    #[test]
    fn a_request_over_the_whole_budget_is_served_at_once_when_nothing_is_lent() {
        let budget = DecodeBudget::new(100);
        assert!(budget.acquire(500, &never).is_some());
    }

    #[test]
    fn requests_are_served_in_the_order_they_were_made() {
        let budget = Arc::new(DecodeBudget::new(100));
        let held = budget.acquire(60, &never).expect("fits");

        // 80 does not fit in the 40 that is left, so it waits at the head of the queue.
        let (big_served, big) = acquire_in_thread(&budget, 80);
        wait_until_queued(&budget, 1);
        // 20 would fit in the 40 that is left, but it was asked for later.
        let (small_served, small) = acquire_in_thread(&budget, 20);
        wait_until_queued(&budget, 2);

        assert!(small_served.recv_timeout(STILL_WAITING).is_err());
        assert!(big_served.try_recv().is_err());

        drop(held);
        let big_permit = big_served.recv_timeout(ENDS_WITHIN).expect("head served");
        let small_permit = small_served
            .recv_timeout(ENDS_WITHIN)
            .expect("served behind it");
        big.join().unwrap();
        small.join().unwrap();
        drop((big_permit, small_permit));
    }

    #[test]
    fn a_cancelled_request_leaves_the_queue_and_the_ones_behind_it_go_on() {
        let budget = Arc::new(DecodeBudget::new(100));
        let _held = budget.acquire(60, &never).expect("fits");

        let cancelled = Arc::new(AtomicBool::new(false));
        let (result_sender, result) = mpsc::channel();
        let head = {
            let budget = Arc::clone(&budget);
            let cancelled = Arc::clone(&cancelled);
            std::thread::spawn(move || {
                let permit = budget.acquire(80, &|| cancelled.load(Ordering::SeqCst));
                let _ = result_sender.send(permit.is_some());
            })
        };
        wait_until_queued(&budget, 1);
        let (behind_served, behind) = acquire_in_thread(&budget, 20);
        wait_until_queued(&budget, 2);
        assert!(behind_served.recv_timeout(STILL_WAITING).is_err());

        let cancelled_at = Instant::now();
        cancelled.store(true, Ordering::SeqCst);
        let was_served = result.recv_timeout(ENDS_WITHIN).expect("cancel is seen");
        assert!(!was_served);
        assert!(
            cancelled_at.elapsed() < Duration::from_millis(500),
            "cancel took {:?}",
            cancelled_at.elapsed()
        );
        let permit = behind_served
            .recv_timeout(ENDS_WITHIN)
            .expect("the request behind goes on");
        head.join().unwrap();
        behind.join().unwrap();
        assert_eq!(budget.waiting(), 0);
        assert_eq!(budget.used(), 80);
        drop(permit);
    }

    #[test]
    fn a_request_that_fits_is_served_even_if_it_is_cancelled() {
        let budget = DecodeBudget::new(100);
        assert!(budget.acquire(10, &|| true).is_some());
    }

    #[test]
    fn a_poisoned_lock_does_not_stop_the_budget() {
        let budget = Arc::new(DecodeBudget::new(100));
        let poisoner = Arc::clone(&budget);
        let _ = std::thread::spawn(move || {
            let _state = poisoner.shared.lock();
            panic!("poison the lock");
        })
        .join();
        assert!(budget.shared.state.is_poisoned());

        let permit = budget.acquire(100, &never).expect("works after poison");
        drop(permit);
        assert_eq!(budget.used(), 0);
    }
}
