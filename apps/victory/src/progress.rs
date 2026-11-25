//! A small window showing how far a long step has got (`precompile`).
//!
//! Windows belong to the thread that made them, so the window is a thread of
//! its own: the work threads only bump counters, and a timer repaints from
//! them every tenth of a second. Without a desktop (or on another OS) there
//! is no window and nothing changes.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub(crate) struct State {
    pub(crate) containers: AtomicU64,
    pub(crate) translated: AtomicU64,
    pub(crate) cached: AtomicU64,
    pub(crate) failed: AtomicU64,
}

/// A window with the counts, alive until it is dropped.
pub struct Progress {
    state: Arc<State>,
    /// Held for the window's lifetime: its `Drop` closes it.
    #[cfg(windows)]
    _window: Option<imp::Window>,
}

/// The counters of a [`Progress`], for the threads that do the work.
#[derive(Clone)]
pub struct Counter(Arc<State>);

impl Counter {
    /// One more container: `Some(true)` came from the cache, `Some(false)`
    /// was translated now, `None` could not be.
    pub fn container(&self, outcome: Option<bool>) {
        self.0.containers.fetch_add(1, Ordering::Relaxed);
        let counter = match outcome {
            Some(true) => &self.0.cached,
            Some(false) => &self.0.translated,
            None => &self.0.failed,
        };
        counter.fetch_add(1, Ordering::Relaxed);
    }
}

impl Progress {
    /// Opens the window; a failure (no desktop, no session) leaves it out.
    pub fn new(title: &str) -> Self {
        let state = Arc::new(State::default());
        #[cfg(windows)]
        let window = imp::Window::new(title, state.clone(), std::time::Instant::now());
        Self {
            state,
            #[cfg(windows)]
            _window: window,
        }
    }

    /// A handle for the threads that do the work.
    pub fn counter(&self) -> Counter {
        Counter(self.state.clone())
    }
}

