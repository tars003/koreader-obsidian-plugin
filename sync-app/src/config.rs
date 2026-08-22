use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub paths: PathsConfig,
    pub kindle: KindleConfig,
    pub behavior: BehaviorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathsConfig {
    /// Directory containing md2kindle.toml
    pub converter_dir: String,
    /// Optional full path to python.exe. Empty = use "python" from PATH.
    #[serde(default)]
    pub python_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KindleConfig {
    pub volume_label: String,
    pub vault_folder: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorConfig {
    pub poll_interval_secs: u64,
    /// Start kindle-vault-sync when the user logs into Windows.
    #[serde(default)]
    pub run_at_startup: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            paths: PathsConfig {
                converter_dir: guess_converter_dir().unwrap_or_default(),
                python_path: String::new(),
            },
            kindle: KindleConfig {
                volume_label: "Kindle".to_string(),
                vault_folder: "obisidian-git-sync.kindle".to_string(),
            },
            behavior: BehaviorConfig {
                poll_interval_secs: 5,
                run_at_startup: false,
            },
        }
    }
}

/// Best-effort locate converter/ next to the exe or cwd (repo layout).
fn guess_converter_dir() -> Option<String> {
    let candidates = [
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("..").join("converter"))),
        std::env::current_dir().ok().map(|d| d.join("converter")),
        std::env::current_dir()
            .ok()
            .map(|d| d.join("..").join("converter")),
    ];
    for cand in candidates.into_iter().flatten() {
        if cand.join("md2kindle.toml").is_file() || cand.join("md2kindle.toml.example").is_file()
        {
            if let Ok(canon) = fs::canonicalize(&cand) {
                return Some(canon.to_string_lossy().trim_start_matches(r"\\?\").to_string());
            }
            return Some(cand.to_string_lossy().to_string());
        }
    }
    None
}

impl AppConfig {
    pub fn config_path() -> PathBuf {
        // Prefer next to the .exe; fall back to cwd.
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                return dir.join("config.toml");
            }
        }
        PathBuf::from("config.toml")
    }

    pub fn log_path() -> PathBuf {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                return dir.join("kindle-sync.log");
            }
        }
        PathBuf::from("kindle-sync.log")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        match fs::read_to_string(&path) {
            Ok(text) => match toml::from_str::<AppConfig>(&text) {
                Ok(cfg) => cfg,
                Err(e) => {
                    eprintln!("config corrupt ({}), using defaults", e);
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&path, text).map_err(|e| format!("write config: {e}"))?;
        Ok(())
    }

    pub fn converter_toml(&self) -> PathBuf {
        Path::new(&self.paths.converter_dir).join("md2kindle.toml")
    }

    pub fn is_configured(&self) -> bool {
        !self.paths.converter_dir.trim().is_empty()
            && Path::new(&self.paths.converter_dir).join("md2kindle.toml").is_file()
    }

    /// Read output_dir from md2kindle.toml at sync time.
    pub fn read_output_dir(&self) -> Result<PathBuf, String> {
        let toml_path = self.converter_toml();
        let text = fs::read_to_string(&toml_path)
            .map_err(|e| format!("read {}: {e}", toml_path.display()))?;
        let value: toml::Value =
            toml::from_str(&text).map_err(|e| format!("parse md2kindle.toml: {e}"))?;
        let out = value
            .get("vault")
            .and_then(|v| v.get("output_dir"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| "md2kindle.toml missing [vault].output_dir".to_string())?;
        Ok(PathBuf::from(out))
    }

    pub fn python_cmd(&self) -> String {
        let p = self.paths.python_path.trim();
        if p.is_empty() {
            "python".to_string()
        } else {
            p.to_string()
        }
    }
}
