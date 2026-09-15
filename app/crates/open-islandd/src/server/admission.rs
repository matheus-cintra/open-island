use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[derive(Default)]
pub struct Budget(Arc<Counter>);
#[derive(Default)]
struct Counter {
    active: AtomicUsize,
    high_water: AtomicUsize,
    rejected: AtomicUsize,
}
pub struct Permit(Arc<Counter>);
impl Budget {
    pub fn acquire(&self, limit: usize) -> Option<Permit> {
        let previous = match self
            .0
            .active
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < limit).then_some(n + 1)
            }) {
            Ok(previous) => previous,
            Err(_) => {
                let _ = self
                    .0
                    .rejected
                    .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                        Some(n.saturating_add(1))
                    });
                return None;
            }
        };
        self.0.high_water.fetch_max(previous + 1, Ordering::AcqRel);
        Some(Permit(self.0.clone()))
    }
    pub fn snapshot(&self, limit: u64) -> open_island_core::diagnostics::Capacity {
        open_island_core::diagnostics::Capacity {
            active: self.0.active.load(Ordering::Acquire) as u64,
            high_water: self.0.high_water.load(Ordering::Acquire) as u64,
            rejected: self.0.rejected.load(Ordering::Acquire) as u64,
            limit,
        }
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::AcqRel);
    }
}
#[derive(Default)]
pub struct Admission {
    pub outbox: Arc<super::outbox_metrics::Metrics>,
    pub bulk: Budget,
    pub connections: Budget,
    pub managed: Budget,
    pub fast: Budget,
    pub blocking: Budget,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn permits_return_after_panic_and_saturated_blocking_does_not_starve_fast() {
        let admission = Admission::default();
        let all: Vec<_> = (0..64)
            .map(|_| admission.blocking.acquire(64).unwrap())
            .collect();
        assert!(admission.blocking.acquire(64).is_none());
        assert!(admission.fast.acquire(64).is_some());
        let budget = Budget::default();
        let _ = std::panic::catch_unwind(|| {
            let _permit = budget.acquire(1).unwrap();
            panic!("release");
        });
        assert!(budget.acquire(1).is_some());
        drop(all);
        assert_eq!(admission.blocking.0.active.load(Ordering::Acquire), 0);
        let snapshot = admission.blocking.snapshot(64);
        assert_eq!(snapshot.active, 0);
        assert_eq!(snapshot.high_water, 64);
        assert_eq!(snapshot.rejected, 1);
    }
}
