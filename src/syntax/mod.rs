use regex::Regex;
use serde::Deserialize;
use std::{fs, path::Path};

use crate::{config::Config, theme};

/// Один цветной кусок строки.
/// style — &'static str, чтобы легко встраивалось в текущий render.
#[derive(Debug, Clone)]
pub struct Segment {
    pub text: String,
    pub style: &'static str,
}

/// Определение языка из TOML.
#[derive(Debug, Deserialize)]
struct LanguageDef {
    name: String,
    extensions: Vec<String>,
    #[serde(default, rename = "rule")]
    rules: Vec<RuleDef>,
}

#[derive(Debug, Deserialize)]
struct RuleDef {
    regex: String,
    group: String,
    #[serde(default)]
    capture: Option<usize>,
}

/// Язык с уже скомпилированными regex.
pub struct Language {
    pub name: String,
    pub extensions: Vec<String>,
    rules: Vec<CompiledRule>,
}

struct CompiledRule {
    re: Regex,
    group: String,
    capture: Option<usize>,
}

pub struct Highlighter {
    languages: Vec<Language>,
}

impl Highlighter {
    pub fn new() -> Self {
        let mut highlighter = Self {
            languages: Vec::new(),
        };

        // Встроенные языки.
        const BUILTINS: &[&str] = &[
            include_str!("languages/python.toml"),
            include_str!("languages/c.toml"),
            include_str!("languages/cpp.toml"),
            include_str!("languages/rust.toml"),
            include_str!("languages/asm.toml"),
        ];

        for text in BUILTINS {
            highlighter.load_toml(text);
        }

        // Пользовательские языки/переопределения.
        highlighter.load_user_dir();

        highlighter
    }

    fn load_user_dir(&mut self) {
        let Some(config_dir) = Config::path().parent().map(|p| p.to_path_buf()) else {
            return;
        };

        let syntax_dir = config_dir.join("syntax");

        let Ok(entries) = fs::read_dir(syntax_dir) else {
            return;
        };

        let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();

        for path in paths {
            if path.extension().and_then(|s| s.to_str()) != Some("toml") {
                continue;
            }

            if let Ok(text) = fs::read_to_string(&path) {
                self.load_toml(&text);
            }
        }
    }

    fn load_toml(&mut self, text: &str) {
        let Ok(def) = toml::from_str::<LanguageDef>(text) else {
            return;
        };

        let mut rules = Vec::new();

        for rule in def.rules {
            // Невалидные правила пропускаем, чтобы один плохой regex не ломал редактор.
            if let Ok(re) = Regex::new(&rule.regex) {
                rules.push(CompiledRule {
                    re,
                    group: rule.group,
                    capture: rule.capture
                });
            }
        } 

        self.languages.push(Language {
            name: def.name,
            extensions: def.extensions,
            rules,
        });
    }

    /// Определяем язык по расширению.
    /// Пользовательские файлы грузятся после встроенных,
    /// поэтому ищем с конца — так пользователь может переопределить язык.
    pub fn detect(&self, path: Option<&Path>) -> Option<&Language> {
        let ext = path?.extension()?.to_str()?.to_lowercase();

        self.languages.iter().rev().find(|lang| {
            lang.extensions
                .iter()
                .any(|e| e.eq_ignore_ascii_case(&ext))
        })
    }

    /// Подсветка одной строки.
    pub fn highlight_line(&self, lang: Option<&Language>, line: &str) -> Vec<Segment> {
        let Some(lang) = lang else {
            return plain_segment(line);
        };

        if line.is_empty() {
            return Vec::new();
        }

        // Собираем все совпадения всех правил.
        let mut matches: Vec<(usize, usize, usize)> = Vec::new();

        for (rule_idx, rule) in lang.rules.iter().enumerate() {
            for caps in rule.re.captures_iter(line) {
                let idx = rule.capture.unwrap_or(0);
                if let Some(m) = caps.get(idx) {
                    if m.start() == m.end() { continue; }
                    matches.push((m.start(), m.end(), rule_idx));
                }
            }
        } 

        // Сортируем:
        // 1. по началу;
        // 2. по порядку правила в TOML;
        // 3. при одинаковом начале — более длинное совпадение раньше.
        matches.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then(a.2.cmp(&b.2))
                .then(b.1.cmp(&a.1))
        });

        let mut segments = Vec::new();
        let mut pos = 0;

        for (start, end, rule_idx) in matches {
            // Пересечения пропускаем.
            if start < pos {
                continue;
            }

            if start > pos {
                segments.push(Segment {
                    text: line[pos..start].to_string(),
                    style: theme::FG_TEXT,
                });
            }

            let group = &lang.rules[rule_idx].group;

            segments.push(Segment {
                text: line[start..end].to_string(),
                style: group_style(group),
            });

            pos = end;
        }

        if pos < line.len() {
            segments.push(Segment {
                text: line[pos..].to_string(),
                style: theme::FG_TEXT,
            });
        }

        if segments.is_empty() {
            plain_segment(line)
        } else {
            segments
        }
    }
}

fn plain_segment(line: &str) -> Vec<Segment> {
    if line.is_empty() {
        Vec::new()
    } else {
        vec![Segment {
            text: line.to_string(),
            style: theme::FG_TEXT,
        }]
    }
}

/// Маппинг групп на Catppuccin.
/// Имена групп специально похожи на Vim.
fn group_style(group: &str) -> &'static str {
    match group {
        "Comment" => theme::FG_OVERLAY,
        "String" => theme::FG_GREEN,
        "Number" => theme::FG_PEACH,
        "Keyword" => theme::FG_MAUVE,
        "Type" => theme::FG_YELLOW,
        "Function" => theme::FG_BLUE,
        "PreProc" => theme::FG_PINK,
        "Special" => theme::FG_TEAL,
        "Macro" => theme::FG_PINK,
        "Label" => theme::FG_BLUE,
        "Register" => theme::FG_TEAL,
        "Operator" => theme::FG_SUBTEXT,
        _ => theme::FG_TEXT,
    }
}
