//! Parsing on several threads, writing on one (issue #635).
//!
//! A full reindex spends much of its time in tree-sitter parses, which
//! touch no database. The walk hands each file that needs a parse to a
//! [`Pipeline`]: worker threads parse while this thread, the only SQLite
//! writer, keeps walking and writes each result as soon as every earlier
//! one is written. The job and result channels are bounded, and the walk
//! stops queueing while too many results wait for an earlier one, so the
//! files held at once stay a small multiple of the worker count. A parse
//! that panics is caught on its thread and re-raised on this one.
//!
//! Writing in job order keeps a parallel run's rows identical to a
//! sequential one: the same files, in the same order, get the same ids.

use std::any::Any;
use std::collections::BTreeMap;
use std::panic::AssertUnwindSafe;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::Mutex;

/// Most parse threads a reindex starts by default.
pub(crate) const MAX_WORKERS: usize = 8;

/// The environment variable that sets the worker count (1 parses on the
/// writing thread, as before #635).
pub(crate) const WORKERS_ENV: &str = "MCT_INDEX_WORKERS";

/// Jobs queued per worker before the walk waits for a result.
const QUEUED_PER_WORKER: usize = 2;

/// How many threads parse: [`WORKERS_ENV`] when set, else the machine's
/// parallelism up to [`MAX_WORKERS`].
pub(crate) fn workers() -> usize {
    std::env::var(WORKERS_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .map(|n| n.clamp(1, 64))
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map_or(1, |n| n.get())
                .min(MAX_WORKERS)
        })
}

type Panic = Box<dyn Any + Send>;

/// The writer's ends: jobs out, results (or a worker's panic) back.
type Channels<J, P> = (SyncSender<(usize, J)>, Receiver<(usize, Result<P, Panic>)>);

/// Hands jobs to the parse threads and gives back their results in job
/// order. Built by [`pipeline`].
pub(crate) struct Pipeline<'a, J, P> {
    parse: &'a (dyn Fn(J) -> P + Sync),
    /// `None` with one worker: jobs are parsed on this thread.
    channels: Option<Channels<J, P>>,
    /// Most results held back waiting for an earlier one.
    capacity: usize,
    waiting: BTreeMap<usize, P>,
    sent: usize,
    written: usize,
    /// A worker's panic: nothing more is queued or written.
    panic: Option<Panic>,
}

impl<J, P> Pipeline<'_, J, P> {
    /// Queues `job` and writes every result already in order, waiting for
    /// a result while the queue is full.
    pub(crate) fn push<E>(
        &mut self,
        job: J,
        write: &mut impl FnMut(P) -> Result<(), E>,
    ) -> Result<(), E> {
        let Some((jobs, _)) = &self.channels else {
            return write((self.parse)(job));
        };
        let jobs = jobs.clone();
        // A slow parse holds every later result back: wait for it.
        while self.waiting.len() >= self.capacity {
            if !self.receive(true) {
                return Ok(());
            }
            self.flush(write)?;
        }
        let mut job = (self.sent, job);
        loop {
            match jobs.try_send(job) {
                Ok(()) => break,
                Err(TrySendError::Full(back)) => {
                    job = back;
                    if !self.receive(true) {
                        return Ok(());
                    }
                    self.flush(write)?;
                }
                Err(TrySendError::Disconnected(_)) => return Ok(()),
            }
        }
        self.sent += 1;
        while self.receive(false) {}
        self.flush(write)
    }

    /// Writes every remaining result, in order.
    pub(crate) fn finish<E>(
        &mut self,
        write: &mut impl FnMut(P) -> Result<(), E>,
    ) -> Result<(), E> {
        while self.written + self.waiting.len() < self.sent && self.receive(true) {
            self.flush(write)?;
        }
        self.flush(write)
    }

    /// Takes one result into `waiting`, blocking when `block`. False when
    /// none came: nothing ready yet, every worker gone, or a parse
    /// panicked (kept for [`pipeline`] to re-raise).
    fn receive(&mut self, block: bool) -> bool {
        if self.panic.is_some() {
            return false;
        }
        let Some((_, results)) = &self.channels else {
            return false;
        };
        let next = if block {
            results.recv().ok()
        } else {
            results.try_recv().ok()
        };
        match next {
            Some((position, Ok(parsed))) => {
                self.waiting.insert(position, parsed);
                true
            }
            Some((_, Err(panic))) => {
                self.panic = Some(panic);
                false
            }
            None => false,
        }
    }

    fn flush<E>(&mut self, write: &mut impl FnMut(P) -> Result<(), E>) -> Result<(), E> {
        while let Some(parsed) = self.waiting.remove(&self.written) {
            self.written += 1;
            write(parsed)?;
        }
        Ok(())
    }
}

