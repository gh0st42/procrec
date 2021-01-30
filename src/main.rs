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
use std::io::{self, Write};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;
use std::{thread, time};
use tempfile::NamedTempFile;
use std::convert::TryFrom;
use std::ops::Deref;

/// Process recorder to log cpu utilization and memory consumption.
#[derive(Clap)]
#[clap(version = crate_version!(), author = crate_authors!())]
struct Opts {
    /// Sampling interval in seconds
    #[clap(short = 'i', long = "interval", default_value = "2")]
    interval: u64,
    /// Duration for observation
    #[clap(short = 'd', long = "duration")]
    duration: Option<u64>,
    /// Process to be inspected. If omitted, a command to execute must be given.
    #[clap(short = 'p', long = "pid", conflicts_with = "command")]
    pid: Option<u32>,
    /// A level of verbosity, and can be used multiple times
    #[clap(short = 'v', long = "verbose", parse(from_occurrences))]
    verbose: i32,

    /// Display graph using gnuplot
    #[clap(short = 'g', long = "graph")]
    graph: bool,
    /// Just print gnuplot script
    #[clap(short = 't', long = "print-gnuplot")]
    script_dump: bool,

    /// The command to execute and record. If omitted, then --pid must be provided.
    #[clap(index = 1, multiple = true, conflicts_with = "pid")]
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

/// Define a struct to carry the information about the process
/// to track. The process can be either external or internal.
///
/// This enum dereferences to the psutil::Process to gather information 
/// about system usage.
pub enum TrackedProcess {
  /// An external process was started outside of this program and
  /// submitted using the --pid parameter.
  External(Process),
  /// An internal process is started by procrec as a fork and requires
  /// joining the forked process.
  Internal(Process, std::process::Child)
}

impl<'a> TryFrom<&'a Opts> for TrackedProcess {
    type Error = String;

    fn try_from(opts: &'a Opts) -> Result<Self, Self::Error> {
     match opts.pid {
       Some(pid) => match Process::new(pid) {
         Ok(p) => Ok(TrackedProcess::External(p)),
         Err(e) => Err(format!("Failed accessing process: {}", e))
       },
       None => {
         let cl = &opts.command;
         if cl.len() == 0 {
           return Err("Process to record must be provided as additional argument or via '--pid' parameter. For detailed information, execute with --help".to_owned())
         }
           
         // Create the command line for the process to be executed
         let mut cmd = Command::new(cl[0].clone());
         if cl.len() > 1 {
           cmd.args(&cl[1..]);
         }
       
         match cmd.spawn() {
           Ok(c) => match Process::new(c.id()) {
               Ok(p) => Ok(TrackedProcess::Internal(p, c)),
               Err(e) => Err(format!("Failed access created process: {}", e))
            },
           Err(e) => {
             return Err(format!("Can not execute command: {}", e));
           }
          }
       }
    }
  }
}

impl TrackedProcess {
  /// Wraps around the internal process.cpu_percent() because
  /// value needs to be mutable.
  pub fn cpu_percent(&mut self) -> psutil::process::ProcessResult<psutil::Percent> {
    match self {
      TrackedProcess::External(p) => p.cpu_percent(),
      TrackedProcess::Internal(p, _) => p.cpu_percent()
    }
  }

  /// Check if the tracked process is still running
  pub fn is_running(&mut self) -> bool {
    match self {
      // For an internal process, check if we can join the child-process
			// Unless the child-process is joined, it will be reported as "running"
      TrackedProcess::Internal(_, ref mut c) => match c.try_wait() {
        Err(e) => panic!("Can not check if child process can be joined: {}", e),
        Ok(Some(_exit_status)) => false, // exit status is irrelevant for the tracking
        Ok(None) => true
      },
      // For external process, rely on psutils to check process status
      TrackedProcess::External(p) => p.is_running()
    }
  }
}

impl Deref for TrackedProcess {
    type Target = Process;

    fn deref(&self) -> &Self::Target {
        match self {
          TrackedProcess::Internal(p, _) => &p,
          TrackedProcess::External(p) => &p
        }
    }
}

// Implement a custom handler to clean up the child-process of an internal process
impl Drop for TrackedProcess {
	fn drop(&mut self) {
		// If we have forked a child process, we need to kill and clean up
		if self.is_running() {
    	if let TrackedProcess::Internal(_, ref mut c) = self {
        if let Err(e) = c.kill() {
					eprintln!("Warning: can not kill child process: {}", e);
				} else if let Err(e) = c.wait() {
        	eprintln!("Warning: Can not join the child process after killing it: {}", e);
				}
			}
    }
	}
}


fn delay(millis: u64) {
    let timeout = time::Duration::from_millis(millis);
    thread::sleep(timeout);
}
fn gnuplot_recording(recording: &[Sample]) -> io::Result<()> {
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

    // Initialize the tracking process
    let mut pid_proc = match TrackedProcess::try_from(&opts) {
			Err(e) => { 
				eprintln!("Error: {}", e);
				std::process::exit(1);
			}, 
			Ok(p) => p
		};

    // Fetch the CPU one time set the "baseline"
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

      if ! pid_proc.is_running() {
          running.store(false, Ordering::SeqCst);
        } else {
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
