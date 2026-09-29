//! The FPS HUD (`IN-0048`, `US-0151`): four rows pinned to the window's top-right below the
//! tab strip — frames drawn per second, the GPU's name, the graphics API and this process'
//! GPU usage.
//!
//! Nothing here exists while the HUD is off: the workspace holds an
//! `Option<Entity<FpsHud>>` and drops it on the first frame the setting is off, which drops
//! the ticker and, on Windows, closes the PDH query. While on, the HUD opens that query on the
//! background executor (the first open in a process takes ~100-165 ms), counts its own renders
//! (it is not `cached`, so it renders on every frame the window draws), turns the count into a
//! rate once a second on the shared `until_next_tick` grid, and samples the GPU counters every
//! 2 s on the background executor. It never asks for a frame beyond that once-a-second refresh,
//! which lands on the same tick as the status bar clock.

use std::time::{Duration, Instant};

use gpui::{
    App, AppContext as _, Context, Entity, GpuSpecs, InteractiveElement as _, IntoElement,
    ParentElement, Pixels, Render, SharedString, Styled, Task, Window, div, px,
};
use gpui_component::{ActiveTheme as _, TITLE_BAR_HEIGHT};

/// The narrowest the HUD gets; a longer adapter name widens it (leftwards), never wraps.
const HUD_WIDTH: Pixels = px(220.);
/// The tab strip under the title bar (the kit's fixed tab height). The HUD starts below it so
/// the strip's `+` and `...`, the caption buttons and the right-dock toggles stay clickable.
const TAB_STRIP_HEIGHT: Pixels = px(32.);
const MARGIN: Pixels = px(12.);

/// How often the rate is republished: once a second, the clock's own grid.
const FPS_INTERVAL: Duration = Duration::from_secs(1);
/// GPU usage is sampled every other tick, the cadence of the status bar's resource sampler.
const GPU_EVERY_TICKS: u64 = 2;

/// The renderer gpui-pre 0.3.7 compiles for this platform. A constant because gpui does not
/// report it at run time: gpui-pre-windows creates only a Direct3D 11 device and logs, but
/// does not expose, the feature level it got; gpui-pre-wgpu asks wgpu for Vulkan or GL and
/// `GpuSpecs` does not say which; macOS has only Metal.
#[cfg(target_os = "windows")]
const GRAPHICS_API: &str = "Direct3D 11";
#[cfg(target_os = "macos")]
const GRAPHICS_API: &str = "Metal";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const GRAPHICS_API: &str = "Vulkan/GL (wgpu)";

pub struct FpsHud {
    device: SharedString,
    /// Renders since the last tick: the one `u64` add the HUD costs per frame while on.
    frames: u64,
    since: Instant,
    /// The last full interval's rate; `None` until the first tick.
    fps: Option<u64>,
    gpu: GpuUsage,
    _ticker: Task<()>,
}

/// What the GPU-usage row shows.
#[derive(Clone, Copy, Debug, PartialEq)]
enum GpuUsage {
    /// No per-process counter on this platform, or the query could not be opened.
    Unavailable,
    /// The query is being opened, or is open and no sample has landed yet.
    Pending,
    Percent(f32),
}

