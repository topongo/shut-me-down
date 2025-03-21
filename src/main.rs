use std::process::exit;
use chrono::{Duration, TimeDelta};

use chrono::{Local, NaiveTime};
use clap::{Parser, ValueEnum};

#[derive(Parser, Debug)]
struct Command {
    #[cfg(feature = "notify")]
    #[arg(short, long)]
    title: Option<String>,
    mode: Mode,
    reference: String,
    #[arg(trailing_var_arg = true)]
    exec: Vec<String>,
}

#[derive(Debug, ValueEnum, Clone)]
enum Mode {
    At,
    In,
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

#[tokio::main]
async fn main() {
    let args = Command::parse();

    let timeout = match args.mode {
        Mode::At => {
            // colon separated time
            let target = match args.reference.chars().filter(|c| *c == ':').count() {
                0 => {
                    // check for hh
                    match args.reference.parse::<u32>() {
                        Ok(v) => match NaiveTime::from_hms_opt(v, 0, 0) {
                            Some(t) => t - Local::now().time(),
                            None => {
                                eprintln!("couldn't parse reference time: {}", args.reference);
                                exit(1);
                            }
                        }
                        Err(_) => {
                            eprintln!("couldn't parse reference time: {}", args.reference);
                            exit(1);
                        }
                    }
                }
                1 | 2 => {
                    // check for hh:mm:ss
                    match args.reference.parse::<NaiveTime>() {
                        Ok(v) => v - Local::now().time(),
                        Err(_) => {
                            eprintln!("couldn't parse reference time: {}", args.reference);
                            exit(1);
                        }
                    }
                }
                _ => todo!()
            };
            if target < Duration::zero() {
                target + Duration::days(1)
            } else {
                target
            }
        }
        Mode::In => {
            // safe unwrap: regex is ok at compile time
            let caps = match regex::Regex::new(r"((\d+)h)? ?((\d+)m)? ?((\d+)s)?").unwrap().captures(&args.reference) {
                Some(c) => c,
                None => {
                    eprintln!("couldn't parse reference time");
                    exit(1);
                }
            };
            // println!("{:?}", caps.iter().collect::<Vec<_>>());
            caps.iter()
                .skip(2)
                .enumerate()
                .filter(|(i, _)| i % 2 == 0)
                .filter_map(|(i, m)| m.map(|v| (i, v)))
                .fold(Duration::zero(), |acc, (i, m)| {
                    let value = match m.as_str().parse::<i64>() {
                        Ok(v) => v,
                        Err(_) => {
                            eprintln!("invalid numeric value: {}", m.as_str());
                            exit(1);
                        }
                    };
                    // println!("{}: {}", i, value);
                    acc + match i {
                        0 => Duration::hours(value),
                        2 => Duration::minutes(value),
                        4 => Duration::seconds(value),
                        _ => unreachable!(),
                    }
                })
        }
    };

    // make program exit if command is invalid
    let command = match args.exec.len() {
        0 => None,
        _ => {
            let mut cmd = std::process::Command::new(&args.exec[0]);
            cmd.args(&args.exec[1..]);
            Some(cmd)
        }
    };

    println!("Timer will go off in {} (at {})", format_timedelta(timeout), Local::now() + timeout);

    let title = args.title.unwrap_or("Unnamed timer".to_owned());

    let mut notifiers = vec![];
    for checkpoint in CHECKPOINTS.iter().chain(vec![&(timeout.num_seconds() as u64)]).map(|&v| Duration::seconds(v as i64)) {
        // skip checkpoint lower than timeout
        if timeout < checkpoint {
            continue;
        }
        // start async waiters
        notifiers.push(tokio::spawn(wait_and_notify(timeout, checkpoint, title.clone())));
    }

    // wait for all async waiters to finish
    for n in notifiers {
        n.await.unwrap();
    }

    // execute end command if provided, otherwise 
    match command {
        Some(mut cmd) => {
            let status = cmd
                .spawn()
                .unwrap()
                .wait()
                .unwrap();
            if !status.success() {
                eprintln!("command failed: {:?}", cmd);
                exit(1);
            }
        }
        None => notify(&title, "Time's up!"),
    }
    // if !args.exec.is_empty() {
    //     let mut cmd = std::process::Command::new(&args.exec[0])
    //         .args(&args.exec[1..])
    //         .spawn().unwrap();
    //     cmd.wait().unwrap();
    // } else {
    //     notify(&title, "Time's up!");
    // }
}

async fn wait_and_notify(timeout: TimeDelta, checkpoint: TimeDelta, title: String) {
    let wait = timeout - checkpoint;
    #[cfg(debug_assertions)]
    println!("==> sleeping for {:?}", wait);
    #[cfg(not(debug_assertions))]
    {
        use tokio::time::Instant;
        let target = Instant::now() + wait.to_std().expect("timeout is negative");
        // println!("==> sleeping until {:?}", target);
        tokio::time::sleep_until(target).await;
    }
    notify(&title, &format!("Will end in {}", format_timedelta(checkpoint)));
}

fn notify(title: &str, body: &str) {
    #[cfg(feature = "notify")]
    notify_rust::Notification::new()
        .summary(title)
        .body(body)
        .show().unwrap();
    #[cfg(not(feature = "notify"))]
    println!("{}: {}", title, body);
    #[cfg(feature = "beep")]
    print!("\x07");
}
