//! Translate every shader found in files or directories into the shader
//! cache a run reads, before the run: a first launch otherwise translates
//! all of them while the loading screen waits (`translate` reports what
//! they cannot handle; this one leaves the translations where the run
//! finds them).
//!
//! The entries are keyed by the translator and every setting it reads
//! (`victory-gpu::shader_cache`), so the run must be this same build, with
//! the same translation settings in the environment (`VICTORY_SHADER_LOOP_LIMIT`,
//! `VICTORY_GROUP_SYNC`, `VICTORY_SHADER_OPT`) and the same host wave size.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use anyhow::{Context, Result};
use victory_shader::emit::validator::Validator;
use victory_shader::{Container, FourCC};

use crate::progress::Progress;
use crate::shader::{MAX_FILE, Source, sources};

#[derive(Default)]
struct Tally {
    containers: usize,
    duplicates: usize,
    /// Translations the cache already held, and ones made now.
    cached: usize,
    translated: usize,
    /// With `--validate`: the emitted containers dxil.dll accepted, and
    /// those it did not, by its first line.
    validated: usize,
    invalid: BTreeMap<String, (usize, String)>,
    /// Containers that failed, by the translator's first line, with an
    /// example of one.
    failed: BTreeMap<String, (usize, String)>,
}

impl Tally {
    fn fail(&mut self, reason: &str, example: impl FnOnce() -> String) {
        let reason = reason.lines().next().unwrap_or_default().to_owned();
        self.failed
            .entry(reason)
            .or_insert_with(|| (0, example()))
            .0 += 1;
    }

    fn reject(&mut self, reason: &str, example: impl FnOnce() -> String) {
        let reason = reason.lines().next().unwrap_or_default().to_owned();
        self.invalid
            .entry(reason)
            .or_insert_with(|| (0, example()))
            .0 += 1;
    }

    fn merge(&mut self, other: Tally) {
        self.containers += other.containers;
        self.duplicates += other.duplicates;
        self.cached += other.cached;
        self.translated += other.translated;
        self.validated += other.validated;
        for (reason, (n, example)) in other.failed {
            self.failed
                .entry(reason)
                .or_insert_with(|| (0, example))
                .0 += n;
        }
        for (reason, (n, example)) in other.invalid {
            self.invalid
                .entry(reason)
                .or_insert_with(|| (0, example))
                .0 += n;
        }
    }
}

