/// P² streaming quantile estimator (Jain & Chlamtac) for p50/p95/p99 without storing all samples.
#[derive(Debug, Clone)]
pub struct StreamingQuantiles {
    p50: P2Estimator,
    p95: P2Estimator,
    p99: P2Estimator,
    count: u64,
    sum: f64,
}

impl Default for StreamingQuantiles {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamingQuantiles {
    pub fn new() -> Self {
        Self {
            p50: P2Estimator::new(0.50),
            p95: P2Estimator::new(0.95),
            p99: P2Estimator::new(0.99),
            count: 0,
            sum: 0.0,
        }
    }

    pub fn observe(&mut self, value: f64) {
        if !value.is_finite() || value < 0.0 {
            return;
        }
        self.count += 1;
        self.sum += value;
        self.p50.observe(value);
        self.p95.observe(value);
        self.p99.observe(value);
    }

    pub fn avg_ms(&self) -> Option<f64> {
        if self.count == 0 {
            return None;
        }
        Some((self.sum / self.count as f64) * 1000.0)
    }

    pub fn p50_ms(&self) -> Option<f64> {
        self.p50.quantile().map(|s| s * 1000.0)
    }

    pub fn p95_ms(&self) -> Option<f64> {
        self.p95.quantile().map(|s| s * 1000.0)
    }

    pub fn p99_ms(&self) -> Option<f64> {
        self.p99.quantile().map(|s| s * 1000.0)
    }
}

#[derive(Debug, Clone)]
struct P2Estimator {
    p: f64,
    initialized: bool,
    count: usize,
    initial: [f64; 5],
    q: [f64; 5],
    n: [f64; 5],
    np: [f64; 5],
    dn: [f64; 5],
}

impl P2Estimator {
    fn new(p: f64) -> Self {
        Self {
            p,
            initialized: false,
            count: 0,
            initial: [0.0; 5],
            q: [0.0; 5],
            n: [0.0; 5],
            np: [0.0; 5],
            dn: [0.0; 5],
        }
    }

    fn observe(&mut self, x: f64) {
        if !self.initialized {
            if self.count < 5 {
                self.initial[self.count] = x;
                self.count += 1;
                if self.count == 5 {
                    self.initial.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    for i in 0..5 {
                        self.q[i] = self.initial[i];
                        self.n[i] = (i + 1) as f64;
                    }
                    self.n[0] = 1.0;
                    self.n[4] = 5.0;
                    self.np[0] = 1.0;
                    self.np[1] = 1.0 + 2.0 * self.p;
                    self.np[2] = 1.0 + 4.0 * self.p;
                    self.np[3] = 3.0 + 2.0 * self.p;
                    self.np[4] = 5.0;
                    self.dn[0] = 0.0;
                    self.dn[1] = self.p / 2.0;
                    self.dn[2] = self.p;
                    self.dn[3] = (1.0 + self.p) / 2.0;
                    self.dn[4] = 1.0;
                    self.initialized = true;
                }
                return;
            }
        }

        let mut k = 0;
        if x < self.q[0] {
            self.q[0] = x;
            k = 0;
        } else if x >= self.q[4] {
            self.q[4] = x;
            k = 3;
        } else {
            for i in 0..4 {
                if x >= self.q[i] && x < self.q[i + 1] {
                    k = i;
                    break;
                }
            }
        }

        for i in (k + 1)..5 {
            self.n[i] += 1.0;
        }

        for i in 0..5 {
            self.np[i] += self.dn[i];
        }

        for i in 1..4 {
            let d = self.np[i] - self.n[i];
            if (d >= 1.0 && self.n[i + 1] - self.n[i] > 1.0)
                || (d <= -1.0 && self.n[i - 1] - self.n[i] < -1.0)
            {
                let di = if d > 0.0 { 1.0 } else { -1.0 };
                let q_new = self.q[i]
                    + di / (self.n[i + 1] - self.n[i - 1])
                        * ((self.n[i] - self.n[i - 1] + di) * (self.q[i + 1] - self.q[i])
                            / (self.n[i + 1] - self.n[i])
                            + (self.n[i + 1] - self.n[i] - di) * (self.q[i] - self.q[i - 1])
                                / (self.n[i] - self.n[i - 1]));
                if self.q[i - 1] < q_new && q_new < self.q[i + 1] {
                    self.q[i] = q_new;
                } else if q_new >= self.q[i + 1] {
                    self.q[i] = self.q[i + 1];
                } else {
                    self.q[i] = self.q[i - 1];
                }
                self.n[i] += di;
            }
        }
    }

    fn quantile(&self) -> Option<f64> {
        if !self.initialized {
            if self.count == 0 {
                return None;
            }
            let mut v: Vec<f64> = self.initial[..self.count].to_vec();
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let idx = ((v.len() as f64 - 1.0) * self.p).round() as usize;
            return Some(v[idx.min(v.len() - 1)]);
        }
        Some(self.q[2])
    }
}
