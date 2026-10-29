mod df;
mod proc_diskstats;
mod proc_frame;
mod proc_loadavg;
mod proc_meminfo;
mod proc_net_dev;
mod proc_stat;
mod ps;
mod ss;
mod systemctl;

use crate::metrics::{
    CHART_HISTORY_LEN, MetricPoint, OverviewServerMetrics, ServerMetricsSnapshot, SPARKLINE_LEN,
};
use crate::ssh::{run_command, SshSession};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

pub use proc_frame::spawn_proc_stream;
use std::time::{SystemTime, UNIX_EPOCH};

pub use systemctl::DEFAULT_SERVICE_CANDIDATES;

#[derive(Default)]
struct Ring {
    values: VecDeque<f64>,
    capacity: usize,
}

impl Ring {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            values: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    fn push(&mut self, v: f64) {
        if self.values.len() >= self.capacity {
            self.values.pop_front();
        }
        self.values.push_back(v);
    }

    fn as_vec(&self) -> Vec<f64> {
        self.values.iter().copied().collect()
    }
}

struct MetricHistory {
    points: VecDeque<MetricPoint>,
    capacity: usize,
}

impl MetricHistory {
    fn new(capacity: usize) -> Self {
        Self {
            points: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    fn push(&mut self, t: u64, v: f64) {
        if self.points.len() >= self.capacity {
            self.points.pop_front();
        }
        self.points.push_back(MetricPoint { t, v });
    }

    fn as_vec(&self) -> Vec<MetricPoint> {
        self.points.iter().cloned().collect()
    }
}

pub struct SystemCollectorState {
    prev_cpu: Option<(u64, u64)>,
    prev_net: Option<(u64, u64)>,
    prev_disk: Option<(u64, u64)>,
    cpu_spark: Ring,
    mem_spark: Ring,
    disk_spark: Ring,
    load_spark: Ring,
    cpu_hist: MetricHistory,
    mem_hist: MetricHistory,
    load_hist: MetricHistory,
    disk_usage_hist: MetricHistory,
    disk_read_hist: MetricHistory,
    disk_write_hist: MetricHistory,
    net_in_hist: MetricHistory,
    net_out_hist: MetricHistory,
    last_cpu_percent: f64,
    last_mem_percent: f64,
    last_disk_percent: f64,
    last_load: (f64, f64, f64),
    last_disk_read_bps: f64,
    last_disk_write_bps: f64,
    last_net_in_bps: f64,
    last_net_out_bps: f64,
    processes: Vec<crate::metrics::ProcessRow>,
    process_tree: Vec<crate::metrics::ProcessTreeRow>,
    cpu_by_pid: HashMap<u32, VecDeque<f64>>,
    last_ingest: Option<Instant>,
    services: Vec<crate::metrics::ServiceRow>,
    ss_summary: crate::metrics::SsSummary,
    detected_services: Vec<String>,
}

impl SystemCollectorState {
    pub fn new() -> Self {
        Self {
            prev_cpu: None,
            prev_net: None,
            prev_disk: None,
            cpu_spark: Ring::with_capacity(SPARKLINE_LEN),
            mem_spark: Ring::with_capacity(SPARKLINE_LEN),
            disk_spark: Ring::with_capacity(SPARKLINE_LEN),
            load_spark: Ring::with_capacity(SPARKLINE_LEN),
            cpu_hist: MetricHistory::new(CHART_HISTORY_LEN),
            mem_hist: MetricHistory::new(CHART_HISTORY_LEN),
            load_hist: MetricHistory::new(CHART_HISTORY_LEN),
            disk_usage_hist: MetricHistory::new(CHART_HISTORY_LEN),
            disk_read_hist: MetricHistory::new(CHART_HISTORY_LEN),
            disk_write_hist: MetricHistory::new(CHART_HISTORY_LEN),
            net_in_hist: MetricHistory::new(CHART_HISTORY_LEN),
            net_out_hist: MetricHistory::new(CHART_HISTORY_LEN),
            last_cpu_percent: 0.0,
            last_mem_percent: 0.0,
            last_disk_percent: 0.0,
            last_load: (0.0, 0.0, 0.0),
            last_disk_read_bps: 0.0,
            last_disk_write_bps: 0.0,
            last_net_in_bps: 0.0,
            last_net_out_bps: 0.0,
            processes: Vec::new(),
            process_tree: Vec::new(),
            cpu_by_pid: HashMap::new(),
            last_ingest: None,
            services: Vec::new(),
            ss_summary: crate::metrics::SsSummary {
                tcp_total: None,
                tcp_established: None,
            },
            detected_services: DEFAULT_SERVICE_CANDIDATES
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    pub async fn poll_fast(&mut self, session: &SshSession) -> Result<(), String> {
        let stat = run_command(session, "cat /proc/stat").await.map_err(|e| e.to_string())?;
        let meminfo =
            run_command(session, "cat /proc/meminfo").await.map_err(|e| e.to_string())?;
        let loadavg =
            run_command(session, "cat /proc/loadavg").await.map_err(|e| e.to_string())?;
        let net_dev =
            run_command(session, "cat /proc/net/dev").await.map_err(|e| e.to_string())?;
        let diskstats = run_command(session, "cat /proc/diskstats")
            .await
            .map_err(|e| e.to_string())?;
        let df = run_command(session, "df -P").await.map_err(|e| e.to_string())?;

        self.apply_sample(&stat, &meminfo, &loadavg, &net_dev, &diskstats, Some(&df), 2.0);
        Ok(())
    }

    pub fn ingest_frame(&mut self, frame: &proc_frame::ProcFrame) {
        let interval = self
            .last_ingest
            .map(|t| t.elapsed().as_secs_f64().clamp(0.2, 5.0))
            .unwrap_or(1.0);
        self.apply_sample(
            &frame.stat,
            &frame.meminfo,
            &frame.loadavg,
            &frame.net_dev,
            &frame.diskstats,
            None,
            interval,
        );
    }

    fn apply_sample(
        &mut self,
        stat: &str,
        meminfo: &str,
        loadavg: &str,
        net_dev: &str,
        diskstats: &str,
        df: Option<&str>,
        interval_secs: f64,
    ) {
        self.last_ingest = Some(Instant::now());
        let t = Self::now_ms();

        if let Some(curr) = proc_stat::parse_cpu_jiffies(stat) {
            if let Some(prev) = self.prev_cpu {
                self.last_cpu_percent = proc_stat::cpu_usage_percent(prev, curr);
            }
            self.prev_cpu = Some(curr);
        }

        if let Some(mem) = proc_meminfo::memory_used_percent(meminfo) {
            self.last_mem_percent = mem;
        }

        if let Some((l1, l5, l15)) = proc_loadavg::parse_loadavg(loadavg) {
            self.last_load = (l1, l5, l15);
        }

        if let Some(df) = df {
            if let Some(pct) = df::parse_root_disk_percent(df) {
                self.last_disk_percent = pct;
            }
        }

        if let Some(curr_net) = proc_net_dev::parse_net_bytes(net_dev) {
            if let Some(prev) = self.prev_net {
                let (in_bps, out_bps) = proc_net_dev::net_rates_bps(prev, curr_net, interval_secs);
                self.last_net_in_bps = in_bps;
                self.last_net_out_bps = out_bps;
                self.net_in_hist.push(t, in_bps);
                self.net_out_hist.push(t, out_bps);
            }
            self.prev_net = Some(curr_net);
        }

        if let Some(curr_disk) = proc_diskstats::parse_disk_sectors(diskstats) {
            if let Some(prev) = self.prev_disk {
                let (read_bps, write_bps) =
                    proc_diskstats::disk_rates_bps(prev, curr_disk, interval_secs);
                self.last_disk_read_bps = read_bps;
                self.last_disk_write_bps = write_bps;
                self.disk_read_hist.push(t, read_bps);
                self.disk_write_hist.push(t, write_bps);
            }
            self.prev_disk = Some(curr_disk);
        }

        self.cpu_spark.push(self.last_cpu_percent);
        self.mem_spark.push(self.last_mem_percent);
        self.disk_spark.push(self.last_disk_percent);
        self.load_spark.push(self.last_load.0);

        self.cpu_hist.push(t, self.last_cpu_percent);
        self.mem_hist.push(t, self.last_mem_percent);
        self.load_hist.push(t, self.last_load.0);
        self.disk_usage_hist.push(t, self.last_disk_percent);
    }

    pub async fn poll_processes(&mut self, session: &SshSession) -> Result<(), String> {
        let out = run_command(
            session,
            "ps -eo pid,user,pcpu,pmem,comm --sort=-pcpu 2>/dev/null | head -n 16",
        )
        .await
        .map_err(|e| e.to_string())?;
        self.processes = ps::parse_ps_top(&out, 15);
        Ok(())
    }

    pub async fn poll_process_tree(
        &mut self,
        session: &SshSession,
        apps: &[crate::apps::TracelynApp],
    ) -> Result<(), String> {
        let out = run_command(session, ps::PS_TREE_CMD)
            .await
            .map_err(|e| e.to_string())?;
        let mut rows = ps::parse_ps_tree(&out, Self::now_ms());
        ps::attach_services(&mut rows, apps);
        let mut live = HashSet::new();
        for row in &mut rows {
            live.insert(row.pid);
            let samples = self.cpu_by_pid.entry(row.pid).or_default();
            if samples.len() >= 30 {
                samples.pop_front();
            }
            samples.push_back(row.cpu_percent);
            row.cpu_sparkline = samples.iter().copied().collect();
        }
        self.cpu_by_pid.retain(|pid, _| live.contains(pid));
        self.process_tree = rows;
        Ok(())
    }

    pub fn process_tree(&self) -> Vec<crate::metrics::ProcessTreeRow> {
        self.process_tree.clone()
    }

    pub async fn poll_ss(&mut self, session: &SshSession) -> Result<(), String> {
        let out = run_command(session, "ss -s 2>/dev/null")
            .await
            .map_err(|e| e.to_string())?;
        self.ss_summary = ss::parse_ss_summary(&out);
        Ok(())
    }

    pub async fn poll_services(&mut self, session: &SshSession) -> Result<(), String> {
        let names: Vec<&str> = self
            .detected_services
            .iter()
            .map(|s| s.as_str())
            .collect();
        let cmd = systemctl::systemctl_is_active_command(&names);
        let out = run_command(session, &cmd).await.map_err(|e| e.to_string())?;
        self.services = systemctl::parse_is_active(&out, &names);
        Ok(())
    }

    pub fn overview(&self, server_id: &str) -> OverviewServerMetrics {
        OverviewServerMetrics {
            server_id: server_id.to_string(),
            cpu_percent: self.last_cpu_percent,
            memory_percent: self.last_mem_percent,
            disk_percent: self.last_disk_percent,
            load_1: self.last_load.0,
            cpu_sparkline: self.cpu_spark.as_vec(),
            memory_sparkline: self.mem_spark.as_vec(),
            disk_sparkline: self.disk_spark.as_vec(),
            load_sparkline: self.load_spark.as_vec(),
        }
    }

    pub fn snapshot(
        &self,
        server_id: &str,
        collector_error: Option<String>,
        collector_name: Option<String>,
        collector_failures: u32,
    ) -> ServerMetricsSnapshot {
        let t = Self::now_ms();
        ServerMetricsSnapshot {
            server_id: server_id.to_string(),
            timestamp_ms: t,
            cpu_percent: self.last_cpu_percent,
            memory_percent: self.last_mem_percent,
            load_1: self.last_load.0,
            load_5: self.last_load.1,
            load_15: self.last_load.2,
            disk_percent: self.last_disk_percent,
            disk_read_bps: self.last_disk_read_bps,
            disk_write_bps: self.last_disk_write_bps,
            net_in_bps: self.last_net_in_bps,
            net_out_bps: self.last_net_out_bps,
            cpu_history: self.cpu_hist.as_vec(),
            memory_history: self.mem_hist.as_vec(),
            load_history: self.load_hist.as_vec(),
            disk_usage_history: self.disk_usage_hist.as_vec(),
            disk_read_history: self.disk_read_hist.as_vec(),
            disk_write_history: self.disk_write_hist.as_vec(),
            net_in_history: self.net_in_hist.as_vec(),
            net_out_history: self.net_out_hist.as_vec(),
            processes: self.processes.clone(),
            services: self.services.clone(),
            ss_summary: self.ss_summary.clone(),
            collector_error,
            collector_name,
            collector_failures,
        }
    }
}
