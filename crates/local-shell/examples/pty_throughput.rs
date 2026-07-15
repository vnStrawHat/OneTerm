//! ConPTY / PTY throughput probe (diagnostic).
//!
//! Spawns a child command inside a real pseudoconsole via the same
//! `alacritty_terminal::tty` path OneTerm uses, then reads from the PTY as fast as
//! possible and reports MiB/s over the active window (first byte → last byte).
//!
//! This isolates the *transport* (ConPTY relay + reserialization) from OneTerm's
//! parser and renderer — there is no `Term`, no grid, no GUI here. If a raw spewer
//! plateaus near the DOOM-fire rate (~30 MiB/s), ConPTY is the ceiling; if it goes
//! far higher, the limiter is elsewhere (VT density or the producer itself).
//!
//! Usage:
//!   cargo run -p oneterm-local-shell --release --example pty_throughput -- <program> [args...]
//! Example (Windows):
//!   cargo run -p oneterm-local-shell --release --example pty_throughput -- cmd /c type C:\Temp\plain.txt

use std::io::{Read, Write};
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use alacritty_terminal::event::WindowSize;
use alacritty_terminal::tty::{self, EventedReadWrite, Options, Shell};
use polling::{Event as PollEvent, Events, PollMode, Poller};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: pty_throughput <program> [args...]");
        std::process::exit(2);
    }
    let program = args[1].clone();
    let child_args: Vec<String> = args[2..].to_vec();

    let opts = Options {
        shell: Some(Shell::new(program.clone(), child_args.clone())),
        working_directory: None,
        drain_on_exit: false,
        env: Default::default(),
        ..Default::default()
    };
    // Match the DOOM-fire grid so ConPTY's screen-buffer reserialization cost is
    // comparable to the real workload.
    let winsize = WindowSize {
        num_lines: 45,
        num_cols: 160,
        cell_width: 0,
        cell_height: 0,
    };
    let mut pty = tty::new(&opts, winsize, 0).expect("tty::new");

    let poll = std::sync::Arc::new(Poller::new().expect("poller"));
    unsafe {
        pty.register(&poll, PollEvent::readable(0), PollMode::Level)
            .expect("register");
    }
    let mut events = Events::with_capacity(NonZeroUsize::new(1024).unwrap());

    let mut buf = vec![0u8; 1024 * 1024];
    let mut total: u64 = 0;
    let mut first: Option<Instant> = None;
    let mut last = Instant::now();
    let mut head: Vec<u8> = Vec::new();
    let mut cap = std::fs::File::create("firecap.bin").expect("create firecap.bin");
    let mut captured: usize = 0;

    // Interaction: full-screen TUIs (incl. DOOM-fire) often show an intro and wait
    // for Enter before animating. Emulate: read for `wait`, send Enter, then measure
    // the animation phase for `measure` seconds.
    let launch = Instant::now();
    let wait = Duration::from_secs(5);
    let measure = Duration::from_secs(6);
    let mut sent_enter = false;
    let mut measure_start: Option<Instant> = None;
    let mut pre_total: u64 = 0;

    loop {
        let el = launch.elapsed();
        if !sent_enter && el >= wait {
            let _ = pty.writer().write_all(b"\r");
            let _ = pty.writer().flush();
            sent_enter = true;
            measure_start = Some(Instant::now());
            // Reset so we measure only the post-Enter animation phase.
            pre_total = total;
            total = 0;
            first = None;
            head.clear();
        }
        if let Some(ms) = measure_start {
            if ms.elapsed() >= measure {
                break;
            }
        }
        if el >= wait + measure + Duration::from_secs(2) {
            break;
        }

        events.clear();
        if poll
            .wait(&mut events, Some(Duration::from_millis(50)))
            .is_err()
        {
            break;
        }
        for ev in events.iter() {
            if !ev.readable {
                continue;
            }
            loop {
                match pty.reader().read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if first.is_none() {
                            first = Some(Instant::now());
                        }
                        last = Instant::now();
                        total += n as u64;
                        if head.len() < 200 {
                            let take = n.min(200 - head.len());
                            head.extend_from_slice(&buf[..take]);
                        }
                        if sent_enter && captured < 8 * 1024 * 1024 {
                            let take = n.min(8 * 1024 * 1024 - captured);
                            let _ = cap.write_all(&buf[..take]);
                            captured += take;
                        }
                    }
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::Interrupted =>
                    {
                        break;
                    }
                    Err(_) => break,
                }
            }
        }
    }

    let active = first
        .map(|f| last.saturating_duration_since(f).as_secs_f64())
        .unwrap_or(0.0)
        .max(1e-6);
    let mib = total as f64 / (1024.0 * 1024.0);
    let head_str: String = head
        .iter()
        .flat_map(|&b| std::ascii::escape_default(b))
        .map(|b| b as char)
        .collect();
    println!(
        "[pty_throughput] program={program} args={child_args:?}\n\
         [pty_throughput] pre-Enter bytes = {pre_total}\n\
         [pty_throughput] post-Enter: read {mib:.1} MiB over {active:.2}s = {:.1} MiB/s\n\
         [pty_throughput] first post-Enter bytes: {head_str}",
        mib / active
    );
}