#[cfg(windows)]
mod imp {
    use std::sync::Arc;
    use std::sync::atomic::Ordering;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        BeginPaint, CreateSolidBrush, DEFAULT_GUI_FONT, DeleteObject, DrawTextW, EndPaint, FillRect,
        GetStockObject, InvalidateRect, PAINTSTRUCT, SelectObject, SetBkMode, SetTextColor,
        TRANSPARENT, UpdateWindow,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
        GetClientRect, GetMessageW, MSG, PostMessageW, PostQuitMessage, RegisterClassW,
        SW_SHOWNORMAL, SetTimer, ShowWindow, TranslateMessage, WM_CLOSE, WM_DESTROY, WM_PAINT,
        WM_TIMER, WNDCLASSW, WS_CAPTION, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_VISIBLE,
    };

    use super::State;

    const TIMER: usize = 1;
    const WIDTH: i32 = 520;
    const HEIGHT: i32 = 132;
    const BACKGROUND: u32 = rgb(28, 28, 30);
    const TEXT: u32 = rgb(230, 230, 232);
    const DIM: u32 = rgb(150, 150, 156);
    const TRACK: u32 = rgb(56, 56, 60);
    const FILL: u32 = rgb(90, 170, 250);

    const fn rgb(r: u8, g: u8, b: u8) -> u32 {
        (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
    }

    /// The window's handle, safe to pass between threads: only Win32 sees
    /// it (a post to our own window; everything else stays on its thread).
    #[derive(Clone, Copy)]
    struct Handle(HWND);

    // SAFETY: the handle is only passed back to Win32 as an argument.
    unsafe impl Send for Handle {}
    // SAFETY: as above; no state behind it is touched from here.
    unsafe impl Sync for Handle {}

    /// The window's thread and handle, kept by the [`super::Progress`].
    pub(super) struct Window {
        hwnd: Handle,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    /// What the paint reads: the counts and when the window opened.
    static SHOWING: std::sync::OnceLock<(Arc<State>, std::time::Instant)> = std::sync::OnceLock::new();

    impl Window {
        pub(super) fn new(
            title: &str,
            state: Arc<State>,
            started: std::time::Instant,
        ) -> Option<Self> {
            let _ = SHOWING.set((state, started));
            let (wanted, made) = std::sync::mpsc::channel::<Handle>();
            let title = wide(title);
            let thread = std::thread::Builder::new()
                .name("victory-progress".into())
                .spawn(move || run(&title, &wanted))
                .ok()?;
            let hwnd = made.recv().ok()?;
            if hwnd.0.is_null() {
                let _ = thread.join();
                return None;
            }
            Some(Window {
                hwnd,
                thread: Some(thread),
            })
        }
    }

    impl Drop for Window {
        fn drop(&mut self) {
            // SAFETY: a plain post to our own window; it ends its loop.
            unsafe { PostMessageW(self.hwnd.0, WM_CLOSE, 0, 0) };
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }

    fn run(title: &[u16], made: &std::sync::mpsc::Sender<Handle>) {
        // SAFETY: plain Win32 calls with our own class, window and handles.
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let class = wide("VictoryProgress");
            let wc = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(wndproc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: instance,
                hIcon: std::ptr::null_mut(),
                hCursor: std::ptr::null_mut(),
                hbrBackground: std::ptr::null_mut(),
                lpszMenuName: std::ptr::null(),
                lpszClassName: class.as_ptr(),
            };
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                WIDTH,
                HEIGHT,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null_mut(),
            );
            if hwnd.is_null() {
                let _ = made.send(Handle(hwnd));
                return;
            }
            let _ = made.send(Handle(hwnd));
            ShowWindow(hwnd, SW_SHOWNORMAL);
            UpdateWindow(hwnd);
            SetTimer(hwnd, TIMER, 100, None);
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    unsafe extern "system" fn wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_TIMER => {
                // SAFETY: a plain invalidation of our own window.
                unsafe { InvalidateRect(hwnd, std::ptr::null(), 0) };
                0
            }
            // SAFETY: painted between BeginPaint and EndPaint below.
            WM_PAINT => unsafe {
                paint(hwnd);
                0
            },
            // SAFETY: our own window.
            WM_CLOSE => unsafe {
                DestroyWindow(hwnd);
                0
            },
            // SAFETY: ends this thread's loop.
            WM_DESTROY => unsafe {
                PostQuitMessage(0);
                0
            },
            // SAFETY: the default handling of anything else.
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    /// The two lines of text: the counts, then the rate and the time taken.
    fn lines() -> [String; 2] {
        let Some((state, started)) = SHOWING.get() else {
            return ["shaders".to_owned(), String::new()];
        };
        let count = |n: &std::sync::atomic::AtomicU64| n.load(Ordering::Relaxed);
        let containers = count(&state.containers);
        let elapsed = started.elapsed().as_secs_f64();
        [
            format!(
                "{} containers      {} translated      {} from the cache      {} failed",
                thousands(containers),
                thousands(count(&state.translated)),
                thousands(count(&state.cached)),
                thousands(count(&state.failed)),
            ),
            format!(
                "{:.0} a second      {}",
                if elapsed > 0.5 {
                    containers as f64 / elapsed
                } else {
                    0.0
                },
                clock(elapsed)
            ),
        ]
    }

    fn thousands(n: u64) -> String {
        let digits = n.to_string();
        let mut out = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i) % 3 == 0 {
                out.push(' ');
            }
            out.push(c);
        }
        out
    }

    fn clock(seconds: f64) -> String {
        let s = seconds as u64;
        if s >= 3600 {
            format!("{} h {} min", s / 3600, s % 3600 / 60)
        } else if s >= 60 {
            format!("{} min {} s", s / 60, s % 60)
        } else {
            format!("{s} s")
        }
    }

    /// SAFETY: called from `WM_PAINT` on the window's own thread, between
    /// `BeginPaint` and `EndPaint`.
    unsafe fn paint(hwnd: HWND) {
        // SAFETY: the paint structures and GDI handles are ours, made and
        // freed here.
        unsafe {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut client = RECT::default();
            GetClientRect(hwnd, &mut client);
            let background = CreateSolidBrush(BACKGROUND);
            FillRect(hdc, &client, background);
            DeleteObject(background);

            let old = SelectObject(hdc, GetStockObject(DEFAULT_GUI_FONT));
            SetBkMode(hdc, TRANSPARENT as i32);
            let text = lines();
            let mut at = 14;
            for (i, line) in text.iter().enumerate() {
                SetTextColor(hdc, if i == 0 { TEXT } else { DIM });
                let mut rect = RECT {
                    left: 16,
                    top: at,
                    right: client.right - 16,
                    bottom: at + 24,
                };
                let words: Vec<u16> = line.encode_utf16().collect();
                DrawTextW(hdc, words.as_ptr(), words.len() as i32, &mut rect, 0);
                at += 26;
            }

            // No total is known before every file is read, so the bar sweeps.
            let top = client.bottom - 40;
            let left = 16;
            let right = client.right - 16;
            let track = CreateSolidBrush(TRACK);
            FillRect(
                hdc,
                &RECT {
                    left,
                    top,
                    right,
                    bottom: top + 12,
                },
                track,
            );
            DeleteObject(track);
            let seconds = SHOWING
                .get()
                .map_or(0.0, |(_, started)| started.elapsed().as_secs_f64());
            let phase = (seconds * 1.6).fract() as f32;
            let span = (right - left) as f32;
            let fill = CreateSolidBrush(FILL);
            FillRect(
                hdc,
                &RECT {
                    left: left + (span * (phase - 0.25).max(0.0)) as i32,
                    top,
                    right: left + (span * phase) as i32,
                    bottom: top + 12,
                },
                fill,
            );
            DeleteObject(fill);
            SelectObject(hdc, old);
            EndPaint(hwnd, &ps);
        }
    }
}

#[cfg(not(windows))]
mod imp {}
