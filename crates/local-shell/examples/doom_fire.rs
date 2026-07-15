//! DOOM-fire — Rust port of <https://github.com/const-void/DOOM-fire-zig>.
//!
//! Renders the classic PSX DOOM-fire effect directly to the terminal using
//! 256-colour ANSI escape sequences and the `▀` (upper-half-block) character
//! to double the vertical resolution.
//!
//! # Multiple-architecture support
//!
//! All buffer indices, sizes, and loop counters use `usize` — the native
//! pointer-width unsigned integer.  This means the code compiles and runs
//! correctly on **both** 32-bit targets (x86, armv7, riscv32 …) and 64-bit
//! targets (x86_64, aarch64, riscv64 …) without any conditional compilation.
//! The original Zig source hard-coded `u64`, which wastes registers and
//! memory on 32-bit platforms; this port fixes the TODO noted in the
//! upstream source.
//!
//! # Usage
//!
//! ```bash
//! cargo run -p oneterm-local-shell --example doom_fire
//! ```
//!
//! Press `Ctrl+C` to quit during the fire animation.
//!
//! Licensed under GPL-3.0 (same as the original).

use std::io::{self, Write};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

// ──────────────────────────────────────────────────────────────────────────
//  Constants — ANSI escape codes (identical to the Zig source)
// ──────────────────────────────────────────────────────────────────────────

// ANSI escape sequences (pre-composed; see Zig source for the building blocks).
#[cfg(windows)]
const NL: &str = "\x1b[0K\x1b[1E\x1b[1G"; // clear-to-eol + next-line + home
#[cfg(not(windows))]
const NL: &str = "\n";

const CURSOR_HOME: &str = "\x1b[1;1H";
const SCREEN_CLEAR: &str = "\x1b[2J";
const CHAR_SET_ASCII: &str = "\x1b(B";

const COLOR_RESET: &str = "\x1b[0m";
const COLOR_DEF: &str = "\x1b[48;5;0m\x1b[38;5;15m"; // black bg + white fg

// term_on  = alt-screen-on + cursor-hide + cursor-home + color-def + screen-clear
const TERM_ON: &str = "\x1b[?1049h\x1b[?25l\x1b[1;1H\x1b[48;5;0m\x1b[38;5;15m\x1b[2J";
// term_off = alt-screen-off + cursor-show + newline
const TERM_OFF: &str = "\x1b[?1049l\x1b[?25h\n";

const PX: &str = "▀"; // upper-half block — two pixel rows per character cell

const MAX_COLOR: usize = 256;
const LAST_COLOR: usize = MAX_COLOR - 1;

// DOOM fire palette — indices into the 256-colour ANSI table.
const FIRE_PALETTE: [u8; 26] = [
    0, 233, 234, 52, 53, 88, 89, 94, 95, 96, 130, 131, 132, 133, 172, 214, 215, 220, 220, 221, 3,
    226, 227, 230, 195, 230,
];
const FIRE_BLACK: u8 = 0;
const FIRE_WHITE: u8 = (FIRE_PALETTE.len() - 1) as u8;

// ──────────────────────────────────────────────────────────────────────────
//  Simple PRNG — xorshift64 seeded from system time (no external deps)
// ──────────────────────────────────────────────────────────────────────────

struct Rng(u64);

impl Rng {
    fn new() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0xDEADBEEF_CAFEBABE);
        // Ensure non-zero state (xorshift requires it).
        Self(seed | 1)
    }

    /// Uniform random integer in `[0, max]` (inclusive).
    #[inline]
    fn range_u8(&mut self, max: u8) -> u8 {
        // xorshift64
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        ((x >> 32) as u32 % (max as u32 + 1)) as u8
    }
}

// ──────────────────────────────────────────────────────────────────────────
//  Terminal size (cross-platform)
// ──────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Default)]
struct TermSz {
    height: usize,
    width: usize,
}

#[cfg(unix)]
mod platform {
    use super::TermSz;

    /// Initialise the terminal — on Unix there's nothing special to do
    /// (ANSI escapes are supported natively).
    pub(super) fn init_term() -> io::Result<()> {
        Ok(())
    }

