//! Fixed-window tick durations and the phase split `/tps` prints.
//!
//! The sample buffers are allocated with the server. A tick only writes into
//! them. A phase that did not run stays at zero.

use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

/// Recent ticks kept for MSPT percentiles. Same length as `Server::tick_times_nanos`.
pub const TICK_SAMPLE_WINDOW: usize = 100;

const TICK_PHASE_COUNT: usize = 6;

/// One slice of a server tick. The order is the order `/tps` prints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TickPhase {
    Entities,
    BlockEntities,
    Chunks,
    /// Light is updated on the block change that caused it, not as its own
    /// tick step, so this stays zero until a tick records it.
    Lighting,
    NetworkFlush,
    Plugins,
}

impl TickPhase {
    pub const ALL: [Self; TICK_PHASE_COUNT] = [
        Self::Entities,
        Self::BlockEntities,
        Self::Chunks,
        Self::Lighting,
        Self::NetworkFlush,
        Self::Plugins,
    ];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Entities => "entities",
            Self::BlockEntities => "block entities",
            Self::Chunks => "chunks",
            Self::Lighting => "lighting",
            Self::NetworkFlush => "network flush",
            Self::Plugins => "plugins",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Entities => 0,
            Self::BlockEntities => 1,
            Self::Chunks => 2,
            Self::Lighting => 3,
            Self::NetworkFlush => 4,
            Self::Plugins => 5,
        }
    }
}

/// Nanoseconds spent in each phase of one tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PhaseDurations {
    nanos: [i64; TICK_PHASE_COUNT],
}

impl PhaseDurations {
    #[must_use]
    pub const fn nanos(self, phase: TickPhase) -> i64 {
        self.nanos[phase.index()]
    }
}

/// p50, p95, and p99 of a tick-duration window, in nanoseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MsptPercentiles {
    pub p50_nanos: i64,
    pub p95_nanos: i64,
    pub p99_nanos: i64,
    pub samples: usize,
}

/// Nearest-rank percentile. `sorted_ascending` must be sorted ascending.
///
/// The index is `len * percentile / 100`, the same rank `/tick query` uses.
#[must_use]
pub const fn percentile_nanos(sorted_ascending: &[i64], percentile: usize) -> i64 {
    if sorted_ascending.is_empty() {
        return 0;
    }
    let index = sorted_ascending.len().saturating_mul(percentile) / 100;
    let last = sorted_ascending.len() - 1;
    let index = if index > last { last } else { index };
    sorted_ascending[index]
}

/// Sorts `samples_nanos` and returns p50, p95, and p99.
///
/// An empty window is all zeros. This allocates; call it from commands, not the tick.
#[must_use]
pub fn mspt_percentiles_from_samples(samples_nanos: &[i64]) -> MsptPercentiles {
    if samples_nanos.is_empty() {
        return MsptPercentiles {
            p50_nanos: 0,
            p95_nanos: 0,
            p99_nanos: 0,
            samples: 0,
        };
    }

    let mut sorted = Vec::with_capacity(samples_nanos.len());
    sorted.extend_from_slice(samples_nanos);
    sorted.sort_unstable();
    MsptPercentiles {
        p50_nanos: percentile_nanos(&sorted, 50),
        p95_nanos: percentile_nanos(&sorted, 95),
        p99_nanos: percentile_nanos(&sorted, 99),
        samples: sorted.len(),
    }
}

/// Formats a tick duration as milliseconds with two decimal places, without a unit suffix.
#[must_use]
pub fn format_tick_millis(nanos: i64) -> String {
    format!("{:.2}", nanos as f64 / 1_000_000.0)
}

#[must_use]
pub fn duration_to_nanos(duration: Duration) -> i64 {
    i64::try_from(duration.as_nanos()).unwrap_or(i64::MAX)
}

/// Slot written by `update_tick_times` for a completed tick count.
///
/// The store uses the pre-increment count, so the latest sample sits at
/// `completed_ticks - 1` inside the window.
#[must_use]
pub const fn phase_ring_index(completed_ticks: i32) -> Option<usize> {
    if completed_ticks <= 0 {
        None
    } else {
        Some((completed_ticks as usize - 1) % TICK_SAMPLE_WINDOW)
    }
}

/// Phase samples for the fixed window. Allocated once.
pub struct TickTimings {
    /// In-progress tick. Worlds tick in parallel, so the slots are atomic.
    current: [AtomicI64; TICK_PHASE_COUNT],
    phases: Mutex<[PhaseDurations; TICK_SAMPLE_WINDOW]>,
}