impl FpsHud {
    /// Called on the first frame the HUD is shown. `gpu_specs` may load a vendor library to
    /// read the driver version, so it is asked once here and not per frame.
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        let device = device_name(window.gpu_specs().as_ref());
        cx.new(|cx| {
            let ticker = cx.spawn_in(window, async move |this, window| {
                // The first PDH open in a process takes ~100-165 ms (PDH loads its counter
                // names and providers), so it runs here, off the UI thread, like every sample.
                let mut probe = window
                    .background_executor()
                    .spawn(async { gpu::Probe::open() })
                    .await;
                if probe.is_none()
                    && this
                        .update(window, |hud: &mut Self, _| hud.gpu = GpuUsage::Unavailable)
                        .is_err()
                {
                    return;
                }
                let mut tick = 0u64;
                loop {
                    let wait = oneterm_state::until_next_tick(FPS_INTERVAL);
                    window.background_executor().timer(wait).await;
                    tick += 1;
                    // The HUD is gone (setting off, window closed): stop.
                    if this
                        .update(window, |hud: &mut Self, cx| hud.tick(cx))
                        .is_err()
                    {
                        break;
                    }
                    if tick % GPU_EVERY_TICKS != 0 {
                        continue;
                    }
                    let Some(mut owned) = probe.take() else {
                        continue;
                    };
                    // PDH walks every engine of every process: never on the UI thread. The
                    // reading shows on the next tick's frame, so it asks for no frame itself.
                    let (returned, percent) = window
                        .background_executor()
                        .spawn(async move {
                            let percent = owned.sample();
                            (owned, percent)
                        })
                        .await;
                    probe = Some(returned);
                    if let Some(percent) = percent
                        && this
                            .update(window, |hud: &mut Self, _| {
                                hud.gpu = GpuUsage::Percent(percent)
                            })
                            .is_err()
                    {
                        break;
                    }
                }
            });
            Self {
                device,
                frames: 0,
                since: Instant::now(),
                fps: None,
                // Until the background open answers; `n/a` from the start where there is
                // no counter to open.
                gpu: if cfg!(windows) {
                    GpuUsage::Pending
                } else {
                    GpuUsage::Unavailable
                },
                _ticker: ticker,
            }
        })
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        let now = Instant::now();
        self.fps = Some(per_second(self.frames, now - self.since));
        self.frames = 0;
        self.since = now;
        cx.notify();
    }
}

impl Render for FpsHud {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.frames += 1;
        let theme = cx.theme();
        let (muted, value) = (theme.muted_foreground, theme.popover_foreground);
        div()
            .id("fps-hud")
            .absolute()
            .top(TITLE_BAR_HEIGHT + TAB_STRIP_HEIGHT + MARGIN)
            .right(MARGIN)
            // Opaque to clicks and drags, so they never reach the terminal under the HUD
            // (a selection, or a click a mouse-reporting TUI would receive); the wheel still
            // scrolls what is underneath.
            .block_mouse_except_scroll()
            .min_w(HUD_WIDTH)
            .px_2()
            .py_1()
            .rounded(px(4.))
            .bg(theme.popover)
            .border_1()
            .border_color(theme.border)
            .font_family(theme.mono_font_family.clone())
            .text_xs()
            .children(hud_rows(self.fps, &self.device, self.gpu).into_iter().map(
                |(label, text)| {
                    div()
                        .flex()
                        .justify_between()
                        .gap_2()
                        .child(div().flex_none().text_color(muted).child(label))
                        .child(
                            div()
                                .whitespace_nowrap()
                                .text_right()
                                .text_color(value)
                                .child(text),
                        )
                },
            ))
    }
}

/// Frames counted over `elapsed`, as a whole number per second. The tick is nominally one
/// second but may land up to a quarter late (`until_next_tick`), so the count is scaled.
fn per_second(frames: u64, elapsed: Duration) -> u64 {
    let secs = elapsed.as_secs_f64();
    if secs <= 0.0 {
        return 0;
    }
    (frames as f64 / secs).round() as u64
}

/// The GPU gpui renders with, from `Window::gpu_specs`.
fn device_name(specs: Option<&GpuSpecs>) -> SharedString {
    match specs {
        Some(specs) if specs.device_name.is_empty() => "n/a".into(),
        Some(specs) if specs.is_software_emulated => {
            format!("{} (software)", specs.device_name).into()
        }
        Some(specs) => specs.device_name.clone().into(),
        None => "n/a".into(),
    }
}