/// Runs `body` with a [`Pipeline`] whose jobs `parse` turns into results
/// on up to `workers` threads. A panicking `parse` panics here once `body`
/// has returned, with nothing written after the job that panicked.
pub(crate) fn pipeline<J, P, R>(
    workers: usize,
    parse: impl Fn(J) -> P + Sync,
    body: impl FnOnce(&mut Pipeline<'_, J, P>) -> R,
) -> R
where
    J: Send,
    P: Send,
{
    if workers <= 1 {
        return body(&mut Pipeline {
            parse: &parse,
            channels: None,
            capacity: 0,
            waiting: BTreeMap::new(),
            sent: 0,
            written: 0,
            panic: None,
        });
    }
    let capacity = workers * QUEUED_PER_WORKER;
    let (job_sender, jobs) = mpsc::sync_channel::<(usize, J)>(capacity);
    let (result_sender, results) = mpsc::sync_channel::<(usize, Result<P, Panic>)>(capacity);
    let jobs = Mutex::new(jobs);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let sender = result_sender.clone();
            let (jobs, parse) = (&jobs, &parse);
            scope.spawn(move || loop {
                let next = jobs.lock().ok().and_then(|jobs| jobs.recv().ok());
                let Some((position, job)) = next else { break };
                let parsed = std::panic::catch_unwind(AssertUnwindSafe(|| parse(job)));
                // The receiver is gone only when the run stopped: stop too.
                if sender.send((position, parsed)).is_err() {
                    break;
                }
            });
        }
        drop(result_sender);
        let mut pipeline = Pipeline {
            parse: &parse,
            channels: Some((job_sender, results)),
            capacity,
            waiting: BTreeMap::new(),
            sent: 0,
            written: 0,
            panic: None,
        };
        let outcome = body(&mut pipeline);
        let panic = pipeline.panic.take();
        // Closing the channels ends the workers' loops.
        drop(pipeline);
        if let Some(panic) = panic {
            std::panic::resume_unwind(panic);
        }
        outcome
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn run(jobs: u64, workers: usize) -> Vec<u64> {
        let mut written = Vec::new();
        let mut write = |n: u64| {
            written.push(n);
            Ok::<(), ()>(())
        };
        pipeline(
            workers,
            |n: u64| {
                // Later jobs finish first.
                std::thread::sleep(std::time::Duration::from_micros(jobs - n));
                n * 2
            },
            |pipeline| {
                for n in 0..jobs {
                    pipeline.push(n, &mut write)?;
                }
                pipeline.finish(&mut write)
            },
        )
        .unwrap();
        written
    }

    #[test]
    fn results_are_written_in_job_order_whatever_finishes_first() {
        let expected: Vec<u64> = (0..300).map(|n| n * 2).collect();
        assert_eq!(run(300, 4), expected);
        assert_eq!(run(300, 1), expected);
    }

    #[test]
    fn a_write_error_stops_the_run() {
        let mut written = 0;
        let mut write = |n: u32| {
            written += 1;
            if n == 10 {
                Err("disk full")
            } else {
                Ok(())
            }
        };
        let result = pipeline(
            3,
            |n: u32| n,
            |pipeline| {
                for n in 0..100 {
                    pipeline.push(n, &mut write)?;
                }
                pipeline.finish(&mut write)
            },
        );
        assert_eq!(result, Err("disk full"));
        assert_eq!(written, 11);
    }

    #[test]
    fn a_panicking_parse_panics_the_caller_without_hanging() {
        let mut written = Vec::new();
        let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut write = |n: u32| {
                written.push(n);
                Ok::<(), ()>(())
            };
            pipeline(
                4,
                |n: u32| {
                    assert_ne!(n, 7, "parser bug");
                    n
                },
                |pipeline| {
                    for n in 0..200 {
                        pipeline.push(n, &mut write)?;
                    }
                    pipeline.finish(&mut write)
                },
            )
        }));
        assert!(outcome.is_err());
        assert_eq!(written, (0..7).collect::<Vec<_>>());
    }

    #[test]
    fn a_slow_first_job_bounds_the_results_held_back() {
        let held = std::sync::atomic::AtomicUsize::new(0);
        let most = std::sync::atomic::AtomicUsize::new(0);
        let mut write = |_: u32| {
            held.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            Ok::<(), ()>(())
        };
        pipeline(
            2,
            |n: u32| {
                if n == 0 {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                let now = held.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                most.fetch_max(now, std::sync::atomic::Ordering::SeqCst);
                n
            },
            |pipeline| {
                for n in 0..500 {
                    pipeline.push(n, &mut write)?;
                }
                pipeline.finish(&mut write)
            },
        )
        .unwrap();
        // Parsed but unwritten: at most the waiting buffer plus both
        // channels and one per worker.
        let bound = 3 * 2 * QUEUED_PER_WORKER + 2;
        assert!(most.into_inner() <= bound, "held more than {bound}");
    }

    #[test]
    fn one_worker_parses_on_this_thread() {
        let here = std::thread::current().id();
        let mut write = |_: u32| Ok::<(), ()>(());
        pipeline(
            1,
            |n: u32| {
                assert_eq!(std::thread::current().id(), here);
                n
            },
            |pipeline| pipeline.push(1, &mut write),
        )
        .unwrap();
    }
}