pub fn run(
    paths: Vec<PathBuf>,
    host_wave: Option<u32>,
    jobs: Option<usize>,
    strict: bool,
    gui: bool,
    validate: bool,
) -> Result<()> {
    let paths = crate::default_paths(paths)?;
    // A run translates the title's shaders for the widest wave its own device
    // runs and the cache keys on it, so the same number has to be used here.
    let wave = match host_wave {
        Some(wave) => wave,
        None => victory_hle::modules::d3d12x::host_wave_max().context(
            "no host Direct3D 12 device to ask its widest wave; pass --host-wave \
             (32 for NVIDIA, 64 for AMD RDNA)",
        )?,
    };
    anyhow::ensure!(
        wave.is_power_of_two() && (4..=128).contains(&wave),
        "{wave} is not a wave size (4 to 128, a power of two)"
    );
    victory_shader::emit::set_host_max_wave(wave);
    let cfg = victory_core::Config::load()?;
    let dir = cfg.cache_subdir("shaders")?;
    victory_hle::modules::d3d12x::use_shader_cache(dir.clone());
    println!("host wave    {wave} lanes");
    println!("shader cache {}", dir.display());
    let dll = validate.then(Validator::default_path).flatten();
    if validate {
        Validator::load(dll.as_deref()).map_err(anyhow::Error::msg)?;
        println!(
            "validator    {}",
            dll.as_ref().map_or("dxil.dll".into(), |p| p.display().to_string())
        );
    }
    let progress = gui.then(|| Progress::new("VIctory - precompiling shaders"));

    let started = Instant::now();
    let threads =
        jobs.unwrap_or_else(|| std::thread::available_parallelism().map_or(4, |n| n.get()));
    let seen = Mutex::new(HashSet::new());
    let done = AtomicUsize::new(0);
    let total = Mutex::new(Tally::default());
    for root in &paths {
        for entry in walkdir::WalkDir::new(root)
            .into_iter()
            .filter_map(Result::ok)
        {
            if !entry.file_type().is_file() || entry.metadata().map_or(0, |m| m.len()) > MAX_FILE {
                continue;
            }
            // Our own cache and saves can sit inside the game folder, and
            // their files are not the title's shaders.
            let path = entry.path();
            if path.starts_with(&cfg.cache_dir) || path.starts_with(&cfg.data_dir) {
                continue;
            }
            let Ok(data) = std::fs::read(path) else {
                continue;
            };
            let containers = sources(&data);
            if containers.is_empty() {
                continue;
            }
            let next = AtomicUsize::new(0);
            std::thread::scope(|s| {
                for _ in 0..threads.min(containers.len()) {
                    s.spawn(|| {
                        let mut t = Tally::default();
                        let counter = progress.as_ref().map(Progress::counter);
                        let mut validator = validate
                            .then(|| Validator::load(dll.as_deref()).ok())
                            .flatten();
                        while let Some(source) = containers.get(next.fetch_add(1, Ordering::Relaxed))
                        {
                            let outcome = one(source, path, &seen, &mut t, &mut validator);
                            if let (Some(counter), Some(hit)) = (&counter, outcome.outcome()) {
                                counter.container(hit);
                            }
                            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                            if n % 1000 == 0 {
                                println!(
                                    "  {n} containers, {:.0} a second",
                                    n as f64 / started.elapsed().as_secs_f64().max(1e-3)
                                );
                            }
                        }
                        lock(&total).merge(t);
                    });
                }
            });
        }
    }
    drop(progress);
    let t = total.into_inner().unwrap_or_else(|e| e.into_inner());
    println!(
        "containers   {} ({} duplicates skipped)",
        t.containers, t.duplicates
    );
    println!(
        "translated   {} stored now, {} already in the cache, {} failed",
        t.translated,
        t.cached,
        t.failed.values().map(|f| f.0).sum::<usize>()
    );
    let mut failed: Vec<_> = t.failed.into_iter().collect();
    failed.sort_by_key(|(_, (n, _))| std::cmp::Reverse(*n));
    for (reason, (n, example)) in &failed {
        println!("  {n:>7}  {reason}  (e.g. {example})");
    }
    if validate {
        println!(
            "DXIL         {} of {} validated",
            t.validated,
            t.validated + t.invalid.values().map(|f| f.0).sum::<usize>()
        );
        let mut invalid: Vec<_> = t.invalid.into_iter().collect();
        invalid.sort_by_key(|(_, (n, _))| std::cmp::Reverse(*n));
        for (reason, (n, example)) in &invalid {
            println!("  {n:>7}  {reason}  (e.g. {example})");
        }
    }
    println!(
        "cache        {} entries, {:.0} MB; {:.1} s",
        count(&dir),
        bytes(&dir) as f64 / (1 << 20) as f64,
        started.elapsed().as_secs_f64()
    );
    if strict && !failed.is_empty() {
        anyhow::bail!("{} containers failed", failed.len());
    }
    Ok(())
}

/// What became of one container, for the progress window.
enum Done {
    /// Stored now, or already in the cache (`true`).
    Counted(bool),
    /// It could not be translated.
    Failed,
    /// Not a container to translate (a duplicate, a corrupt record): the
    /// tally says why.
    Skipped,
}

impl Done {
    fn outcome(&self) -> Option<Option<bool>> {
        match self {
            Done::Counted(hit) => Some(Some(*hit)),
            Done::Failed => Some(None),
            Done::Skipped => None,
        }
    }
}

/// Translates one container into the cache, counting what it found there.
fn one(
    source: &Source,
    path: &Path,
    seen: &Mutex<HashSet<blake3::Hash>>,
    t: &mut Tally,
    validator: &mut Option<Validator>,
) -> Done {
    let example = || format!("{}{}", path.display(), source.label());
    let Some(bytes) = source.bytes() else {
        t.fail("corrupt record", example);
        return Done::Skipped;
    };
    t.containers += 1;
    let Ok(container) = Container::parse(&bytes) else {
        t.fail("not a DXBC container", example);
        return Done::Skipped;
    };
    // The run translates a container as the title passes it; the same DXIL
    // in another container is the same translation.
    let Some(dxil) = container.part(FourCC::DXIL) else {
        t.fail("no DXIL part (SM5 or older)", example);
        return Done::Skipped;
    };
    if !lock(seen).insert(blake3::hash(dxil.data)) {
        t.duplicates += 1;
        return Done::Skipped;
    }
    match victory_hle::modules::d3d12x::precompile_shader(&bytes) {
        Ok((mut dxil, hit)) => {
            if hit {
                t.cached += 1;
            } else {
                t.translated += 1;
            }
            if let Some(validator) = validator {
                match validator.validate(&mut dxil) {
                    Ok(()) => t.validated += 1,
                    Err(e) => t.reject(&e, example),
                }
            }
            Done::Counted(hit)
        }
        Err(e) => {
            t.fail(&e, example);
            Done::Failed
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn count(dir: &Path) -> usize {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .count()
}

fn bytes(dir: &Path) -> u64 {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}
