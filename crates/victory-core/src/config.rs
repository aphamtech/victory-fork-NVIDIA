//! Where the user's files live.
//!
//! Resolution order for each value: environment variable, then the config
//! file (`VICTORY_CONFIG`, else `./victory.toml`), then the default.
//! See `victory.example.toml` at the repository root.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::policy::ensure_outside_repo;

/// The resolution the title renders the game at when nothing else says
/// otherwise: the display mode it is told its output has, which is the
/// size of every screen-bound render target it makes. The 3D scene is
/// filled by pixel work, so this is the one largest lever on its frame
/// time (640x480 is a seventh of 1920x1080's pixels, a twenty-seventh of
/// 3840x2160's). Change it here, set `VICTORY_DISPLAY_MODE=WIDTHxHEIGHT`,
/// or set `[graphics] display_mode` in the config file.
pub const DEFAULT_DISPLAY_MODE: (u32, u32) = (640, 480);

/// The performance profile's tier unless the environment or the config
/// file says otherwise (see `victory_gpu::settings`): 2 is the fastest
/// measured set, 1 the conservative one, 0 every switch at its own
/// default, 3 the one that also gives up exactness.
pub const DEFAULT_PERFORMANCE: u8 = 2;

/// The highest tier the performance profile has.
pub const PERFORMANCE_TIERS: u8 = 3;

/// Whether a console's output can be this size: 640 to 7680 across, 360 to
/// 4320 down.
pub fn display_mode_ok(width: u32, height: u32) -> bool {
    (640..=7680).contains(&width) && (360..=4320).contains(&height)
}

/// `WIDTHxHEIGHT` as a display mode, if it is one ([`display_mode_ok`]).
pub fn parse_display_mode(value: &str) -> Option<(u32, u32)> {
    let (width, height) = value.trim().split_once(['x', 'X'])?;
    let width = width.trim().parse::<u32>().ok()?;
    let height = height.trim().parse::<u32>().ok()?;
    display_mode_ok(width, height).then_some((width, height))
}