impl TickTimings {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            current: [const { AtomicI64::new(0) }; TICK_PHASE_COUNT],
            phases: Mutex::new(
                [const {
                    PhaseDurations {
                        nanos: [0; TICK_PHASE_COUNT],
                    }
                }; TICK_SAMPLE_WINDOW],
            ),
        }
    }

    /// Clears the in-progress sample. The tick loop calls this once per tick.
    pub fn begin_tick(&self) {
        for slot in &self.current {
            slot.store(0, Ordering::Relaxed);
        }
    }

    /// Adds time for a phase that runs on the ticker thread.
    ///
    /// A non-positive duration is ignored, which leaves a phase that did not
    /// run at zero.
    pub fn add_phase(&self, phase: TickPhase, nanos: i64) {
        if nanos <= 0 {
            return;
        }
        self.current[phase.index()].fetch_add(nanos, Ordering::Relaxed);
    }

    /// Keeps the greater duration. Dimensions tick together, so the slowest
    /// one is the phase's wait, not the sum of their CPU time.
    pub fn record_max_phase(&self, phase: TickPhase, nanos: i64) {
        if nanos <= 0 {
            return;
        }
        let slot = &self.current[phase.index()];
        let mut current = slot.load(Ordering::Relaxed);
        while current < nanos {
            match slot.compare_exchange_weak(current, nanos, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(observed) => current = observed,
            }
        }
    }

    /// Copies the in-progress sample into the ring and clears it.
    pub fn finish_tick(&self, index: usize) {
        let mut sample = PhaseDurations::default();
        for phase in TickPhase::ALL {
            sample.nanos[phase.index()] = self.current[phase.index()].swap(0, Ordering::Relaxed);
        }
        let mut phases = self
            .phases
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        phases[index % TICK_SAMPLE_WINDOW] = sample;
    }

    #[must_use]
    pub fn phase_at(&self, index: usize) -> PhaseDurations {
        let phases = self
            .phases
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        phases[index % TICK_SAMPLE_WINDOW]
    }
}

