use std::{
    env, fs,
    net::IpAddr,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::Parser;
use serde::Deserialize;

const DEFAULT_CONFIG_PATH: &str = "/etc/photoframe/config.yaml";
const ENV_PREFIX: &str = "PHOTOFRAME_";

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
        let config_path = cli
            .config
            .clone()
            .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH));

        let mut config = Self::default();
        if config_path.exists() {
            apply_file(&mut config, &config_path)?;
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
    if let Ok(v) = env::var(format!("{ENV_PREFIX}BIND_ADDRESS")) {
        config.bind_address = v
            .parse()
            .with_context(|| format!("invalid {ENV_PREFIX}BIND_ADDRESS"))?;
    }
    if let Ok(v) = env::var(format!("{ENV_PREFIX}PORT")) {
        config.port = v
            .parse()
            .with_context(|| format!("invalid {ENV_PREFIX}PORT"))?;
    }
    if let Ok(v) = env::var(format!("{ENV_PREFIX}IMAGE_DIR")) {
        config.image_dir = PathBuf::from(v);
    }
    if let Ok(v) = env::var(format!("{ENV_PREFIX}DB_PATH")) {
        config.database_path = PathBuf::from(v);
    }
    if let Ok(v) = env::var(format!("{ENV_PREFIX}SLIDESHOW_INTERVAL_SECONDS")) {
        config.slideshow_interval_seconds = v
            .parse()
            .with_context(|| format!("invalid {ENV_PREFIX}SLIDESHOW_INTERVAL_SECONDS"))?;
    }
    if let Ok(v) = env::var(format!("{ENV_PREFIX}NIGHT_MODE_START")) {
        config.night_mode_start = v;
    }
    if let Ok(v) = env::var(format!("{ENV_PREFIX}NIGHT_MODE_END")) {
        config.night_mode_end = v;
    }
    if let Ok(v) = env::var(format!("{ENV_PREFIX}FRAME_POLL_INTERVAL_SECONDS")) {
        config.frame_poll_interval_seconds = v
            .parse()
            .with_context(|| format!("invalid {ENV_PREFIX}FRAME_POLL_INTERVAL_SECONDS"))?;
    }
    if let Ok(v) = env::var(format!("{ENV_PREFIX}DISPLAY_FIT_MODE")) {
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

pub(crate) fn validate_hh_mm(field: &str, value: &str) -> Result<()> {
    let parts: Vec<&str> = value.split(':').collect();
    if parts.len() != 2 {
        bail!("{field} must be in HH:MM format");
    }

    let hours: u8 = parts[0]
        .parse()
        .with_context(|| format!("{field} has invalid hour component"))?;
    let minutes: u8 = parts[1]
        .parse()
        .with_context(|| format!("{field} has invalid minute component"))?;

    if hours > 23 || minutes > 59 {
        bail!("{field} must be a valid 24-hour time");
    }

    Ok(())
}
