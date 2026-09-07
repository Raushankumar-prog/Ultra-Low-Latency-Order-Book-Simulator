/// High-Dynamic-Range (HDR) Histogram with sub-bucket linear resolution.
/// Uses 16 linear sub-buckets per power-of-2 octave for high precision (6.25% resolution)
/// across the entire dynamic range from 1 nanosecond to 68 seconds.
/// Stored in a fixed 2.1 KB array [u32; 544], completely cache-resident with zero heap allocations.
#[derive(Debug, Clone)]
pub struct LatencyHistogram {
    buckets: [u32; 544],
    count: u64,
    min_ns: u64,
    max_ns: u64,
    sum_ns: u128,
}

impl Default for LatencyHistogram {
    fn default() -> Self {
        Self::new()
    }
}

impl LatencyHistogram {
    pub fn new() -> Self {
        Self {
            buckets: [0u32; 544],
            count: 0,
            min_ns: u64::MAX,
            max_ns: 0,
            sum_ns: 0,
        }
    }

    #[inline(always)]
    fn value_to_bucket(nanos: u64) -> usize {
        if nanos < 16 {
            return nanos as usize;
        }

        let leading = nanos.leading_zeros() as usize;
        let k = 63 - leading; // Most significant bit position

        if k < 4 {
            nanos as usize
        } else {
            let sub_bucket = ((nanos >> (k - 4)) & 0x0F) as usize;
            let octave = k - 4;
            let idx = 16 + (octave * 16) + sub_bucket;
            idx.min(543)
        }
    }

    #[inline(always)]
    fn bucket_to_value(bucket: usize) -> u64 {
        if bucket < 16 {
            return bucket as u64;
        }

        let octave = (bucket - 16) / 16;
        let sub_bucket = (bucket - 16) % 16;
        let k = octave + 4;

        // Reconstruct midpoint of the sub-bucket interval: [base + sub * step, base + (sub + 1) * step)
        let base = 1u64 << k;
        let step = 1u64 << (k - 4);
        base + (sub_bucket as u64 * step) + (step / 2)
    }

    #[inline(always)]
    pub fn record(&mut self, nanos: u64) {
        self.count += 1;
        self.sum_ns += nanos as u128;
        if nanos < self.min_ns {
            self.min_ns = nanos;
        }
        if nanos > self.max_ns {
            self.max_ns = nanos;
        }

        let bucket = Self::value_to_bucket(nanos);
        self.buckets[bucket] = self.buckets[bucket].saturating_add(1);
    }

    pub fn percentile(&self, p: f64) -> u64 {
        if self.count == 0 {
            return 0;
        }

        let target = ((self.count as f64) * (p / 100.0)).ceil() as u64;
        let mut accumulated = 0u64;

        for bucket in 0..544 {
            accumulated += self.buckets[bucket] as u64;
            if accumulated >= target {
                return Self::bucket_to_value(bucket);
            }
        }

        self.max_ns
    }

    pub fn stats(&self, elapsed_secs: f64) -> LatencyReport {
        if self.count == 0 {
            return LatencyReport::default();
        }

        let throughput = if elapsed_secs > 0.0 {
            self.count as f64 / elapsed_secs
        } else {
            0.0
        };

        LatencyReport {
            total_orders: self.count,
            min_ns: if self.min_ns == u64::MAX { 0 } else { self.min_ns },
            avg_ns: (self.sum_ns / self.count as u128) as u64,
            p50_ns: self.percentile(50.0),
            p95_ns: self.percentile(95.0),
            p99_ns: self.percentile(99.0),
            max_ns: self.max_ns,
            throughput_ops: throughput,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LatencyReport {
    pub total_orders: u64,
    pub min_ns: u64,
    pub avg_ns: u64,
    pub p50_ns: u64,
    pub p95_ns: u64,
    pub p99_ns: u64,
    pub max_ns: u64,
    pub throughput_ops: f64,
}

impl LatencyReport {
    pub fn print_summary(&self, label: &str) {
        println!("--------------------------------------------------");
        println!("HDR Latency Profile [{}]:", label);
        println!("Total Orders:   {}", self.total_orders);
        println!("Throughput:     {:.1} ops/sec", self.throughput_ops);
        println!("Min:            {} ns", self.min_ns);
        println!("Avg:            {} ns", self.avg_ns);
        println!("p50:            {} ns (~{:.2} µs)", self.p50_ns, self.p50_ns as f64 / 1000.0);
        println!("p95:            {} ns (~{:.2} µs)", self.p95_ns, self.p95_ns as f64 / 1000.0);
        println!("p99:            {} ns (~{:.2} µs)", self.p99_ns, self.p99_ns as f64 / 1000.0);
        println!("Max:            {} ns (~{:.2} µs)", self.max_ns, self.max_ns as f64 / 1000.0);
        println!("--------------------------------------------------");
    }
}
