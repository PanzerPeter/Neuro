// `std::time`: intervals measured on a monotonic clock, which no change to the wall clock
// moves. The clock's zero is arbitrary, so an `Instant` is only ever compared with another.

const NANOS_PER_MILLI: u64 = 1000000
const NANOS_PER_SECOND: f64 = 1000000000.0

// One reading of the clock.
@derive(Copy, Clone)
export struct Instant {
    nanos: u64
}

impl Instant {
    func now() -> Instant {
        Instant { nanos: __monotonic_nanos() }
    }

    // The time since this reading.
    func elapsed(&self) -> Duration {
        Instant::now().duration_since(&self)
    }

    // The time from `earlier` to this reading, zero when `earlier` is the later one.
    func duration_since(&self, earlier: &Instant) -> Duration {
        if self.nanos < earlier.nanos {
            return Duration { nanos: 0 }
        }
        Duration { nanos: self.nanos - earlier.nanos }
    }
}

// A span of time, to the nanosecond.
@derive(Copy, Clone)
export struct Duration {
    nanos: u64
}

impl Duration {
    func as_nanos(&self) -> u64 {
        self.nanos
    }

    func as_millis(&self) -> u64 {
        self.nanos / NANOS_PER_MILLI
    }

    func as_secs_f64(&self) -> f64 {
        self.nanos as f64 / NANOS_PER_SECOND
    }
}
