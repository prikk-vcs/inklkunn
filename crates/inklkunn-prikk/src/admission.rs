//! The admission gate (RFC 001 D-4; internal design STO-4; planeter RFC 011's shape): a memory budget
//! split into a **light pool** for floor-cost reads and a **heavy pool** for the rest, so a cheap read never
//! queues behind a large one. A read that does not fit its pool at all is `TooLarge`; one that would fit
//! but finds the pool full waits a bounded time and is then `Busy`. Both are answers, never silence.

use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::cost::ReadVerb;

/// Why a read was not admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The pool was full for the whole bounded wait.
    Busy,
    /// The read costs more than its pool holds.
    TooLarge {
        /// The pool's size, in bytes.
        pool: u64,
    },
}

#[derive(Debug)]
struct Pools {
    light_free: u64,
    heavy_free: u64,
}

/// The gate. Cheap to clone the handle (`Arc` inside).
#[derive(Debug, Clone)]
pub struct Admission {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    light_pool: u64,
    heavy_pool: u64,
    max_wait: Duration,
    state: Mutex<Pools>,
    changed: Condvar,
}

/// A granted admission; the bytes return to their pool on drop.
#[derive(Debug)]
pub struct Permit {
    inner: Arc<Inner>,
    bytes: u64,
    light: bool,
}

impl Drop for Permit {
    fn drop(&mut self) {
        if let Ok(mut p) = self.inner.state.lock() {
            if self.light {
                p.light_free = p
                    .light_free
                    .saturating_add(self.bytes)
                    .min(self.inner.light_pool);
            } else {
                p.heavy_free = p
                    .heavy_free
                    .saturating_add(self.bytes)
                    .min(self.inner.heavy_pool);
            }
        }
        self.inner.changed.notify_all();
    }
}

impl Permit {
    /// The bytes this permit holds.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
}

impl Admission {
    /// A gate over `budget` bytes: a quarter (at least `min_light`, at most the budget) is the light pool,
    /// the rest the heavy pool. `max_wait` bounds how long a read waits for room.
    pub fn new(budget: u64, min_light: u64, max_wait: Duration) -> Self {
        let light_pool = (budget / 4).max(min_light).min(budget);
        let heavy_pool = budget.saturating_sub(light_pool);
        Admission {
            inner: Arc::new(Inner {
                light_pool,
                heavy_pool,
                max_wait,
                state: Mutex::new(Pools {
                    light_free: light_pool,
                    heavy_free: heavy_pool,
                }),
                changed: Condvar::new(),
            }),
        }
    }

    /// The light pool's size.
    pub fn light_pool(&self) -> u64 {
        self.inner.light_pool
    }

    /// The heavy pool's size.
    pub fn heavy_pool(&self) -> u64 {
        self.inner.heavy_pool
    }

    /// Admit a read of `cost` bytes, waiting at most the gate's bound for room.
    pub fn admit(&self, cost: u64) -> Result<Permit, Refusal> {
        let light = ReadVerb::is_light(cost);
        let pool = if light {
            self.inner.light_pool
        } else {
            self.inner.heavy_pool
        };
        if cost > pool {
            return Err(Refusal::TooLarge { pool });
        }
        let deadline = Instant::now() + self.inner.max_wait;
        let Ok(mut state) = self.inner.state.lock() else {
            return Err(Refusal::Busy);
        };
        loop {
            let free = if light {
                state.light_free
            } else {
                state.heavy_free
            };
            if free >= cost {
                if light {
                    state.light_free -= cost;
                } else {
                    state.heavy_free -= cost;
                }
                return Ok(Permit {
                    inner: Arc::clone(&self.inner),
                    bytes: cost,
                    light,
                });
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(Refusal::Busy);
            }
            match self.inner.changed.wait_timeout(state, deadline - now) {
                Ok((s, _)) => state = s,
                Err(_) => return Err(Refusal::Busy),
            }
        }
    }
}

#[cfg(test)]
mod tests;