impl Default for TickTimings {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MsptPercentiles, TICK_SAMPLE_WINDOW, TickPhase, TickTimings, duration_to_nanos,
        mspt_percentiles_from_samples, percentile_nanos, phase_ring_index,
    };
    use std::time::{Duration, Instant};

    #[test]
    fn percentile_ranks_match_a_fixed_sample() {
        // 1ms, 2ms, ... 100ms, passed in reverse so the sort is part of the check.
        // Index is `len * percentile / 100`: 50 -> 51ms, 95 -> 96ms, 99 -> 100ms.
        let mut samples = [0i64; 100];
        for (index, sample) in samples.iter_mut().enumerate() {
            *sample = (100 - index as i64) * 1_000_000;
        }

        let percentiles = mspt_percentiles_from_samples(&samples);
        assert_eq!(
            percentiles,
            MsptPercentiles {
                p50_nanos: 51_000_000,
                p95_nanos: 96_000_000,
                p99_nanos: 100_000_000,
                samples: 100,
            }
        );
        assert_eq!(percentile_nanos(&[], 95), 0);
    }

    #[test]
    fn empty_and_single_sample_percentiles() {
        assert_eq!(
            mspt_percentiles_from_samples(&[]),
            MsptPercentiles {
                p50_nanos: 0,
                p95_nanos: 0,
                p99_nanos: 0,
                samples: 0,
            }
        );

        let percentiles = mspt_percentiles_from_samples(&[42]);
        assert_eq!(percentiles.p50_nanos, 42);
        assert_eq!(percentiles.p95_nanos, 42);
        assert_eq!(percentiles.p99_nanos, 42);
        assert_eq!(percentiles.samples, 1);
    }

    #[test]
    fn phase_that_did_not_run_is_zero() {
        let timings = TickTimings::new();
        timings.begin_tick();
        timings.record_max_phase(TickPhase::Entities, 2_000_000);
        timings.record_max_phase(TickPhase::Chunks, 500_000);
        timings.finish_tick(0);

        let phases = timings.phase_at(0);
        assert_eq!(phases.nanos(TickPhase::Entities), 2_000_000);
        assert_eq!(phases.nanos(TickPhase::Chunks), 500_000);
        assert_eq!(phases.nanos(TickPhase::BlockEntities), 0);
        assert_eq!(phases.nanos(TickPhase::Lighting), 0);
        assert_eq!(phases.nanos(TickPhase::NetworkFlush), 0);
        assert_eq!(phases.nanos(TickPhase::Plugins), 0);
    }

    #[test]
    fn parallel_worlds_keep_the_slowest_phase_and_plugins_add() {
        let timings = TickTimings::new();
        timings.begin_tick();
        timings.record_max_phase(TickPhase::Entities, 5);
        timings.record_max_phase(TickPhase::Entities, 9);
        timings.record_max_phase(TickPhase::Entities, 4);
        timings.add_phase(TickPhase::Plugins, 3);
        timings.add_phase(TickPhase::Plugins, 4);
        timings.finish_tick(0);

        let phases = timings.phase_at(0);
        assert_eq!(phases.nanos(TickPhase::Entities), 9);
        assert_eq!(phases.nanos(TickPhase::Plugins), 7);
        assert_eq!(phases.nanos(TickPhase::Lighting), 0);
    }

    #[test]
    fn one_allocation_covers_a_full_window_and_the_next_ticks() {
        let timings = TickTimings::new();
        let ticks = TICK_SAMPLE_WINDOW + 7;
        for tick in 0..ticks {
            timings.begin_tick();
            timings.add_phase(TickPhase::Entities, tick as i64);
            timings.finish_tick(tick % TICK_SAMPLE_WINDOW);
        }

        let last = ticks - 1;
        assert_eq!(
            timings
                .phase_at(last % TICK_SAMPLE_WINDOW)
                .nanos(TickPhase::Entities),
            last as i64
        );
        // The slot just past the last write still holds the previous lap.
        let older = last % TICK_SAMPLE_WINDOW + 1;
        assert_eq!(
            timings.phase_at(older).nanos(TickPhase::Entities),
            older as i64
        );
        assert_eq!(
            timings
                .phase_at(last % TICK_SAMPLE_WINDOW)
                .nanos(TickPhase::Lighting),
            0
        );
    }

    #[test]
    fn latest_phase_index_matches_the_slot_just_written() {
        assert_eq!(phase_ring_index(0), None);
        assert_eq!(phase_ring_index(-1), None);
        for completed in 1..250 {
            let stored_at = (completed - 1) % TICK_SAMPLE_WINDOW;
            assert_eq!(phase_ring_index(completed as i32), Some(stored_at));
        }
    }

    /// Idle cost of the hooks a tick actually calls: clear, three dimensions
    /// publishing phases the world tick already measured, one plugin clock
    /// pair, and the ring write. Lighting and network flush are not recorded.
    #[test]
    fn idle_instrumentation_stays_well_under_a_tick() {
        const ITERATIONS: u32 = 20_000;
        let timings = TickTimings::new();
        for _ in 0..1_000 {
            timings.begin_tick();
            timings.finish_tick(0);
        }

        let started = Instant::now();
        for tick in 0..ITERATIONS {
            timings.begin_tick();
            for _world in 0..3 {
                timings.record_max_phase(TickPhase::Chunks, 20_000);
                timings.record_max_phase(TickPhase::Entities, 15_000);
                timings.record_max_phase(TickPhase::BlockEntities, 5_000);
            }
            let plugin_started = Instant::now();
            std::hint::black_box(plugin_started);
            timings.add_phase(
                TickPhase::Plugins,
                duration_to_nanos(plugin_started.elapsed()),
            );
            timings.finish_tick((tick as usize) % TICK_SAMPLE_WINDOW);
        }
        let per_iter_ns = started.elapsed().as_nanos() / u128::from(ITERATIONS);
        let phases = timings.phase_at(((ITERATIONS - 1) as usize) % TICK_SAMPLE_WINDOW);
        assert_eq!(phases.nanos(TickPhase::Entities), 15_000);
        assert_eq!(phases.nanos(TickPhase::Lighting), 0);
        assert_eq!(phases.nanos(TickPhase::NetworkFlush), 0);
        assert!(
            per_iter_ns < 50_000,
            "instrumentation averaged {per_iter_ns} ns per tick"
        );
    }

    #[test]
    fn duration_to_nanos_saturates_instead_of_wrapping() {
        assert_eq!(duration_to_nanos(Duration::from_nanos(25)), 25);
        assert_eq!(duration_to_nanos(Duration::ZERO), 0);
        assert_eq!(duration_to_nanos(Duration::from_nanos(u64::MAX)), i64::MAX);
    }
}