    /// Get terminal size via `ioctl(TIOCGWINSZ)` on stdout, falling back to
    /// `/dev/tty` (useful when running under a debugger that detaches stdout).
    pub(super) fn term_size() -> io::Result<TermSz> {
        use libc::{TIOCGWINSZ, ioctl, winsize};
        use std::os::fd::AsRawFd;

        let fd = io::stdout().as_raw_fd();
        let mut ws: winsize = unsafe { std::mem::zeroed() };
        let rv = unsafe { ioctl(fd, TIOCGWINSZ, &mut ws) };
        if rv >= 0 && ws.ws_row > 0 && ws.ws_col > 0 {
            return Ok(TermSz {
                height: ws.ws_row as usize,
                width: ws.ws_col as usize,
            });
        }

        // Fallback: open /dev/tty (for when stdout is redirected or in a debugger).
        let tty = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")?;
        let mut ws2: winsize = unsafe { std::mem::zeroed() };
        let rv2 = unsafe { ioctl(tty.as_raw_fd(), TIOCGWINSZ, &mut ws2) };
        if rv2 >= 0 && ws2.ws_row > 0 && ws2.ws_col > 0 {
            return Ok(TermSz {
                height: ws2.ws_row as usize,
                width: ws2.ws_col as usize,
            });
        }

        Err(io::Error::other(
            "failed to determine terminal size via ioctl",
        ))
    }
}

#[cfg(windows)]
mod platform {
    use super::TermSz;
    use std::io;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::System::Console::{
        CONSOLE_SCREEN_BUFFER_INFO, COORD, DISABLE_NEWLINE_AUTO_RETURN,
        ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetConsoleScreenBufferInfo,
        GetStdHandle, STD_OUTPUT_HANDLE, SetConsoleMode, SetConsoleOutputCP,
    };

    /// Enable ANSI VT processing and UTF-8 output on the Windows console.
    pub(super) fn init_term() -> io::Result<()> {
        unsafe {
            let handle = GetStdHandle(STD_OUTPUT_HANDLE);
            if handle.is_null() {
                return Err(io::Error::other("GetStdHandle returned NULL"));
            }

            let mut mode: u32 = 0;
            if GetConsoleMode(handle, &mut mode) == 0 {
                return Err(io::Error::last_os_error());
            }
            mode |= ENABLE_VIRTUAL_TERMINAL_PROCESSING | DISABLE_NEWLINE_AUTO_RETURN;
            if SetConsoleMode(handle, mode) == 0 {
                return Err(io::Error::last_os_error());
            }
            if SetConsoleOutputCP(65001) == 0 {
                // CP_UTF8 — non-fatal
                eprintln!("warning: SetConsoleOutputCP(CP_UTF8) failed");
            }
        }
        Ok(())
    }

    /// Get the *visible* terminal window size (not the internal buffer size).
    pub(super) fn term_size() -> io::Result<TermSz> {
        unsafe {
            let handle = GetStdHandle(STD_OUTPUT_HANDLE);
            let mut info: CONSOLE_SCREEN_BUFFER_INFO = std::mem::zeroed();
            if GetConsoleScreenBufferInfo(handle, &mut info) == 0 {
                return Err(io::Error::last_os_error());
            }
            // srWindow gives the visible portion; the buffer may be larger.
            let w = (info.srWindow.Right - info.srWindow.Left + 1) as usize;
            let h = (info.srWindow.Bottom - info.srWindow.Top + 1) as usize;
            Ok(TermSz {
                height: h,
                width: w,
            })
        }
    }

    // Suppress unused-import warnings for items referenced only in signatures.
    const _: (HANDLE, COORD) = (std::ptr::null_mut(), COORD { X: 0, Y: 0 });
}

// ──────────────────────────────────────────────────────────────────────────
//  Emitter — write helper
// ──────────────────────────────────────────────────────────────────────────

fn emit(s: &str) -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    out.write_all(s.as_bytes())?;
    out.flush()?;
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────
//  Color cache — pre-compute fg/bg ANSI strings for all 256 colours
// ──────────────────────────────────────────────────────────────────────────

struct ColorCache {
    fg: Vec<String>,
    bg: Vec<String>,
}

