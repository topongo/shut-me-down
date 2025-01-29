use std::process::exit;
use chrono::Duration;

use chrono::{Local, NaiveTime};
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
struct Command {
    #[command(subcommand)]
    mode: Mode,
    #[cfg(feature = "notify")]
    #[arg(short, long)]
    title: Option<String>,
    #[arg(trailing_var_arg = true)]
    reference: Vec<String>,
}

#[derive(Debug, Subcommand, Clone)]
enum Mode {
    At {
        reference: String,
    },
    In {
        #[arg(trailing_var_arg = true)]
        trailing: Vec<String>,
    }
}

fn format_timedelta(delta: Duration) -> String {
    let mut seconds = delta.num_seconds();
    let mut minutes = seconds / 60;
    seconds %= 60;
    let mut hours = minutes / 60;
    minutes %= 60;
    let days = hours / 24;
    hours %= 24;

    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{}d", days));
    }
    if hours > 0 {
        parts.push(format!("{}h", hours));
    }
    if minutes > 0 {
        parts.push(format!("{}m", minutes));
    }
    if seconds > 0 {
        parts.push(format!("{}s", seconds));
    }

    parts.join(" ")
}

// this must be sorted in descending order
static CHECKPOINTS: [u64; 4] = [
    60 * 10,
    60 * 5,
    60,
    10,
];

fn main() {
    let args = Command::parse();

    let mut timeout = match args.mode {
        Mode::At { reference } => {
            // colon separated time
            match reference.chars().filter(|c| *c == ':').count() {
                0 => {
                    // check for plain seconds
                    match reference.parse::<u64>() {
                        Ok(v) => Duration::seconds(v as i64),
                        Err(_) => {
                            // try using h m s shorthands
                            eprintln!("couldn't parse reference time");
                            exit(1);
                        }
                    }
                }
                1 => {
                    // check fo hh:mm
                    let parsed: NaiveTime = reference.parse().unwrap();
                    let mut target = parsed - Local::now().time();
                    if target < Duration::seconds(0) {
                        target += Duration::days(1);
                    }
                    target
                }
                _ => todo!()
            }
        }
        Mode::In { trailing } => {
            let trailing = trailing.join(" ");
            let caps = regex::Regex::new(r"((\d+)h)? ?((\d+)m)? ?((\d+)s)?").unwrap().captures(&trailing).unwrap();
            println!("{:?}", caps.iter().collect::<Vec<_>>());
            caps.iter()
                .skip(2)
                .enumerate()
                .filter(|(i, _)| i % 2 == 0)
                .filter_map(|(i, m)| m.map(|v| (i, v)))
                .fold(Duration::zero(), |acc, (i, m)| {
                    let value = m.as_str().parse::<i64>().unwrap();
                    println!("{}: {}", i, value);
                    acc + match i {
                        0 => Duration::hours(value),
                        2 => Duration::minutes(value),
                        4 => Duration::seconds(value),
                        _ => unreachable!(),
                    }
                })
        }
    };
    println!("Timer will go off in {} (at {})", format_timedelta(timeout), Local::now() + timeout);

    let title = args.title.unwrap_or("Unnamed timer".to_owned());
    for checkpoint in CHECKPOINTS.iter().chain(vec![&0]).map(|&v| Duration::seconds(v as i64)) {
        // skip checkpoint lower than timeout
        if timeout <= checkpoint {
            continue;
        }
        #[cfg(debug_assertions)]
        println!("==> sleeping for {:?}", timeout - checkpoint);
        #[cfg(not(debug_assertions))]
        std::thread::sleep((timeout - checkpoint).to_std().unwrap());
        timeout = checkpoint;
        #[cfg(feature = "beep")]
        print!("\x07");
        #[cfg(feature = "notify")]
        notify_rust::Notification::new()
            .summary(&format!("{}", title))
            .body(&format!("{} will end in {}", title, format_timedelta(timeout)))
            .show().unwrap();

    }
}