#[derive(Debug, Default, Clone)]
pub struct Config {
    /// The user's dump of the title.
    pub game_dir: Option<PathBuf>,
    /// Root for all derived data. Always outside the repository.
    pub cache_dir: PathBuf,
    /// What titles write and must keep (persistent local storage). Never
    /// inside the cache, which may be deleted at any time.
    pub data_dir: PathBuf,
    /// Other titles used to exercise platform-level code.
    pub test_titles: Vec<PathBuf>,
    /// The resolution the title renders at (`[graphics] display_mode`).
    pub display_mode: Option<(u32, u32)>,
    /// The performance profile's tier (`[graphics] performance`).
    pub performance: Option<u8>,
    /// The config file that was read, if any.
    pub source: Option<PathBuf>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    #[serde(default)]
    paths: FilePaths,
    #[serde(default)]
    graphics: FileGraphics,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileGraphics {
    /// `WIDTHxHEIGHT`: the resolution the title renders at.
    display_mode: Option<String>,
    /// The performance profile's tier: 0, 1 or 2.
    performance: Option<u8>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct FilePaths {
    game_dir: Option<PathBuf>,
    cache_dir: Option<PathBuf>,
    data_dir: Option<PathBuf>,
    #[serde(default)]
    test_titles: Vec<PathBuf>,
}

impl Config {
    /// Loads configuration from the environment and the config file.
    pub fn load() -> Result<Self> {
        let explicit = std::env::var_os("VICTORY_CONFIG").map(PathBuf::from);
        let path = explicit
            .clone()
            .unwrap_or_else(|| PathBuf::from("victory.toml"));
        let file = if path.is_file() {
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {}", path.display()))?;
            Some((path, text))
        } else if let Some(p) = explicit {
            anyhow::bail!(
                "VICTORY_CONFIG points at {}, which does not exist",
                p.display()
            );
        } else {
            None
        };
        let (source, parsed) = match file {
            Some((path, text)) => {
                let parsed: FileConfig =
                    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
                (Some(path), parsed)
            }
            None => (None, FileConfig::default()),
        };
        Self::resolve(parsed.paths, parsed.graphics, source, |k| {
            std::env::var_os(k)
        })
    }

    fn resolve(
        file: FilePaths,
        graphics: FileGraphics,
        source: Option<PathBuf>,
        env: impl Fn(&str) -> Option<std::ffi::OsString>,
    ) -> Result<Self> {
        let env_path = |k: &str| env(k).filter(|v| !v.is_empty()).map(PathBuf::from);
        let test_titles = match env("VICTORY_TEST_TITLES") {
            Some(v) if !v.is_empty() => std::env::split_paths(&v)
                .filter(|p| !p.as_os_str().is_empty())
                .collect(),
            _ => file.test_titles,
        };
        let cache_dir = env_path("VICTORY_CACHE_DIR")
            .or(file.cache_dir)
            .map_or_else(|| default_dir("cache"), Ok)?;
        let data_dir = env_path("VICTORY_DATA_DIR")
            .or(file.data_dir)
            .map_or_else(|| default_dir("data"), Ok)?;
        // The environment first, then the file; a value neither can read
        // is refused in the file and ignored in the environment (the
        // display mode reported to the title falls back to the default).
        let display_mode = match env("VICTORY_DISPLAY_MODE").filter(|v| !v.is_empty()) {
            Some(value) => parse_display_mode(&value.to_string_lossy()),
            None => match graphics.display_mode.as_deref() {
                Some(value) => Some(parse_display_mode(value).with_context(|| {
                    format!(
                        "[graphics] display_mode = {value:?} is not WIDTHxHEIGHT \
                         (e.g. \"640x480\")"
                    )
                })?),
                None => None,
            },
        };
        if let Some(performance) = graphics.performance
            && performance > PERFORMANCE_TIERS
        {
            anyhow::bail!(
                "[graphics] performance = {performance} is not 0 (every switch at its \
                 default), 1 (the conservative set), 2 (the fastest measured) or \
                 {PERFORMANCE_TIERS} (that, with two switches that give up exactness)"
            );
        }
        Ok(Config {
            game_dir: env_path("VICTORY_GAME_DIR").or(file.game_dir),
            cache_dir,
            data_dir,
            test_titles,
            display_mode,
            performance: graphics.performance,
            source,
        })
    }

    /// The resolution the title renders at: [`DEFAULT_DISPLAY_MODE`] unless
    /// the config file or `VICTORY_DISPLAY_MODE` says otherwise.
    pub fn display_size(&self) -> (u32, u32) {
        self.display_mode.unwrap_or(DEFAULT_DISPLAY_MODE)
    }

    /// The performance profile's tier ([`DEFAULT_PERFORMANCE`] unless the
    /// config file says otherwise; `VICTORY_PERF` is read by the profile
    /// itself and wins over this).
    pub fn performance(&self) -> u8 {
        self.performance.unwrap_or(DEFAULT_PERFORMANCE)
    }

    /// The game directory, or a helpful error if it is not configured.
    pub fn require_game_dir(&self) -> Result<&Path> {
        self.game_dir.as_deref().context(
            "no game directory configured: set VICTORY_GAME_DIR or [paths].game_dir in \
             victory.toml (see victory.example.toml)",
        )
    }

    /// Returns `<cache>/<sub>`, creating it, after checking the asset policy.
    pub fn cache_subdir(&self, sub: &str) -> Result<PathBuf> {
        subdir(&self.cache_dir, sub)
    }

    /// Returns `<data>/<sub>`, creating it, after checking the asset policy.
    pub fn data_subdir(&self, sub: &str) -> Result<PathBuf> {
        subdir(&self.data_dir, sub)
    }
}

fn subdir(root: &Path, sub: &str) -> Result<PathBuf> {
    let dir = root.join(sub);
    ensure_outside_repo(&dir)?;
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    Ok(dir)
}

/// `%LOCALAPPDATA%/victory/<name>` (elsewhere the XDG or home cache).
fn default_dir(name: &str) -> Result<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_CACHE_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .context("cannot determine a user directory; set VICTORY_CACHE_DIR and VICTORY_DATA_DIR")?;
    Ok(base.join("victory").join(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn environment_overrides_file() {
        let file = FilePaths {
            game_dir: Some("from-file".into()),
            cache_dir: Some("cache-from-file".into()),
            ..Default::default()
        };
        let env = |k: &str| (k == "VICTORY_GAME_DIR").then(|| OsString::from("from-env"));
        let cfg = Config::resolve(file, FileGraphics::default(), None, env).unwrap();
        assert_eq!(cfg.game_dir.unwrap(), PathBuf::from("from-env"));
        assert_eq!(cfg.cache_dir, PathBuf::from("cache-from-file"));
    }

    #[test]
    fn empty_env_values_are_ignored() {
        let file = FilePaths {
            game_dir: Some("from-file".into()),
            ..Default::default()
        };
        let env = |k: &str| (k == "VICTORY_GAME_DIR").then(OsString::new);
        let cfg = Config::resolve(file, FileGraphics::default(), None, env).unwrap();
        assert_eq!(cfg.game_dir.unwrap(), PathBuf::from("from-file"));
    }

    #[test]
    fn empty_test_titles_env_is_ignored() {
        let file = FilePaths {
            test_titles: vec!["from-file".into()],
            ..Default::default()
        };
        let env = |k: &str| (k == "VICTORY_TEST_TITLES").then(OsString::new);
        let cfg = Config::resolve(file, FileGraphics::default(), None, env).unwrap();
        assert_eq!(cfg.test_titles, vec![PathBuf::from("from-file")]);
    }

    #[test]
    fn test_titles_env_splits_paths() {
        let value = std::env::join_paths(["first", "second"]).unwrap();
        let file = FilePaths::default();
        let env = |k: &str| (k == "VICTORY_TEST_TITLES").then(|| value.clone());
        let cfg = Config::resolve(file, FileGraphics::default(), None, env).unwrap();
        assert_eq!(
            cfg.test_titles,
            vec![PathBuf::from("first"), PathBuf::from("second")]
        );
    }

    #[test]
    fn display_mode_env_overrides_file() {
        let graphics = FileGraphics {
            display_mode: Some("1920x1080".into()),
            performance: None,
        };
        let env = |k: &str| (k == "VICTORY_DISPLAY_MODE").then(|| OsString::from("800x600"));
        let cfg = Config::resolve(FilePaths::default(), graphics, None, env).unwrap();
        assert_eq!(cfg.display_size(), (800, 600));
    }

    #[test]
    fn display_mode_defaults_to_640x480() {
        let env = |_: &str| None;
        let cfg =
            Config::resolve(FilePaths::default(), FileGraphics::default(), None, env).unwrap();
        assert_eq!(cfg.display_size(), DEFAULT_DISPLAY_MODE);
        assert_eq!(cfg.performance(), DEFAULT_PERFORMANCE);
    }

    #[test]
    fn a_file_display_mode_is_read() {
        let graphics = FileGraphics {
            display_mode: Some(" 1280 x 720 ".into()),
            performance: Some(1),
        };
        let env = |_: &str| None;
        let cfg = Config::resolve(FilePaths::default(), graphics, None, env).unwrap();
        assert_eq!(cfg.display_size(), (1280, 720));
        assert_eq!(cfg.performance(), 1);
    }

    #[test]
    fn a_bad_file_display_mode_is_refused() {
        let graphics = FileGraphics {
            display_mode: Some("big".into()),
            performance: None,
        };
        let env = |_: &str| None;
        assert!(Config::resolve(FilePaths::default(), graphics, None, env).is_err());
    }

    #[test]
    fn a_bad_file_performance_tier_is_refused() {
        let graphics = FileGraphics {
            display_mode: None,
            performance: Some(9),
        };
        let env = |_: &str| None;
        assert!(Config::resolve(FilePaths::default(), graphics, None, env).is_err());
    }

    #[test]
    fn example_config_parses() {
        let text = include_str!("../../../victory.example.toml");
        let parsed: FileConfig = toml::from_str(text).unwrap();
        assert!(parsed.paths.game_dir.is_some());
        assert!(parsed.graphics.display_mode.is_some());
    }

    #[test]
    fn cache_inside_repo_is_refused() {
        let inside = Path::new(env!("CARGO_MANIFEST_DIR")).join("cache");
        let cfg = Config {
            cache_dir: inside,
            ..Default::default()
        };
        assert!(cfg.cache_subdir("x").is_err());
    }

    #[test]
    fn data_inside_repo_is_refused() {
        let inside = Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        let cfg = Config {
            data_dir: inside,
            ..Default::default()
        };
        assert!(cfg.data_subdir("x").is_err());
    }
}
