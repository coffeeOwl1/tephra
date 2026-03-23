use std::collections::VecDeque;

/// Fixed-capacity ring buffer for time-series data.
/// When full, the oldest sample is dropped on push.
#[derive(Debug, Clone)]
pub struct RingBuffer<T> {
    data: VecDeque<T>,
    capacity: usize,
}

#[allow(dead_code)]
impl<T: Clone> RingBuffer<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            data: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, value: T) {
        if self.data.len() == self.capacity {
            self.data.pop_front();
        }
        self.data.push_back(value);
    }

    pub fn extend(&mut self, values: impl IntoIterator<Item = T>) {
        for v in values {
            self.push(v);
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn back(&self) -> Option<&T> {
        self.data.back()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.data.iter()
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }
}

/// Trend direction based on recent history.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Trend {
    Rising,
    Falling,
    Stable,
}

impl Trend {
    pub fn arrow(&self) -> &'static str {
        match self {
            Trend::Rising => "↑",
            Trend::Falling => "↓",
            Trend::Stable => "→",
        }
    }

    pub fn color(&self) -> iced::Color {
        match self {
            Trend::Rising => crate::theme::colors::MAGMA,
            Trend::Falling => crate::theme::colors::GEOTHERMAL,
            Trend::Stable => crate::theme::colors::TEPHRA,
        }
    }
}

/// Compute standard deviation of the last N samples in a ring buffer.
pub fn compute_sigma(buf: &RingBuffer<f64>, window: usize) -> Option<f64> {
    let samples: Vec<f64> = buf.iter().copied().collect();
    let n = samples.len();
    if n < 10 {
        return None; // Not enough data for meaningful σ
    }
    let start = n.saturating_sub(window);
    let window_data = &samples[start..];
    let count = window_data.len() as f64;
    let mean = window_data.iter().sum::<f64>() / count;
    let variance = window_data.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / count;
    Some(variance.sqrt())
}

/// Compute trend with default window (5 samples / 2.5s, ±3%).
pub fn compute_trend(buf: &RingBuffer<f64>) -> Trend {
    compute_trend_with(buf, 5, 0.03)
}

/// Compute trend with a longer window for noisy signals.
pub fn compute_trend_smooth(buf: &RingBuffer<f64>) -> Trend {
    compute_trend_with(buf, 20, 0.05)
}

/// Compute trend: compare current value to the average of the last N samples.
fn compute_trend_with(buf: &RingBuffer<f64>, window: usize, threshold: f64) -> Trend {
    if buf.len() < window {
        return Trend::Stable;
    }
    let recent: Vec<f64> = buf.iter().copied().collect();
    let n = recent.len();
    let win = &recent[n.saturating_sub(window)..n];
    let avg: f64 = win.iter().sum::<f64>() / win.len() as f64;
    let current = recent[n - 1];

    if avg <= 0.0 {
        return Trend::Stable;
    }

    if current > avg * (1.0 + threshold) {
        Trend::Rising
    } else if current < avg * (1.0 - threshold) {
        Trend::Falling
    } else {
        Trend::Stable
    }
}

/// Collection of ring buffers for all time-series metrics from a node.
#[derive(Debug, Clone)]
pub struct TimeSeriesStore {
    pub temp_c: RingBuffer<f64>,
    pub ppt_watts: RingBuffer<f64>,
    pub avg_freq_mhz: RingBuffer<f64>,
    pub avg_util_pct: RingBuffer<f64>,
    pub fan_rpm: RingBuffer<f64>,
}

