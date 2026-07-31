/// Roughly 5.7 seconds of history at 90 frames per second.
const CAPACITY: usize = 512;

/// Frame-time statistics behind the overlay.
pub struct Perf {
    times: [f32; CAPACITY],
    sorted: Vec<f32>,
    head: usize,
    count: usize,
    since_recompute: f32,

    pub last: f32,
    pub median: f32,
    pub mean: f32,
    pub p95: f32,
    pub p99: f32,
    pub max: f32,
    pub frames_per_second: f32,
    pub frames_per_second_low: f32,

    pub draw_calls: u32,
    pub triangles: u32,
    pub gpu_milliseconds: f32,

    /// Per-system costs in milliseconds, in registration order.
    pub system_milliseconds: Vec<(&'static str, f32)>,

    pub spike_count: u32,
    pub frames_since_reset: u32,
}

impl Default for Perf {
    fn default() -> Self {
        Self {
            times: [0.0; CAPACITY],
            sorted: vec![0.0; CAPACITY],
            head: 0,
            count: 0,
            since_recompute: 0.0,
            last: 0.0,
            median: 0.0,
            mean: 0.0,
            p95: 0.0,
            p99: 0.0,
            max: 0.0,
            frames_per_second: 0.0,
            frames_per_second_low: 0.0,
            draw_calls: 0,
            triangles: 0,
            gpu_milliseconds: 0.0,
            system_milliseconds: Vec::new(),
            spike_count: 0,
            frames_since_reset: 0,
        }
    }
}

pub fn sample(perf: &mut Perf, milliseconds: f32) {
    perf.times[perf.head] = milliseconds;
    perf.head = (perf.head + 1) % CAPACITY;
    if perf.count < CAPACITY {
        perf.count += 1;
    }
    perf.last = milliseconds;

    perf.since_recompute += milliseconds;
    if perf.since_recompute >= 250.0 {
        perf.since_recompute = 0.0;
        recompute(perf);
    }

    perf.frames_since_reset += 1;
    if perf.median > 0.0 && milliseconds > perf.median + 4.0 {
        perf.spike_count += 1;
    }
}

fn recompute(perf: &mut Perf) {
    if perf.count == 0 {
        return;
    }

    let mut sum = 0.0;
    let mut max: f32 = 0.0;
    perf.sorted.clear();
    for value in &perf.times[..perf.count] {
        perf.sorted.push(*value);
        sum += *value;
        max = max.max(*value);
    }
    perf.sorted.sort_by(f32::total_cmp);

    let last_index = perf.count - 1;
    perf.mean = sum / perf.count as f32;
    perf.max = max;
    perf.median = perf.sorted[perf.count / 2];
    perf.p95 = perf.sorted[last_index.min(perf.count * 95 / 100)];
    perf.p99 = perf.sorted[last_index.min(perf.count * 99 / 100)];
    perf.frames_per_second = if perf.median > 0.0 {
        1000.0 / perf.median
    } else {
        0.0
    };
    perf.frames_per_second_low = if perf.p99 > 0.0 {
        1000.0 / perf.p99
    } else {
        0.0
    };
}

/// Records one system's cost for this frame, overwriting the previous value.
pub fn mark(perf: &mut Perf, name: &'static str, milliseconds: f32) {
    if let Some(entry) = perf
        .system_milliseconds
        .iter_mut()
        .find(|(entry_name, _)| *entry_name == name)
    {
        entry.1 = milliseconds;
        return;
    }
    perf.system_milliseconds.push((name, milliseconds));
}

pub fn reset_spikes(perf: &mut Perf) {
    perf.spike_count = 0;
    perf.frames_since_reset = 0;
}

/// Frame times oldest to newest, for the graph.
pub fn history(perf: &Perf) -> impl Iterator<Item = f32> + '_ {
    (0..perf.count).map(move |offset| {
        let index = (perf.head + CAPACITY - perf.count + offset) % CAPACITY;
        perf.times[index]
    })
}
