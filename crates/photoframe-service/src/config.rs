use std::{
    env, fs,
    net::IpAddr,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::Parser;
use serde::Deserialize;

const DEFAULT_CONFIG_PATH: &str = "/etc/photoframe/config.yaml";

#[derive(Debug, Clone)]
pub enum DisplayFitMode {
    Contain,
    Cover,
}

impl DisplayFitMode {
    fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "contain" => Ok(Self::Contain),
            "cover" => Ok(Self::Cover),
            other => bail!("invalid display_fit_mode '{other}', expected contain|cover"),
        }
    }

    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Contain => "contain",
            Self::Cover => "cover",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub bind_address: IpAddr,
    pub port: u16,
    pub image_dir: PathBuf,
    pub database_path: PathBuf,
    pub slideshow_interval_seconds: u64,
    pub night_mode_start: String,
    pub night_mode_end: String,
    pub frame_poll_interval_seconds: u64,
    pub display_fit_mode: DisplayFitMode,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0"
                .parse()
                .expect("default bind address must be valid"),
            port: 8080,
            image_dir: PathBuf::from("/var/lib/photoframe/images"),
            database_path: PathBuf::from("/var/lib/photoframe/photoframe.sqlite"),
            slideshow_interval_seconds: 30,
            night_mode_start: "20:00".to_string(),
            night_mode_end: "06:00".to_string(),
            frame_poll_interval_seconds: 15,
            display_fit_mode: DisplayFitMode::Contain,
        }
    }
}

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Cli {
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    bind_address: Option<IpAddr>,
    #[arg(long)]
    port: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct FileConfig {
    bind_address: Option<IpAddr>,
    port: Option<u16>,
    image_dir: Option<PathBuf>,
    database_path: Option<PathBuf>,
    slideshow_interval_seconds: Option<u64>,
    night_mode_start: Option<String>,
    night_mode_end: Option<String>,
    frame_poll_interval_seconds: Option<u64>,
    display_fit_mode: Option<String>,
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        let cli = Cli::parse();
        let mut config = Self::default();

        if let Some(ref config_path) = cli.config {
            if !config_path.exists() {
                bail!("configuration file '{}' not found", config_path.display());
            }
            apply_file(&mut config, config_path)?;
        } else {
            let default_path = Path::new(DEFAULT_CONFIG_PATH);
            if default_path.exists() {
                apply_file(&mut config, default_path)?;
            }
        }

        apply_env(&mut config)?;
        apply_cli(&mut config, &cli);
        validate(&config)?;

        Ok(config)
    }
}

fn apply_file(config: &mut AppConfig, path: &Path) -> Result<()> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;
    let parsed: FileConfig = serde_yaml::from_str(&contents)
        .with_context(|| format!("invalid YAML in {}", path.display()))?;

    if let Some(v) = parsed.bind_address {
        config.bind_address = v;
    }
    if let Some(v) = parsed.port {
        config.port = v;
    }
    if let Some(v) = parsed.image_dir {
        config.image_dir = v;
    }
    if let Some(v) = parsed.database_path {
        config.database_path = v;
    }
    if let Some(v) = parsed.slideshow_interval_seconds {
        config.slideshow_interval_seconds = v;
    }
    if let Some(v) = parsed.night_mode_start {
        config.night_mode_start = v;
    }
    if let Some(v) = parsed.night_mode_end {
        config.night_mode_end = v;
    }
    if let Some(v) = parsed.frame_poll_interval_seconds {
        config.frame_poll_interval_seconds = v;
    }
    if let Some(v) = parsed.display_fit_mode {
        config.display_fit_mode = DisplayFitMode::parse(&v)?;
    }

    Ok(())
}

fn apply_env(config: &mut AppConfig) -> Result<()> {
    if let Ok(v) = env::var("PHOTOFRAME_BIND_ADDRESS") {
        config.bind_address = v
            .parse()
            .with_context(|| "invalid PHOTOFRAME_BIND_ADDRESS")?;
    }
    if let Ok(v) = env::var("PHOTOFRAME_PORT") {
        config.port = v
            .parse()
            .with_context(|| "invalid PHOTOFRAME_PORT")?;
    }
    if let Ok(v) = env::var("PHOTOFRAME_IMAGE_DIR") {
        config.image_dir = PathBuf::from(v);
    }
    if let Ok(v) = env::var("PHOTOFRAME_DB_PATH") {
        config.database_path = PathBuf::from(v);
    }
    if let Ok(v) = env::var("PHOTOFRAME_SLIDESHOW_INTERVAL_SECONDS") {
        config.slideshow_interval_seconds = v
            .parse()
            .with_context(|| "invalid PHOTOFRAME_SLIDESHOW_INTERVAL_SECONDS")?;
    }
    if let Ok(v) = env::var("PHOTOFRAME_NIGHT_MODE_START") {
        config.night_mode_start = v;
    }
    if let Ok(v) = env::var("PHOTOFRAME_NIGHT_MODE_END") {
        config.night_mode_end = v;
    }
    if let Ok(v) = env::var("PHOTOFRAME_FRAME_POLL_INTERVAL_SECONDS") {
        config.frame_poll_interval_seconds = v
            .parse()
            .with_context(|| "invalid PHOTOFRAME_FRAME_POLL_INTERVAL_SECONDS")?;
    }
    if let Ok(v) = env::var("PHOTOFRAME_DISPLAY_FIT_MODE") {
        config.display_fit_mode = DisplayFitMode::parse(&v)?;
    }

    Ok(())
}

