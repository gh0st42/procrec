# procrec

A simple recorder for cpu and memory usage of processes.

Currently, limited to linux but support for macOS might come in the future. Results are outputted in machine parsable format to stdout or can be directly plotted using gnuplot (must be in `$PATH`).

## Installation

Unfortunately, clap 3.0 is not yet published to *crates.io*. Thus, the following does not work (yet):
```
cargo install procrec
```

Therefore, manual installation from github is required:
```
$ git clone https://github.com/gh0st42/procrec
$ cargo install --path .
```

If you want plotting functionality you also need to install gnuplot via your package manager (e.g., `sudo apt install gnuplot`).

## Usage

### Help

``` 
$ procrec -h
procrec 0.1.0
Lars Baumgaertner
Process recorded to log cpu utilization and memory consumption

USAGE:
    procrec [FLAGS] [OPTIONS] --pid <pid>

FLAGS:
    -g, --graph      Display graph using gnuplot
    -h, --help       Prints help information
    -v, --verbose    A level of verbosity, and can be used multiple times
    -V, --version    Prints version information

OPTIONS:
    -d, --duration <duration>    Duration for observation
    -i, --interval <interval>    Sampling interval in seconds [default: 2]
    -p, --pid <pid>              Process to be inspected
```

### Examples

Interactive recording of specific process:
```
$ pgrep chrome
4730
4742
4748
4769
4772

$ procrec -p 4730 -g -i 1 -v
0.00 PID 4730 CPU% 1.00 RSS 283265 VSIZE 3794771968 THREADS 33
1.00 PID 4730 CPU% 3.00 RSS 283263 VSIZE 3794771968 THREADS 33
2.00 PID 4730 CPU% 1.33 RSS 283263 VSIZE 3794771968 THREADS 33
3.01 PID 4730 CPU% 1.00 RSS 283265 VSIZE 3794771968 THREADS 33
4.01 PID 4730 CPU% 0.80 RSS 283265 VSIZE 3794771968 THREADS 33
5.01 PID 4730 CPU% 2.00 RSS 283265 VSIZE 3794771968 THREADS 34
6.01 PID 4730 CPU% 1.00 RSS 283265 VSIZE 3794771968 THREADS 34
7.02 PID 4730 CPU% 2.67 RSS 283288 VSIZE 3794771968 THREADS 34
8.02 PID 4730 CPU% 0.80 RSS 283288 VSIZE 3794771968 THREADS 34
^C9.02 PID 4730 CPU% 1.33 RSS 283288 VSIZE 3794771968 THREADS 34

```

Afterwards a gnuplot window pops up and visualizes the data:

![gnuplot screenshot](/img/gnuplot.png?raw=true "gnuplot screenshot")

## Alternatives

- `pidstat` from the [sysstat package](https://github.com/sysstat/sysstat/) for pure recording, no plotting - written in C and probably available in most distro package managers
- [psrecord](https://github.com/astrofrog/psrecord) a nice python tool for recording and plotting (using `matplotlib`) - installation via `pip` 