use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::{fs, io, path::PathBuf};
use dirs;

/// Главный конфиг Nuta.
///
/// Важно: `#[serde(default)]` означает:
/// если какого-то поля нет в TOML — взять его из Default.
/// Поэтому старый/неполный конфиг не ломает редактор.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub editor: EditorConfig,
    pub theme: ThemeConfig,
    pub syntax: SyntaxConfig,

    /// Языковые серверы.
    ///
    /// Пример TOML:
    /// [lsp.rust]
    /// command = "rust-analyzer"
    /// args = []
    pub lsp: HashMap<String, LspServerConfig>,

    pub ai: AiConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            editor: EditorConfig::default(),
            theme: ThemeConfig::default(),
            syntax: SyntaxConfig::default(),
            lsp: HashMap::new(),
            ai: AiConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorConfig {
    pub tab_width: usize,
    pub line_numbers: bool,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            tab_width: 4,
            line_numbers: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub name: String,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: "catppuccin-mocha".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SyntaxConfig {
    pub enabled: bool,
}

impl Default for SyntaxConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LspServerConfig {
    pub command: String,
    pub args: Vec<String>,
}

impl Default for LspServerConfig {
    fn default() -> Self {
        Self {
            command: String::new(),
            args: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    pub enabled: bool,
    pub llama_bin: String,
    pub model: String,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            llama_bin: "llama-cli".into(),
            model: String::new(),
        }
    }
}

impl Config {
    /// Путь конфига:
    /// $XDG_CONFIG_HOME/nuta/config.toml
    /// или ~/.config/nuta/config.toml
    pub fn path() -> PathBuf {
        // dirs::config_dir() автоматически возвращает:
        // Linux: ~/.config
        // macOS: ~/Library/Application Support
        // Windows: C:\Users\Name\AppData\Roaming
        let base = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        base.join("nuta").join("config.toml") 
    }

    /// Загрузить конфиг. Если файла нет — создать дефолтный.
    pub fn load_or_create() -> io::Result<Self> {
        let path = Self::path();

        if path.exists() {
            let text = fs::read_to_string(&path)?;

            return toml::from_str(&text).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("config error in {}: {}", path.display(), e),
                )
            });
        }

        let cfg = Self::default();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let text = toml::to_string_pretty(&cfg).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to serialize default config: {}", e),
            )
        })?;

        fs::write(&path, text)?;

        Ok(cfg)
    }
}
