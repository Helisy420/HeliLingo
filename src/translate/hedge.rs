//! Hedged requests: try candidates in order, but don't wait for a slow one.
//!
//! Candidate 0 starts at once. Each next candidate starts when every
//! running one has failed, or when its delay since the previous start has
//! passed, whichever comes first. The first success wins; slower requests
//! still running are left to finish on their own threads (their results
//! are dropped), so the caller never waits for them.

use std::sync::Arc;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::{Duration, Instant};

/// Runs `run(i)` for `i in 0..n` hedged as described above. `delay(i)` is
/// how long candidate `i` (i ≥ 1) waits after the previous start while the
/// earlier ones are still running. On failure returns every candidate's
/// error in order (`None` for ones never started).
pub fn race<T, E>(
    n: usize,
    delay: impl Fn(usize) -> Duration,
    run: Arc<dyn Fn(usize) -> Result<T, E> + Send + Sync>,
) -> Result<T, Vec<Option<E>>>
where
    T: Send + 'static,
    E: Send + 'static,
{
    let mut errors: Vec<Option<E>> = (0..n).map(|_| None).collect();
    if n == 0 {
        return Err(errors);
    }
    let (tx, rx) = channel::<(usize, Result<T, E>)>();
    let mut next = 0;
    let mut running = 0;
    let mut next_at = Instant::now();
    loop {
        // Start everything that is due: the next candidate once its delay
        // has passed, or at once when nothing is running any more.
        while next < n && (running == 0 || Instant::now() >= next_at) {
            let (tx, run, i) = (tx.clone(), run.clone(), next);
            std::thread::spawn(move || {
                let _ = tx.send((i, run(i)));
            });
            running += 1;
            next += 1;
            if next < n {
                next_at = Instant::now() + delay(next);
            }
        }
        if running == 0 {
            return Err(errors);
        }
        let wait = if next < n {
            next_at.saturating_duration_since(Instant::now())
        } else {
            Duration::from_secs(3600)
        };
        match rx.recv_timeout(wait) {
            Ok((_, Ok(t))) => return Ok(t),
            Ok((i, Err(e))) => {
                running -= 1;
                errors[i] = Some(e);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Err(errors),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn slow_first_is_overtaken() {
        let t0 = Instant::now();
        let r = race(2, |_| ms(100), Arc::new(|i| {
            if i == 0 {
                std::thread::sleep(ms(2000));
                Ok::<_, ()>("first")
            } else {
                Ok("second")
            }
        }));
        assert_eq!(r, Ok("second"));
        assert!(t0.elapsed() < ms(1000), "waited for the slow one: {:?}", t0.elapsed());
    }

    #[test]
    fn a_failure_starts_the_next_at_once() {
        let t0 = Instant::now();
        let r = race(3, |_| ms(5000), Arc::new(|i| if i < 2 { Err(i) } else { Ok("third") }));
        assert_eq!(r, Ok("third"));
        assert!(t0.elapsed() < ms(1000));
    }

    #[test]
    fn fast_first_wins_and_all_errors_are_kept() {
        assert_eq!(race(2, |_| ms(100), Arc::new(|i| if i == 0 { Ok::<_, ()>(0) } else { Ok(1) })), Ok(0));
        let r: Result<(), _> = race(3, |_| ms(10), Arc::new(Err));
        assert_eq!(r, Err(vec![Some(0), Some(1), Some(2)]));
        let r: Result<(), Vec<Option<()>>> = race(0, |_| ms(10), Arc::new(|_| Ok(())));
        assert_eq!(r, Err(vec![]));
    }
}
