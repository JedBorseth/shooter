//! Per-system CPU time, for the CPU benchmark (`godot/tests/cpu_bench.gd`)
//! and the dedicated server's `SHOOTER_CPU_PROFILE` report.
//!
//! Off unless something turns it on, so normal play pays one relaxed atomic
//! load per scope. Rust systems time themselves with [`scope`]; GDScript wraps
//! a block in `CpuProfile.begin(&"name")` and `CpuProfile.end()`. Everything
//! runs on Godot's main thread, so the table is thread-local.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use godot::prelude::*;

static ENABLED: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
struct Entry {
    name: String,
    total_us: f64,
    calls: u64,
    max_us: f64,
    /// Calls that took longer than [`SLOW_US`].
    slow: u64,
    /// Time in the frame now under way, and each finished frame's total
    /// (see [`end_frame`]), for medians that a hitch cannot skew.
    frame_us: f64,
    frames: Vec<f32>,
}

/// A call this long is counted as slow: half of a server tick at 120 Hz,
/// or a quarter of a frame at 60 fps.
const SLOW_US: f64 = 4000.0;

#[derive(Default)]
struct Table {
    entries: Vec<Entry>,
    /// Frames closed so far: a system first seen later starts with zeros.
    frame_count: usize,
    /// Open GDScript scopes: entry index and start time.
    stack: Vec<(usize, Instant)>,
}

impl Table {
    fn index(&mut self, name: &str) -> usize {
        if let Some(i) = self.entries.iter().position(|e| e.name == name) {
            return i;
        }
        self.entries.push(Entry {
            name: name.to_owned(),
            frames: vec![0.0; self.frame_count],
            ..Entry::default()
        });
        self.entries.len() - 1
    }

    fn add(&mut self, index: usize, us: f64) {
        let entry = &mut self.entries[index];
        entry.total_us += us;
        entry.calls += 1;
        entry.max_us = entry.max_us.max(us);
        entry.frame_us += us;
        if us > SLOW_US {
            entry.slow += 1;
        }
    }
}

thread_local! {
    static TABLE: RefCell<Table> = RefCell::new(Table::default());
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// Times until dropped, under `name` (a system, such as "server/bots").
#[must_use]
pub fn scope(name: &'static str) -> Scope {
    Scope(enabled().then(|| (name, Instant::now())))
}

pub struct Scope(Option<(&'static str, Instant)>);

impl Drop for Scope {
    fn drop(&mut self) {
        if let Some((name, start)) = self.0 {
            let us = start.elapsed().as_secs_f64() * 1e6;
            TABLE.with_borrow_mut(|t| {
                let i = t.index(name);
                t.add(i, us);
            });
        }
    }
}

/// Closes a frame (or tick): each system's time in it joins its per-frame
/// list, so the report can give a median. Systems that did not run count as
/// zero for that frame.
pub fn end_frame() {
    TABLE.with_borrow_mut(|t| {
        t.frame_count += 1;
        for e in &mut t.entries {
            e.frames.push((e.frame_us / 1000.0) as f32);
            e.frame_us = 0.0;
        }
    });
}

fn percentile(frames: &[f32], at: f32) -> f32 {
    if frames.is_empty() {
        return 0.0;
    }
    let mut sorted = frames.to_vec();
    sorted.sort_by(f32::total_cmp);
    sorted[((sorted.len() - 1) as f32 * at).round() as usize]
}

/// Every system's totals since the last reset: name to `total_ms`, `calls`,
/// `max_ms` (the longest single call), `slow_calls` (calls over 4 ms), and,
/// when frames were closed with [`end_frame`], `median_ms` and `p95_ms` per
/// frame.
pub fn report() -> VarDictionary {
    let mut out = VarDictionary::new();
    TABLE.with_borrow(|t| {
        for e in &t.entries {
            let mut d = VarDictionary::new();
            d.set("total_ms", e.total_us / 1000.0);
            d.set("calls", e.calls as i64);
            d.set("max_ms", e.max_us / 1000.0);
            d.set("slow_calls", e.slow as i64);
            d.set("frames", e.frames.len() as i64);
            d.set("median_ms", percentile(&e.frames, 0.5));
            d.set("p95_ms", percentile(&e.frames, 0.95));
            out.set(e.name.as_str(), &d);
        }
    });
    out
}

pub fn reset() {
    TABLE.with_borrow_mut(|t| {
        t.entries.clear();
        t.stack.clear();
        t.frame_count = 0;
    });
}

/// Per-system CPU time for benchmarks. Off by default.
#[derive(GodotClass)]
#[class(no_init, base = Object)]
pub struct CpuProfile;

#[godot_api]
impl CpuProfile {
    #[func]
    fn set_enabled(on: bool) {
        set_enabled(on);
    }

    #[func]
    fn is_enabled() -> bool {
        enabled()
    }

    /// Starts timing `name` until the matching `end()`. Scopes nest.
    #[func]
    fn begin(name: StringName) {
        if !enabled() {
            return;
        }
        let start = Instant::now();
        TABLE.with_borrow_mut(|t| {
            let i = t.index(&name.to_string());
            t.stack.push((i, start));
        });
    }

    #[func]
    fn end() {
        if !enabled() {
            return;
        }
        let now = Instant::now();
        TABLE.with_borrow_mut(|t| {
            if let Some((i, start)) = t.stack.pop() {
                t.add(i, (now - start).as_secs_f64() * 1e6);
            }
        });
    }

    /// Closes a frame for the per-frame medians (see [`end_frame`]).
    #[func]
    fn end_frame() {
        if enabled() {
            end_frame();
        }
    }

    /// Totals since the last reset (see [`report`]).
    #[func]
    fn report() -> VarDictionary {
        report()
    }

    #[func]
    fn reset() {
        reset();
    }
}