impl ColorCache {
    fn new() -> Self {
        let mut fg = Vec::with_capacity(MAX_COLOR);
        let mut bg = Vec::with_capacity(MAX_COLOR);
        for i in 0..MAX_COLOR {
            fg.push(format!("\x1b[38;5;{i}m"));
            bg.push(format!("\x1b[48;5;{i}m"));
        }
        Self { fg, bg }
    }
}

// ──────────────────────────────────────────────────────────────────────────
//  Frame buffer — accumulate one frame's worth of VT bytes, write once
// ──────────────────────────────────────────────────────────────────────────

struct FrameBuf {
    buf: Vec<u8>,
    /// Minimum frame size seen (bytes).
    sz_min: usize,
    /// Maximum frame size seen (bytes).
    sz_max: usize,
    /// Running average frame size (bytes).
    sz_avg: usize,
    /// Frame counter.
    frame_tic: usize,
    /// Program start time.
    t_start: Instant,
}

impl FrameBuf {
    fn new(capacity: usize) -> Self {
        Self {
            buf: Vec::with_capacity(capacity),
            sz_min: 0,
            sz_max: 0,
            sz_avg: 0,
            frame_tic: 0,
            t_start: Instant::now(),
        }
    }

    #[inline]
    fn reset(&mut self) {
        self.buf.clear();
    }

    #[inline]
    fn draw(&mut self, s: &str) {
        self.buf.extend_from_slice(s.as_bytes());
    }

    fn paint(&mut self) -> io::Result<()> {
        let stdout = io::stdout();
        let mut out = stdout.lock();
        out.write_all(&self.buf)?;
        out.flush()?;

        let bs_len = self.buf.len();
        self.frame_tic += 1;
        if self.sz_min == 0 {
            self.sz_min = bs_len;
            self.sz_max = bs_len;
            self.sz_avg = bs_len;
        } else {
            if bs_len < self.sz_min {
                self.sz_min = bs_len;
            }
            if bs_len > self.sz_max {
                self.sz_max = bs_len;
            }
            // Running average: avg = avg * (n-1)/n + len/n
            let n = self.frame_tic;
            self.sz_avg = self.sz_avg * (n - 1) / n + bs_len / n;
        }

        let dur = self.t_start.elapsed().as_secs_f64();
        let fps = self.frame_tic as f64 / dur.max(1e-6);

        let min_s = fmt_size_bin(self.sz_min);
        let avg_s = fmt_size_bin(self.sz_avg);
        let max_s = fmt_size_bin(self.sz_max);
        let stat =
            format!("\x1b[38;5;0mmem: {min_s} min / {avg_s} avg / {max_s} max [ {fps:.2} fps ]");
        out.write_all(stat.as_bytes())?;
        out.flush()?;
        Ok(())
    }
}

/// Format a byte count in binary units (KiB, MiB, …) — mirrors Zig's
/// `std.fmt.fmtIntSizeBin`.
fn fmt_size_bin(bytes: usize) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut val = bytes as f64;
    let mut idx = 0;
    while val >= 1024.0 && idx < UNITS.len() - 1 {
        val /= 1024.0;
        idx += 1;
    }
    format!("{val:.2} {}", UNITS[idx])
}

// ──────────────────────────────────────────────────────────────────────────
//  DOOM-fire application
// ──────────────────────────────────────────────────────────────────────────

struct DoomFire {
    term_sz: TermSz,
    colors: ColorCache,
    rng: Rng,
}

impl DoomFire {
    fn new() -> io::Result<Self> {
        platform::init_term()?;
        let term_sz = platform::term_size()?;
        let colors = ColorCache::new();
        let rng = Rng::new();

        // Enter alternate screen buffer, hide cursor, reset colours, clear.
        emit(TERM_ON)?;
        #[cfg(windows)]
        emit(CHAR_SET_ASCII)?;

        Ok(Self {
            term_sz,
            colors,
            rng,
        })
    }

    fn complete(&self) -> io::Result<()> {
        emit(TERM_OFF)?;
        emit("Complete!\n")
    }

    // ── DOOM fire ───────────────────────────────────────────────────────────

