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
use psutil::process::Process;
use std::fmt;
use std::io::Result;
use std::io::{self, Write};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use std::{thread, time};
use tempfile::NamedTempFile;

/// Process recorder to log cpu utilization and memory consumption.
#[derive(Clap)]
#[clap(version = crate_version!(), author = crate_authors!())]
struct Opts {
    /// Sampling interval in seconds
    #[clap(short = "i", long = "interval", default_value = "2")]
    interval: u64,
    /// Duration for observation
    #[clap(short = "d", long = "duration")]
    duration: Option<u64>,
    /// Process to be inspected. If omitted, a command to execute must be given.
    #[clap(short = "p", long = "pid", required_unless_one = &["command"])]
    pid: Option<u32>,
    /// A level of verbosity, and can be used multiple times
    #[clap(short = "v", long = "verbose", parse(from_occurrences))]
    verbose: i32,

    /// Display graph using gnuplot
    #[clap(short = "g", long = "graph")]
    graph: bool,
    /// Just print gnuplot script
    #[clap(short = "t", long = "print-gnuplot")]
    script_dump: bool,

    /// The command to execute and record. If omitted, then --pid must be provided.
    #[clap(index = 1, multiple = true, conflicts_with = "pid", required = false, required_unless_one = &["pid"])]
    command: Vec<String>,
}

#[derive(Debug)]
struct Sample {
    ts: f32,
    pid: u32,
    //num_threads: u64, // currently not supported in psutil crate
    cpu: f32,
    vsize: u64,
    rss: u64,
}

impl fmt::Display for Sample {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{:.02} PID {} CPU% {:.02} RSS {} VSIZE {} ",
            self.ts, self.pid, self.cpu, self.rss, self.vsize
        )
    }
}

fn delay(millis: u64) {
    let timeout = time::Duration::from_millis(millis);
    thread::sleep(timeout);
}
fn gnuplot_recording(recording: &[Sample]) -> Result<()> {
    let gnuplot_script_content = include_str!("../recording.plot");
    let mut gnuplot_file = NamedTempFile::new()?;
    gnuplot_file.write_all(gnuplot_script_content.as_bytes())?;

    let mut data_file = NamedTempFile::new()?;
    for i in recording {
        data_file.write_all(format!("{}\n", i).as_bytes())?;
    }
    data_file.flush()?;
    let fname_param = format!("filename={:?};", data_file.path().display());

    let output = Command::new("gnuplot")
        .arg("-e")
        .arg(fname_param)
        .arg("-p")
        .arg(gnuplot_file.path())
        .output()?;

    if !output.status.success() {
        println!("status: {}", output.status);
        io::stdout().write_all(&output.stdout)?;
        io::stderr().write_all(&output.stderr)?;
    }
    Ok(())
}

fn main() {
    let opts: Opts = Opts::parse();

    if opts.script_dump {
        let gnuplot_script_content = include_str!("../recording.plot");
        println!("{}", gnuplot_script_content);
        std::process::exit(0);
    }
    let mut child : Option<std::process::Child> = None;

    // SETUP phase
    let mut pid_proc = match opts.pid {
      Some(pid) => Process::new(pid).expect("Failed accessing process"),
      None => {
        let mut cl = opts.command;
        let mut cmd = Command::new(cl.remove(0));
        cmd.args(cl);

        let pid;
        match cmd.spawn() {
          Ok(c) => {
            pid = c.id();
            child = Some(c);
          },
          Err(e) => {
            eprintln!("Can not execute command: {}", e);
            std::process::exit(1);
          }
        }
        Process::new(pid).expect("Failed accessing process")
      }
    };
    let _percent_cpu = pid_proc.cpu_percent();
    let sample_rate = opts.interval * 1000;

    let mut recording = vec![];
    let mut start: Option<SystemTime> = None;

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");
    // MAIN phase
    while running.load(Ordering::SeqCst) {
        delay(sample_rate);
        match child.as_mut() {
          Some(c) => match c.try_wait() {
            Ok(Some(_)) => { 
              running.store(false, Ordering::SeqCst);
            },
            Ok(None) => { },
            Err(e) => panic!("Can not check status of child process: {}", e),
          },
          _ => {}
        }

        if pid_proc.is_running() {
          let percent_cpu = pid_proc.cpu_percent().unwrap();
          let cur_mem = pid_proc.memory_info().unwrap();
          let time_since_start = if let Some(time) = start {
              time.elapsed().unwrap().as_secs_f32()
          } else {
              start = Some(time::SystemTime::now());
              0.0
          };
          let data = Sample {
              ts: time_since_start,
              pid: pid_proc.pid(),
              cpu: percent_cpu,
              rss: cur_mem.rss() / 1000,
              vsize: cur_mem.vms() / 1000,
              //num_threads: pid_proc.num_threads(),
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
        }
    }

    match child {
      Some(mut c) => { c.wait().expect("Can not kill child process"); },
      None => {}
    };

    // POST phase
    if opts.verbose == 0 {
        for i in &recording {
            println!("{}", i);
        }
    }
    if opts.graph {
        if let Err(err) = gnuplot_recording(&recording) {
            println!("Fatal error calling gnuplot: {}", err);
        }
    }
}
