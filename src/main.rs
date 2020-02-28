// procrec - record/plot cpu and memory usage of processes
// Copyright (C) 2020 Lars Baumgaertner
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT
// ANY WARRANTY; without even the implied warranty of  MERCHANTABILITY or
// FITNESS FOR A PARTICULAR PURPOSE. See the GNU General Public License for
// more details.
//
// You should have received a copy of the GNU General Public License along with
// this program.  If not, see <http://www.gnu.org/licenses/>.

use clap::{crate_authors, crate_version, Clap};
use ctrlc;
use procfs::process::Process;
use procfs::process::Stat;
use procfs::KernelStats;
use std::convert::TryInto;
use std::fmt;
use std::io::{self, Write};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use std::{thread, time};
use tempfile::NamedTempFile;

/// Process recorded to log cpu utilization and memory consumption.
#[derive(Clap)]
#[clap(version = crate_version!(), author = crate_authors!())]
struct Opts {
    /// Sampling interval in seconds
    #[clap(short = "i", long = "interval", default_value = "2")]
    interval: u64,
    /// Duration for observation
    #[clap(short = "d", long = "duration")]
    duration: Option<u64>,
    /// Process to be inspected
    #[clap(short = "p", long = "pid")]
    pid: i32,
    /// A level of verbosity, and can be used multiple times
    #[clap(short = "v", long = "verbose", parse(from_occurrences))]
    verbose: i32,

    /// Display graph using gnuplot
    #[clap(short = "g", long = "graph")]
    graph: bool,
}

#[derive(Debug)]
struct Sample {
    ts: f32,
    pid: i32,
    num_threads: i64,
    cpu: f32,
    vsize: u64,
    rss: i64,
}

impl fmt::Display for Sample {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:.02} PID {} CPU% {:.02} RSS {} VSIZE {} THREADS {}",
            self.ts, self.pid, self.cpu, self.rss, self.vsize, self.num_threads
        )
    }
}

fn get_total_jiffies() -> f32 {
    let cpu = KernelStats::new().unwrap().total;
    cpu.user
        + cpu.nice
        + cpu.system
        + cpu.idle
        + cpu.iowait.unwrap_or_default()
        + cpu.irq.unwrap_or_default()
        + cpu.steal.unwrap_or_default()
        + cpu.softirq.unwrap_or_default()
}
fn get_stat(pid: i32) -> Stat {
    let target = Process::new(pid).unwrap();
    target.stat().unwrap()
}
fn delay(millis: u64) {
    let timeout = time::Duration::from_millis(millis);
    thread::sleep(timeout);
}
fn main() {
    let opts: Opts = Opts::parse();

    let cores = procfs::CpuInfo::new().unwrap().num_cores() as f32;
    let sample_rate = opts.interval * 1000;
    let mut jiffies1 = get_total_jiffies() / cores;
    let stats = get_stat(opts.pid);
    let mut lasttimes = stats.utime + stats.stime;

    let mut recording = vec![];
    let mut start: Option<SystemTime> = None;

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");
    while running.load(Ordering::SeqCst) {
        delay(sample_rate);
        let jiffies2 = get_total_jiffies() / cores;
        let stats = get_stat(opts.pid);
        let d_j = jiffies2 - jiffies1;
        jiffies1 = jiffies2;
        let d_t: f32 = (stats.utime + stats.stime - lasttimes) as f32;
        let percent_cpu = d_t / d_j as f32;
        let time_since_start = if start.is_none() {
            start = Some(time::SystemTime::now());
            0.0
        } else {
            start.unwrap().elapsed().unwrap().as_secs_f32()
        };

        let data = Sample {
            ts: time_since_start,
            pid: opts.pid,
            cpu: percent_cpu,
            rss: stats.rss,
            vsize: stats.vsize,
            num_threads: stats.num_threads,
        };
        if opts.verbose > 0 {
            println!("{}", data);
        }
        recording.push(data);
        if let Some(dur) = opts.duration {
            if time_since_start > dur as f32 {
                break;
            }
        }
        lasttimes = stats.utime + stats.stime;
    }

    if opts.verbose == 0 {
        for i in &recording {
            println!("{}", i);
        }
    }
    if opts.graph {
        let gnuplot_script_content = include_str!("../recording.plot");
        let mut gnuplot_file = NamedTempFile::new().unwrap();
        gnuplot_file.write_all(gnuplot_script_content.as_bytes());

        let mut data_file = NamedTempFile::new().unwrap();
        for i in &recording {
            data_file.write_all(format!("{}\n", i).as_bytes());
        }
        data_file.flush();
        let fname_param = format!("filename={:?};", data_file.path().display());

        let output = Command::new("gnuplot")
            .arg("-e")
            .arg(fname_param)
            .arg("-p")
            .arg(gnuplot_file.path())
            .output()
            .expect("failed to execute process");

        if !output.status.success() {
            println!("status: {}", output.status);
            io::stdout().write_all(&output.stdout).unwrap();
            io::stderr().write_all(&output.stderr).unwrap();
        }
    }
}