    fn show_doom_fire(&mut self) -> io::Result<()> {
        // Term size → fire size.  Height is doubled because `▀` renders two
        // pixel rows per character cell.
        let fire_w: usize = self.term_sz.width;
        let fire_h: usize = self.term_sz.height * 2;
        let fire_sz: usize = fire_h * fire_w;
        let fire_last_row: usize = (fire_h - 1) * fire_w;

        // Allocate the fire pixel buffer.
        let mut screen_buf = vec![FIRE_BLACK; fire_sz];

        // Last row is white — the "fire source".
        for x in 0..fire_w {
            screen_buf[fire_last_row + x] = FIRE_WHITE;
        }

        // Reset terminal.
        emit(CURSOR_HOME)?;
        emit(COLOR_RESET)?;
        emit(COLOR_DEF)?;
        emit(SCREEN_CLEAR)?;

        // Scope-cached init frame: cursor_home + bg[black] + fg[black]
        let init_frame = format!(
            "{}{}{}",
            CURSOR_HOME, &self.colors.bg[0], &self.colors.fg[0]
        );

        // Estimate buffer capacity — generous to avoid reallocation.
        let px_color_sz = self.colors.bg[LAST_COLOR].len() + self.colors.fg[LAST_COLOR].len();
        let px_sz = px_color_sz + PX.len();
        let screen_sz = px_sz * fire_w * fire_w;
        let overflow_sz = PX.len() * 100;
        let bs_sz = (screen_sz + overflow_sz) * 2;

        let mut fb = FrameBuf::new(bs_sz);

        // Main animation loop — Ctrl+C to exit.
        loop {
            // ── Update fire buffer ──────────────────────────────────────────
            for x in 0..fire_w {
                for y in 0..fire_h {
                    let idx = y * fire_w + x;
                    let spread_px = screen_buf[idx];

                    if spread_px == 0 && idx >= fire_w {
                        // Extinguish: propagate black upwards.
                        screen_buf[idx - fire_w] = 0;
                    } else {
                        let spread_rnd_idx = self.rng.range_u8(3); // 0..=3
                        let spread_dst = if idx >= (spread_rnd_idx as usize + 1) {
                            idx - spread_rnd_idx as usize + 1
                        } else {
                            idx
                        };
                        if spread_dst >= fire_w {
                            let decay = spread_rnd_idx & 1;
                            if spread_px > decay {
                                screen_buf[spread_dst - fire_w] = spread_px - decay;
                            } else {
                                screen_buf[spread_dst - fire_w] = 0;
                            }
                        }
                    }
                }
            }

            // ── Paint fire buffer ────────────────────────────────────────────
            fb.reset();
            fb.draw(&init_frame);

            let mut px_prev_hi: u8 = FIRE_BLACK;
            let mut px_prev_lo: u8 = FIRE_BLACK;

            // Step by 2: each `▀` renders two pixel rows (hi = fg colour, lo = bg colour).
            let mut y = 0;
            while y < fire_h {
                for x in 0..fire_w {
                    let px_hi = screen_buf[y * fire_w + x];
                    let px_lo = screen_buf[(y + 1) * fire_w + x];

                    if px_lo != px_prev_lo {
                        fb.draw(&self.colors.bg[FIRE_PALETTE[px_lo as usize] as usize]);
                    }
                    if px_hi != px_prev_hi {
                        fb.draw(&self.colors.fg[FIRE_PALETTE[px_hi as usize] as usize]);
                    }
                    fb.draw(PX);

                    px_prev_hi = px_hi;
                    px_prev_lo = px_lo;
                }
                fb.draw(NL);
                y += 2;
            }

            fb.paint()?;
            fb.reset();
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────
//  Main
// ──────────────────────────────────────────────────────────────────────────

fn main() -> io::Result<()> {
    let mut fire = DoomFire::new()?;
    // Ensure cleanup even on early return (panic still won't trigger this, but
    // Ctrl+C sends SIGINT which terminates the process; the terminal's
    // alternate-screen restore is handled by the OS on most modern terminals).
    let result = fire.show_doom_fire();
    // Restore terminal on any error path.
    let _ = fire.complete();
    result
}
