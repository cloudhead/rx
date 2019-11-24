use std::collections::VecDeque;
use std::time;

pub struct FrameTimer {
    timings: VecDeque<u128>,
    avg: time::Duration,
}

impl FrameTimer {
    const WINDOW: usize = 60;

    pub fn new() -> Self {
        Self {
            timings: VecDeque::with_capacity(Self::WINDOW),
            avg: time::Duration::from_secs(0),
        }
    }

    pub fn run<F, R>(&mut self, frame: F) -> R
    where
        F: FnOnce(time::Duration) -> R,
    {
        let start = time::Instant::now();
        let result = frame(self.avg);
        let elapsed = start.elapsed();

        self.timings.truncate(Self::WINDOW - 1);
        self.timings.push_front(elapsed.as_micros());

        let avg = self.timings.iter().sum::<u128>() / self.timings.len() as u128;
        self.avg = time::Duration::from_micros(avg as u64);

        result
    }
}