impl TimeSeriesStore {
    /// Default: 240 samples = 2 minutes at 500ms interval.
    pub const DEFAULT_CAPACITY: usize = 240;

    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::with_capacity(Self::DEFAULT_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            temp_c: RingBuffer::new(capacity),
            ppt_watts: RingBuffer::new(capacity),
            avg_freq_mhz: RingBuffer::new(capacity),
            avg_util_pct: RingBuffer::new(capacity),
            fan_rpm: RingBuffer::new(capacity),
        }
    }

    /// Backfill from the /history endpoint response.
    pub fn backfill(&mut self, hist: &crate::net::api_types::HistoryResponse) {
        self.temp_c.clear();
        self.ppt_watts.clear();
        self.avg_freq_mhz.clear();
        self.avg_util_pct.clear();
        self.fan_rpm.clear();

        self.temp_c
            .extend(hist.temp_c.iter().map(|&v| v as f64));
        self.ppt_watts.extend(hist.ppt_watts.iter().copied());
        self.avg_freq_mhz
            .extend(hist.avg_freq_mhz.iter().map(|&v| v as f64));
        self.avg_util_pct.extend(hist.avg_util_pct.iter().copied());
        self.fan_rpm
            .extend(hist.fan_rpm.iter().map(|&v| v as f64));
    }

    /// Push a single snapshot's values into all ring buffers.
    pub fn push_snapshot(&mut self, snap: &crate::net::api_types::Snapshot) {
        self.temp_c.push(snap.temp_c as f64);
        self.ppt_watts.push(snap.ppt_watts);
        self.avg_freq_mhz.push(snap.avg_freq_mhz as f64);
        self.avg_util_pct.push(snap.avg_util_pct);
        self.fan_rpm.push(snap.fan_rpm as f64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_push_and_iterate() {
        let mut buf = RingBuffer::new(4);
        buf.push(1.0);
        buf.push(2.0);
        buf.push(3.0);
        let vals: Vec<f64> = buf.iter().copied().collect();
        assert_eq!(vals, vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn ring_buffer_wraps_at_capacity() {
        let mut buf = RingBuffer::new(3);
        buf.push(1.0);
        buf.push(2.0);
        buf.push(3.0);
        buf.push(4.0); // evicts 1.0
        let vals: Vec<f64> = buf.iter().copied().collect();
        assert_eq!(vals, vec![2.0, 3.0, 4.0]);
        assert_eq!(buf.len(), 3);
    }

    #[test]
    fn ring_buffer_extend() {
        let mut buf = RingBuffer::new(3);
        buf.extend(vec![1.0, 2.0, 3.0, 4.0, 5.0]);
        let vals: Vec<f64> = buf.iter().copied().collect();
        assert_eq!(vals, vec![3.0, 4.0, 5.0]);
    }

    #[test]
    fn ring_buffer_empty() {
        let buf: RingBuffer<f64> = RingBuffer::new(10);
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);
        assert!(buf.back().is_none());
    }

    #[test]
    fn ring_buffer_back() {
        let mut buf = RingBuffer::new(5);
        buf.push(10.0);
        buf.push(20.0);
        assert_eq!(buf.back(), Some(&20.0));
    }

    #[test]
    fn time_series_store_backfill() {
        let mut store = TimeSeriesStore::new();

        let hist = crate::net::api_types::HistoryResponse {
            interval_ms: 500,
            samples: 3,
            temp_c: vec![50, 51, 52],
            avg_freq_mhz: vec![4500, 4490, 4495],
            ppt_watts: vec![10.0, 11.0, 10.5],
            avg_util_pct: vec![1.0, 2.0, 1.5],
            fan_rpm: vec![0, 0, 0],
        };

        store.backfill(&hist);
        assert_eq!(store.temp_c.len(), 3);
        assert_eq!(store.ppt_watts.back(), Some(&10.5));
    }

    #[test]
    fn compute_trend_stable_on_insufficient_data() {
        let mut buf = RingBuffer::new(10);
        // 0 samples
        assert_eq!(compute_trend(&buf), Trend::Stable);
        // 4 samples (< 5 window)
        for v in [70.0, 71.0, 72.0, 73.0] {
            buf.push(v);
        }
        assert_eq!(compute_trend(&buf), Trend::Stable);
        // 5th sample — now enough data
        buf.push(80.0); // rising
        assert_eq!(compute_trend(&buf), Trend::Rising);
    }
}
