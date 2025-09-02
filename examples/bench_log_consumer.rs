extern crate rshm;

mod log;

use self::log::LogConsumer;

use std::{
    mem::size_of,
    num::NonZero,
    time::{SystemTime, UNIX_EPOCH},
};

use clap::{self, Parser};
use rshm::shm::ShmDefinition;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Number of warmup events
    #[clap(short, long, value_parser, default_value_t = 1000)]
    warmup_count: usize,

    /// Number of events to produce
    #[clap(short, long, value_parser, default_value_t = 100000)]
    count: usize,
}

///
/// Reads a given number of records from the shared memory log.
///
/// Warmup records are part of the count.
///
/// It will log information about the record consumption"
fn main() {
    env_logger::init();

    let args = Args::parse();

    test_light_load(args.warmup_count, args.count);
}

fn test_light_load(warmup_count: usize, count: usize) {
    let definition = ShmDefinition {
        path: "test_log".to_string(),
        size: NonZero::new(size_of::<LigthRecord>() * (warmup_count + count)).unwrap(),
    };
    let log_shm = definition.open().unwrap();
    let mut log: LogConsumer<LigthRecord> = LogConsumer::new(log_shm);

    let mut sequence = 0;

    // Warmup
    while sequence < warmup_count {
        match log.next() {
            Some(t) => {
                sequence = t.value.0;
            }
            None => {}
        }
    }

    let mut result = Vec::with_capacity(count);
    while sequence < count {
        match log.next() {
            Some(t) => {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos();

                result.push((t.value.0, now, t.value.1));
                sequence = t.value.0;
            }
            None => {}
        }
    }

    let mut previous: Option<(usize, u128, u128)> = None;
    println!("SeqNum\t(Received-Sent nanos)\tReceived nanos\tSent nanos\t(Received - Previous Received nanos)\t(Sent - Previous Sent nanos)");
    for r in result {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            r.0,
            r.1 - r.2,
            r.1,
            r.2,
            previous.map(|p| r.1 - p.1).unwrap_or(0),
            previous.map(|p| r.2 - p.2).unwrap_or(0)
        );
        previous = Some(r);
    }
}

#[derive(Clone, Copy)]
pub struct LigthRecord {
    pub value: (usize, u128),
}