/// The four rows, in display order.
fn hud_rows(
    fps: Option<u64>,
    device: &SharedString,
    gpu: GpuUsage,
) -> [(&'static str, SharedString); 4] {
    let fps = fps.map_or("-".into(), |fps| fps.to_string().into());
    let usage = match gpu {
        GpuUsage::Unavailable => "n/a".into(),
        GpuUsage::Pending => "-".into(),
        GpuUsage::Percent(percent) => format!("{percent:.1}%").into(),
    };
    [
        ("FPS", fps),
        ("GPU", device.clone()),
        ("API", GRAPHICS_API.into()),
        ("GPU usage", usage),
    ]
}

/// This process' share of the GPU from PDH's `GPU Engine` counters, the source of Task
/// Manager's GPU column. Every instance is one engine of one process, named as in
/// `pid_4242_luid_0x00000000_0x0000BEEF_phys_0_eng_1_engtype_3D`; the reading is this pid's
/// busiest single engine across all GPUs, which is how Task Manager defines its GPU column
/// (Microsoft, "GPUs in the task manager"), clamped to 100 %.
#[cfg(any(windows, test))]
fn busiest_engine<'a>(
    pid: u32,
    instances: impl IntoIterator<Item = (&'a str, f64)>,
) -> Option<f32> {
    let owner = format!("pid_{pid}_");
    instances
        .into_iter()
        .filter(|(name, _)| name.starts_with(&owner))
        .map(|(_, value)| value.clamp(0.0, 100.0) as f32)
        .reduce(f32::max)
}
#[cfg(windows)]
mod gpu {
    use windows_sys::Win32::System::Performance::{
        PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_MORE_DATA, PdhAddEnglishCounterW,
        PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
    };

    /// An open PDH query on `\GPU Engine(*)\Utilization Percentage`. Opened when the HUD is
    /// shown, closed when it is dropped.
    pub(super) struct Probe {
        query: isize,
        counter: isize,
        /// Reused between samples. PDH writes the instance names after the item array, so
        /// it is sized in bytes; `u64` words give the array its alignment.
        buffer: Vec<u64>,
    }

    impl Probe {
        pub(super) fn open() -> Option<Self> {
            let path: Vec<u16> = "\\GPU Engine(*)\\Utilization Percentage\0"
                .encode_utf16()
                .collect();
            let mut query = 0;
            // SAFETY: `query` is a live out parameter.
            if unsafe { PdhOpenQueryW(std::ptr::null(), 0, &mut query) } != 0 {
                return None;
            }
            let mut probe = Self {
                query,
                counter: 0,
                buffer: Vec::new(),
            };
            // SAFETY: the query is open (and closed by `Drop` either way); `path` is a
            // NUL-terminated wide string that outlives the call. The English name resolves
            // on a localized Windows too.
            if unsafe { PdhAddEnglishCounterW(probe.query, path.as_ptr(), 0, &mut probe.counter) }
                != 0
            {
                return None;
            }
            // Utilization is a rate between two collections; this is the first one.
            // SAFETY: the query is open.
            unsafe { PdhCollectQueryData(probe.query) };
            Some(probe)
        }

        pub(super) fn sample(&mut self) -> Option<f32> {
            // SAFETY: the query is open until `Drop`.
            if unsafe { PdhCollectQueryData(self.query) } != 0 {
                return None;
            }
            let (mut bytes, mut count) = (0u32, 0u32);
            // SAFETY: a null buffer asks for the size, reported through `PDH_MORE_DATA`.
            let sized = unsafe {
                PdhGetFormattedCounterArrayW(
                    self.counter,
                    PDH_FMT_DOUBLE,
                    &mut bytes,
                    &mut count,
                    std::ptr::null_mut(),
                )
            };
            if sized != PDH_MORE_DATA || bytes == 0 {
                return None;
            }
            const {
                assert!(align_of::<u64>() >= align_of::<PDH_FMT_COUNTERVALUE_ITEM_W>());
            }
            self.buffer.clear();
            self.buffer.resize((bytes as usize).div_ceil(8), 0);
            // SAFETY: the buffer holds at least `bytes`, suitably aligned.
            let read = unsafe {
                PdhGetFormattedCounterArrayW(
                    self.counter,
                    PDH_FMT_DOUBLE,
                    &mut bytes,
                    &mut count,
                    self.buffer.as_mut_ptr().cast(),
                )
            };
            if read != 0 {
                return None;
            }
            // SAFETY: PDH filled `count` items at the start of the buffer.
            let items = unsafe {
                std::slice::from_raw_parts(
                    self.buffer.as_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>(),
                    count as usize,
                )
            };
            let names: Vec<(String, f64)> = items
                .iter()
                .map(|item| {
                    // SAFETY: each name is a NUL-terminated wide string in the buffer's tail,
                    // and the value is the double the format asked for.
                    let name = unsafe {
                        let len = (0..).take_while(|&i| *item.szName.add(i) != 0).count();
                        String::from_utf16_lossy(std::slice::from_raw_parts(item.szName, len))
                    };
                    (name, unsafe { item.FmtValue.Anonymous.doubleValue })
                })
                .collect();
            super::busiest_engine(
                std::process::id(),
                names.iter().map(|(name, value)| (name.as_str(), *value)),
            )
        }
    }

    impl Drop for Probe {
        fn drop(&mut self) {
            // SAFETY: opened in `open`, closed exactly once; this releases the counter too.
            unsafe { PdhCloseQuery(self.query) };
        }
    }
}

#[cfg(not(windows))]
mod gpu {
    /// No per-process GPU counter is read outside Windows: the row says `n/a`.
    pub(super) struct Probe;

    impl Probe {
        pub(super) fn open() -> Option<Self> {
            None
        }

        pub(super) fn sample(&mut self) -> Option<f32> {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specs(name: &str, software: bool) -> GpuSpecs {
        GpuSpecs {
            is_software_emulated: software,
            device_name: name.into(),
            driver_name: "Intel Corporation".into(),
            driver_info: "32.0.101".into(),
        }
    }

    #[test]
    fn the_rows_are_fps_gpu_api_and_usage_in_that_order() {
        let device = device_name(Some(&specs("Intel(R) UHD Graphics 770", false)));
        let rows = hud_rows(Some(2), &device, GpuUsage::Percent(1.84));
        assert_eq!(
            rows.map(|(label, text)| format!("{label} {text}")),
            [
                "FPS 2".to_string(),
                "GPU Intel(R) UHD Graphics 770".to_string(),
                format!("API {GRAPHICS_API}"),
                "GPU usage 1.8%".to_string(),
            ]
        );
        let waiting = hud_rows(None, &device, GpuUsage::Pending);
        assert_eq!((waiting[0].1.as_ref(), waiting[3].1.as_ref()), ("-", "-"));
        assert_eq!(hud_rows(None, &device, GpuUsage::Unavailable)[3].1, "n/a");
        assert_eq!(
            device_name(Some(&specs("Microsoft Basic Render Driver", true))),
            "Microsoft Basic Render Driver (software)"
        );
        // gpui-pre-macos 0.3.7 answers `None`; the row says so instead of vanishing.
        assert_eq!(device_name(None), "n/a");
        assert_eq!(device_name(Some(&specs("", false))), "n/a");
    }

    #[test]
    fn n_renders_in_a_second_read_n_fps() {
        assert_eq!(per_second(60, Duration::from_secs(1)), 60);
        assert_eq!(per_second(2, Duration::from_secs(1)), 2);
        // A tick that landed a quarter late still reads per second.
        assert_eq!(per_second(125, Duration::from_millis(1250)), 100);
        assert_eq!(per_second(0, Duration::from_secs(1)), 0);
        assert_eq!(per_second(5, Duration::ZERO), 0);
    }

    #[test]
    fn gpu_usage_is_this_pids_busiest_single_engine() {
        let sample = [
            // This pid: two 3D engines and a copy engine. The busiest engine is the reading,
            // not the sum of an engine type (1.75) nor of everything (2.75).
            (
                "pid_42_luid_0x00000000_0x0000BEEF_phys_0_eng_0_engtype_3D",
                1.25,
            ),
            (
                "pid_42_luid_0x00000000_0x0000BEEF_phys_0_eng_1_engtype_3D",
                0.5,
            ),
            (
                "pid_42_luid_0x00000000_0x0000BEEF_phys_0_eng_2_engtype_Copy",
                1.0,
            ),
            // Another process whose pid starts with the same digits, and a busier one.
            (
                "pid_421_luid_0x00000000_0x0000BEEF_phys_0_eng_0_engtype_3D",
                90.0,
            ),
            (
                "pid_7_luid_0x00000000_0x0000BEEF_phys_0_eng_0_engtype_VideoDecode",
                50.0,
            ),
        ];
        assert_eq!(busiest_engine(42, sample), Some(1.25));
        // A process with no engines has no reading, not zero.
        assert_eq!(busiest_engine(9, sample), None);
        // Two busy engines of one type (this iGPU has two VideoDecode engines) read as the
        // busier one, and a counter overshoot is clamped to 100 %.
        assert_eq!(
            busiest_engine(
                1,
                [
                    ("pid_1_luid_0x0_0x1_phys_0_eng_3_engtype_VideoDecode", 40.0),
                    ("pid_1_luid_0x0_0x1_phys_0_eng_11_engtype_VideoDecode", 30.0),
                ]
            ),
            Some(40.0)
        );
        assert_eq!(
            busiest_engine(1, [("pid_1_luid_0x0_0x1_phys_0_eng_0_engtype_3D", 130.0)]),
            Some(100.0)
        );
    }
}