fn apply_cli(config: &mut AppConfig, cli: &Cli) {
    if let Some(v) = cli.bind_address {
        config.bind_address = v;
    }
    if let Some(v) = cli.port {
        config.port = v;
    }
}

fn validate(config: &AppConfig) -> Result<()> {
    if config.port == 0 {
        bail!("port must be greater than 0");
    }
    if config.slideshow_interval_seconds == 0 {
        bail!("slideshow_interval_seconds must be greater than 0");
    }
    if config.frame_poll_interval_seconds == 0 {
        bail!("frame_poll_interval_seconds must be greater than 0");
    }
    validate_hh_mm("night_mode_start", &config.night_mode_start)?;
    validate_hh_mm("night_mode_end", &config.night_mode_end)?;

    Ok(())
}

pub(crate) fn parse_hh_mm(field: &str, value: &str) -> Result<chrono::NaiveTime> {
    let parts: Vec<&str> = value.split(':').collect();
    if parts.len() != 2
        || parts[0].len() != 2
        || parts[1].len() != 2
        || !parts[0].chars().all(|c| c.is_ascii_digit())
        || !parts[1].chars().all(|c| c.is_ascii_digit())
    {
        bail!("{field} must be in HH:MM format");
    }

    let hours: u32 = parts[0]
        .parse()
        .with_context(|| format!("{field} has invalid hour component"))?;
    let minutes: u32 = parts[1]
        .parse()
        .with_context(|| format!("{field} has invalid minute component"))?;

    chrono::NaiveTime::from_hms_opt(hours, minutes, 0)
        .with_context(|| format!("{field} must be a valid 24-hour time"))
}

pub(crate) fn validate_hh_mm(field: &str, value: &str) -> Result<()> {
    parse_hh_mm(field, value).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hh_mm_valid() {
        let t1 = parse_hh_mm("test", "00:00").unwrap();
        assert_eq!(t1, chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap());

        let t2 = parse_hh_mm("test", "23:59").unwrap();
        assert_eq!(t2, chrono::NaiveTime::from_hms_opt(23, 59, 0).unwrap());

        let t3 = parse_hh_mm("test", "12:30").unwrap();
        assert_eq!(t3, chrono::NaiveTime::from_hms_opt(12, 30, 0).unwrap());
    }

    #[test]
    fn test_parse_hh_mm_invalid() {
        assert!(parse_hh_mm("test", "24:00").is_err());
        assert!(parse_hh_mm("test", "12:60").is_err());
        assert!(parse_hh_mm("test", "8:30").is_err()); // Not 2-digit hour
        assert!(parse_hh_mm("test", "08:3").is_err());  // Not 2-digit minute
        assert!(parse_hh_mm("test", "+8:30").is_err());
        assert!(parse_hh_mm("test", "invalid").is_err());
        assert!(parse_hh_mm("test", "12:34:56").is_err());
    }

    #[test]
    fn test_display_fit_mode_parse() {
        assert!(matches!(DisplayFitMode::parse("contain").unwrap(), DisplayFitMode::Contain));
        assert!(matches!(DisplayFitMode::parse("COVER").unwrap(), DisplayFitMode::Cover));
        assert!(matches!(DisplayFitMode::parse("  contain ").unwrap(), DisplayFitMode::Contain));
        assert!(DisplayFitMode::parse("stretch").is_err());
    }

    #[test]
    fn test_default_config_validation() {
        let config = AppConfig::default();
        assert!(validate(&config).is_ok());
    }

    #[test]
    fn test_config_validation_failures() {
        let config_bad_port = AppConfig {
            port: 0,
            ..Default::default()
        };
        assert!(validate(&config_bad_port).is_err());

        let config_bad_interval = AppConfig {
            slideshow_interval_seconds: 0,
            ..Default::default()
        };
        assert!(validate(&config_bad_interval).is_err());

        let config_bad_poll = AppConfig {
            frame_poll_interval_seconds: 0,
            ..Default::default()
        };
        assert!(validate(&config_bad_poll).is_err());

        let config_bad_night = AppConfig {
            night_mode_start: "bad".to_string(),
            ..Default::default()
        };
        assert!(validate(&config_bad_night).is_err());
    }
}
