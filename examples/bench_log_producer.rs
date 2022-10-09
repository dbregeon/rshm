extern crate rshm;

mod log;
use self::log::LogProducer;
use core::ops::Add;

use std::{
    mem::size_of,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use clap::{self, Parser};
use rshm::shm::ShmDefinition;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Interval between events in microseconds
    #[clap(short, long, value_parser, default_value_t = 100)]
    beat: u64,

    /// Number of warmup events
    #[clap(short, long, value_parser, default_value_t = 1000)]
    warmup_count: usize,

    /// Number of events to produce
    #[clap(short, long, value_parser, default_value_t = 100000)]
    count: usize,
}

/// The production side of the benchmark test.
///
/// It must be started before the consumption side.
///
/// It will produce a set number of records at the specified interval. Warmup records are part
/// of the count used for the benchmark.
fn main() {
    env_logger::init();

    let args = Args::parse();

    run_light_load(
        args.warmup_count,
        args.count,
        std::time::Duration::from_micros(args.beat),
    );
}

fn run_light_load(warmup_count: usize, count: usize, beat: std::time::Duration) {
    let log_definition = ShmDefinition {
        path: "test_log".to_string(),
        size: size_of::<LigthRecord>() * (warmup_count + count),
    };
    let log_shm = log_definition.create().unwrap();
    let mut log: LogProducer<LigthRecord> = LogProducer::new(log_shm);

    std::thread::sleep(std::time::Duration::from_secs(30));

    // Warmup
    for i in 0..warmup_count {
        wait(beat);
        log.insert(build_light_record(i)).unwrap();
    }

    std::thread::sleep(std::time::Duration::from_secs(5));

    // Warmup
    for i in warmup_count..count {
        wait(beat);
        log.insert(build_light_record(i)).unwrap();
    }
}

// Thread sleep may not accomodate small durations
fn wait(duration: std::time::Duration) {
    let end = Instant::now().add(duration);
    while Instant::now() < end {
        // burn
    }
}

fn build_light_record(seq_num: usize) -> LigthRecord {
    LigthRecord {
        value: (
            seq_num + 1,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ),
    }
}

#[derive(Clone, Copy)]
pub struct LigthRecord {
    pub value: (usize, u128),
}
