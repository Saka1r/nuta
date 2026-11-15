pub mod buffer;
pub mod modes;

use crate::event::Event;
use crate::lsp::LspClient;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::mpsc;
use url::Url;


use buffer::TextBuffer;
use modes::Mode;
use crate::terminal::{CursorShape, Key, Screen};
use crate::syntax::Highlighter;
use crate::config::Config;
use crate::theme;

use std::io;
use std::path::PathBuf;
use unicode_width::{UnicodeWidthStr, UnicodeWidthChar}; 

/// Тип строки справки. Заголовки, пары "клавиша-описание", пустые строки.
enum HelpLine {
    Title,
    Header(&'static str),
    Entry(&'static str, &'static str),
    Blank,
}

const HELP: &[HelpLine] = &[
    HelpLine::Title,
    HelpLine::Blank,
    HelpLine::Header("MODES"),
    HelpLine::Entry("Normal", "навигация и команды (фиолетовый бейдж)"),
    HelpLine::Entry("Insert", "ввод текста (зелёный бейдж)"),
    HelpLine::Entry("Visual", "выделение (розовый бейдж)"),
    HelpLine::Blank,
    HelpLine::Header("NORMAL — movement"),
    HelpLine::Entry("h j k l / arrows", "движение курсора"),
    HelpLine::Entry("0 / Home", "в начало строки"),
    HelpLine::Entry("$ / End", "в конец строки"),
    HelpLine::Entry("gg / G", "в начало / конец файла"),
    HelpLine::Entry("PgUp / PgDn", "на экран вверх / вниз"),
    HelpLine::Blank,
    HelpLine::Header("NORMAL — enter Insert"),
    HelpLine::Entry("i / a", "вставить до / после курсора"),
    HelpLine::Entry("I / A", "в начало / конец строки"),
    HelpLine::Entry("o / O", "новая строка снизу / сверху"),
    HelpLine::Blank,
    HelpLine::Header("NORMAL — editing"),
    HelpLine::Entry("x", "удалить символ"),
    HelpLine::Entry("dd / yy", "удалить / копировать строку"),
    HelpLine::Entry("p", "вставить"),
    HelpLine::Entry("u", "отменить"),
    HelpLine::Entry("v", "режим Visual"),
    HelpLine::Entry("?", "эта справка"),
    HelpLine::Blank,
    HelpLine::Header("INSERT"),
    HelpLine::Entry("Esc", "вернуться в Normal"),
    HelpLine::Entry("Enter / Backspace", "новая строка / удалить назад"),
    HelpLine::Entry("Delete", "удалить символ под курсором"),
    HelpLine::Entry("Tab", "4 пробела"),
    HelpLine::Blank,
    HelpLine::Header("VISUAL"),
    HelpLine::Entry("h j k l", "расширить выделение"),
    HelpLine::Entry("d / x", "вырезать"),
    HelpLine::Entry("y", "копировать"),
    HelpLine::Entry("Esc", "снять выделение"),
    HelpLine::Blank,
    HelpLine::Header("COMMANDS (:)"),
    HelpLine::Entry(":w [path]", "сохранить"),
    HelpLine::Entry(":q / :q!", "выйти / выйти без сохранения"),
    HelpLine::Entry(":wq / :x", "сохранить и выйти"),
    HelpLine::Entry(":help / :h", "эта справка"),
    HelpLine::Blank,
    HelpLine::Header("GLOBAL"),
    HelpLine::Entry("Ctrl+S", "сохранить"),
    HelpLine::Entry("Ctrl+Q", "выйти"),
    HelpLine::Blank,
    HelpLine::Header("FILES / BUFFERS"),
    HelpLine::Entry(":e <path>", "открыть файл"),
    HelpLine::Entry(":Ex", "файловый менеджер"),
    HelpLine::Entry(":ls", "список буферов"),
    HelpLine::Entry(":bn / :bp", "следующий / предыдущий буфер"),
    HelpLine::Entry(":bd / :bd!", "закрыть буфер / принудительно"),
    HelpLine::Blank,
    HelpLine::Header("EXPLORER"),
    HelpLine::Entry("j / k", "вверх / вниз"),
    HelpLine::Entry("Enter", "открыть файл / директорию"),
    HelpLine::Entry("-", "на уровень выше"),
    HelpLine::Entry("q / Esc", "закрыть"),
    HelpLine::Blank,
    HelpLine::Header("SYNTAX / LSP"),
    HelpLine::Entry(":syntax", "статистика подсветки"),
    HelpLine::Entry(":lsp", "статус language server"),
    HelpLine::Entry("config", "~/.config/nuta/config.toml"),
    HelpLine::Entry("LSP servers", "rust-analyzer, pyright, clangd, asm-lsp"),
];

/// Неактивный буфер, ожидающий переключения.
struct ParkedBuffer {
    text: TextBuffer,
    filepath: Option<PathBuf>,
    dirty: bool,
    scroll_row: usize,
    undo_stack: Vec<TextBuffer>,
}

struct DirEntry {
    name: String,
    is_dir: bool,
    path: PathBuf,
}

#[derive(Clone)]
struct Diagnostic {
    start_line: usize,
    start_col: usize,
    end_line: usize,
    end_col: usize,
    severity: u8, // 1 = error, 2 = warning
    message: String,
}

pub struct Editor {
    buffer: TextBuffer,
    mode: Mode,
    scroll_row: usize,          // первая видимая строка файла
    view_height: usize,         // высота текстовой области (из render)
    filepath: Option<PathBuf>,
    dirty: bool,
    status: String,
    command: Option<String>,    // Some(...) = набираем :команду
    pub quit: bool,
    anchor: Option<(usize, usize)>, // якорь Visual-выделения
    clipboard: String,
    undo_stack: Vec<TextBuffer>,    // undo снапшотами всего буфера (просто и надёжно)
    pending: Option<char>,      // первая клавиша двухклавишных команд: g, d, y
    show_help: bool,
    help_scroll: usize,
    config: Config,
    highlighter: Highlighter,
    parked: Vec<ParkedBuffer>,
    show_explorer: bool,
    explorer_dir: PathBuf,
    explorer_entries: Vec<DirEntry>,
    explorer_cursor: usize,
    explorer_scroll: usize,
    lsp: Option<LspClient>,
    lsp_ready: bool,
    lsp_diagnostics: usize,
    diagnostics: Vec<Diagnostic>,
    lsp_version: i32,
    lsp_tx: Option<mpsc::Sender<Event>>,
    lsp_lang: Option<String>,
}

fn language_id(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_lowercase();

    match ext.as_str() {
        "rs" => Some("rust"),
        "py" => Some("python"),
        "c" | "h" => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => Some("cpp"),
        "asm" | "s" | "nasm" => Some("asm"),
        _ => None,
    }
}

fn path_to_uri(path: &Path) -> io::Result<String> {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };

    Url::from_file_path(abs)
        .map(|url| url.to_string())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "failed to convert path to file URI"))
}

impl Editor {
    pub fn new(filepath: Option<String>, config: Config) -> io::Result<Self> {
        let (buffer, filepath) = match filepath {
            Some(path) => {
                let p = PathBuf::from(&path);
                let buf = if p.exists() { TextBuffer::load(&p)? } else { TextBuffer::new() };
                (buf, Some(p)) // несуществующий файл = новый файл с этим именем
            }
            None => (TextBuffer::new(), None),
        };
        Ok(Self {
            buffer,
            mode: Mode::Normal,
            scroll_row: 0,
            view_height: 24,
            filepath,
            dirty: false,
            status: "nuta — i: insert, :w save, :q quit, v: visual".into(),
            command: None,
            quit: false,
            anchor: None,
            clipboard: String::new(),
            undo_stack: Vec::new(),
            pending: None,
            show_help: false,
            help_scroll: 0,
            config,
            highlighter: Highlighter::new(),
            parked: Vec::new(),
            show_explorer: false,
            explorer_dir: PathBuf::new(),
            explorer_entries: Vec::new(),
            explorer_cursor: 0,
            explorer_scroll: 0,
            lsp: None,
            lsp_ready: false,
            lsp_diagnostics: 0,
            diagnostics: Vec::new(),
            lsp_version: 1,
            lsp_tx: None,
            lsp_lang: None,
        })
    }
    fn help_key(&mut self, key: Key) {
        match key {
            Key::Esc | Key::Char('q') | Key::Char('?') => {
                self.show_help = false;
                self.help_scroll = 0;
            }
            Key::Char('j') | Key::Down => self.help_scroll += 1,
            Key::Char('k') | Key::Up => self.help_scroll = self.help_scroll.saturating_sub(1),
            Key::PageDown => self.help_scroll += self.view_height,
            Key::PageUp => self.help_scroll = self.help_scroll.saturating_sub(self.view_height),
            _ => {}
        }
    }

    // ---------- lsp ------------
    pub fn start_lsp(&mut self, tx: mpsc::Sender<Event>) {

        self.lsp_tx = Some(tx.clone());

        let Some(path) = self.filepath.clone() else {
            return;
        };

        let Some(lang) = language_id(&path) else {
            return;
        };

        let Some(server) = self.config.lsp.get(lang).cloned() else {
            return;
        };

        if server.command.is_empty() {
            return;
        }

        let root_uri = path.parent().and_then(|p| path_to_uri(p).ok());

        match LspClient::start(&server.command, &server.args, root_uri, tx) {
            Ok(client) => {
                self.lsp = Some(client);
                self.lsp_lang = Some(lang.to_string());
                self.status = format!("LSP starting: {}", server.command);
            }
            Err(e) => {
                self.lsp_lang = None;
                self.status = format!("LSP start error: {}", e);
            }
        }
    }

    fn refresh_lsp_for_buffer(&mut self) {
        self.diagnostics.clear();
        self.lsp_diagnostics = 0;

        let Some(path) = self.filepath.clone() else {
            self.lsp = None;
            self.lsp_ready = false;
            self.lsp_lang = None;
            return;
        };

        let Some(lang) = language_id(&path) else {
            self.lsp = None;
            self.lsp_ready = false;
            self.lsp_lang = None;
            return;
        };

        if self.lsp.is_none() || self.lsp_lang.as_deref() != Some(lang) {
            self.lsp = None;
            self.lsp_ready = false;

            if let Some(tx) = self.lsp_tx.clone() {
                self.start_lsp(tx);
            }
        } else if self.lsp_ready {
            self.lsp_did_open();
        }
    }

    pub fn handle_event(&mut self, event: Event) {
        match event {
            Event::Key(key) => {
                self.handle_key(key);
                let prev_mode = self.mode;

                if prev_mode == Mode::Insert && self.mode == Mode::Normal {
                    self.lsp_did_change();
                }
            }
            Event::LspMessage(value) => self.handle_lsp_message(value),
            Event::LspError(e) => self.status = format!("LSP error: {}", e),
            Event::LspExit => {
                self.status = "LSP exited".into();
                self.lsp = None;
                self.lsp_ready = false;
            }
        }
    }

    fn parse_diagnostics(&mut self, params: &Value) {
        self.diagnostics.clear();

        let Some(items) = params.get("diagnostics").and_then(Value::as_array) else {
            self.lsp_diagnostics = 0;
            return;
        };

        for item in items {
            let Some(range) = item.get("range") else {
                continue;
            };

            let start = range.get("start");
            let end = range.get("end");

            let start_line = start
                .and_then(|p| p.get("line"))
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;

            let start_col = start
                .and_then(|p| p.get("character"))
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;

            let end_line = end
                .and_then(|p| p.get("line"))
                .and_then(Value::as_u64)
                .unwrap_or(start_line as u64) as usize;

            let end_col = end
                .and_then(|p| p.get("character"))
                .and_then(Value::as_u64)
                .unwrap_or(start_col as u64) as usize;

            let severity = item
                .get("severity")
                .and_then(Value::as_u64)
                .unwrap_or(1) as u8;

            let message = item
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();

            self.diagnostics.push(Diagnostic {
                start_line,
                start_col,
                end_line,
                end_col,
                severity,
                message,
            });
        }

        self.lsp_diagnostics = self.diagnostics.len();
    }

    fn lsp_did_change(&mut self) {
        if !self.lsp_ready {
            return;
        }

        let Some(path) = self.filepath.as_ref() else {
            return;
        };

        let Ok(uri) = path_to_uri(path) else {
            return;
        };

        self.lsp_version += 1;
        let text = self.buffer.lines.join("\n");

        if let Some(lsp) = self.lsp.as_mut() {
            let _ = lsp.notify(
                "textDocument/didChange",
                json!({
                    "textDocument": {
                        "uri": uri,
                        "version": self.lsp_version
                    },
                    "contentChanges": [
                        { "text": text }
                    ]
                }),
            );
        }
    }

    fn handle_lsp_message(&mut self, msg: Value) {

        // Запрос от сервера к клиенту: есть и id, и method.
        if let (Some(id), Some(method)) = (msg.get("id"), msg.get("method").and_then(Value::as_str)) {
            let result = match method {
                "workspace/configuration" => {
                    let mut result = Vec::new();

                    if let Some(items) = msg
                        .get("params")
                        .and_then(|p| p.get("items"))
                        .and_then(Value::as_array)
                    {
                        for item in items {
                            let section = item
                                .get("section")
                                .and_then(Value::as_str)
                                .unwrap_or("");

                            if section == "python" {
                                result.push(json!({
                                    "analysis": {
                                        "diagnosticMode": "openFilesOnly",
                                        "typeCheckingMode": "basic",
                                        "autoSearchPaths": true,
                                        "useLibraryCodeForTypes": true
                                    }
                                }));
                            } else {
                                result.push(json!({}));
                            }
                        }
                    }

                    Value::Array(result)
                }

                // На эти запросы достаточно ответить null.
                "client/registerCapability"
                | "client/unregisterCapability"
                | "window/workDoneProgress/create"
                | "workspace/semanticTokens/refresh"
                | "workspace/inlayHint/refresh" => Value::Null,

                _ => Value::Null,
            };

            if let Some(lsp) = self.lsp.as_mut() {
                let _ = lsp.respond(id.clone(), result);
            }

            return;
        }

        // Ответ на initialize.
        if msg.get("id").and_then(Value::as_i64) == Some(1) {
            if let Some(err) = msg.get("error") {
                self.status = format!("LSP initialize error: {}", err);
                return;
            }

            if let Some(lsp) = self.lsp.as_mut() {
                let _ = lsp.notify("initialized", json!({}));

                self.lsp_ready = true;
                self.lsp_did_open();
                self.status = "LSP ready".into(); 
            }

            self.status = "LSP ready".into();
            self.lsp_ready = true;
            return;
        }

        let Some(method) = msg.get("method").and_then(Value::as_str) else {
            return;
        };

        match method {
            "textDocument/publishDiagnostics" => {
                if let Some(params) = msg.get("params") {
                    self.parse_diagnostics(params);
                    self.status = format!("LSP diagnostics: {}", self.diagnostics.len());
                }
            } 

            "window/showMessage" | "window/logMessage" => {
                if let Some(message) = msg
                    .get("params")
                    .and_then(|p| p.get("message"))
                    .and_then(Value::as_str)
                {
                    self.status = format!("LSP: {}", message);
                }
            }

            _ => {}
        }
    }

    fn did_open_params(&self) -> Option<Value> {
        let path = self.filepath.as_ref()?;
        let uri = path_to_uri(path).ok()?;
        let lang = language_id(path)?;
        let text = self.buffer.lines.join("\n");

        Some(json!({
            "textDocument": {
                "uri": uri,
                "languageId": lang,
                "version": self.lsp_version,
                "text": text
            }
        }))
    }

    fn lsp_did_open(&mut self) {
        if !self.lsp_ready {
            return;
        }

        self.lsp_version += 1;

        let Some(params) = self.did_open_params() else {
            return;
        };

        if let Some(lsp) = self.lsp.as_mut() {
            let _ = lsp.notify("textDocument/didOpen", params);
        }
    }

    // ---------- file -----------
   
    fn open_explorer(&mut self) {
        let dir = self.filepath.as_ref()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        self.load_explorer_dir(dir);
        self.show_explorer = true;
    }

    fn load_explorer_dir(&mut self, dir: PathBuf) {
        let mut entries = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                entries.push(DirEntry { name, is_dir, path: e.path() });
            }
        }
        // Директории сверху, затем по имени.
        entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then(a.name.cmp(&b.name)));
        self.explorer_dir = dir;
        self.explorer_entries = entries;
        self.explorer_cursor = 0;
        self.explorer_scroll = 0;
    }

    fn explorer_key(&mut self, key: Key) {
        match key {
            Key::Esc | Key::Char('q') => self.show_explorer = false,
            Key::Char('j') | Key::Down => {
                self.explorer_cursor = (self.explorer_cursor + 1)
                    .min(self.explorer_entries.len().saturating_sub(1));
            }
            Key::Char('k') | Key::Up => {
                self.explorer_cursor = self.explorer_cursor.saturating_sub(1);
            }
            Key::Char('-') => {
                if let Some(parent) = self.explorer_dir.parent().map(|p| p.to_path_buf()) {
                    self.load_explorer_dir(parent);
                }
            }
            Key::Enter => {
                if let Some(entry) = self.explorer_entries.get(self.explorer_cursor) {
                    let path = entry.path.clone();
                    if entry.is_dir {
                        self.load_explorer_dir(path);
                    } else {
                        self.show_explorer = false;
                        if let Err(e) = self.open_file(path) {
                            self.status = format!("open error: {}", e);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn render_explorer(&mut self, screen: &mut Screen, text_rows: usize) -> io::Result<()> {
        if self.explorer_cursor < self.explorer_scroll {
            self.explorer_scroll = self.explorer_cursor;
        }
        if self.explorer_cursor >= self.explorer_scroll + text_rows {
            self.explorer_scroll = self.explorer_cursor - text_rows + 1;
        }

        for i in 0..text_rows {
            let idx = self.explorer_scroll + i;
            let row = (i + 1) as u16;
            if idx >= self.explorer_entries.len() {
                screen.draw_line(row, &[("", "")])?;
                continue;
            }
            let entry = &self.explorer_entries[idx];
            let icon = if entry.is_dir { " " } else { " " };
            let text = format!("  {} {}", icon, entry.name);

            let style = if idx == self.explorer_cursor {
                format!("{}{}", theme::BG_SURFACE2, theme::FG_TEXT)
            } else if entry.is_dir {
                theme::FG_BLUE.to_string()
            } else {
                theme::FG_TEXT.to_string()
            };

            screen.draw_line(row, &[(&text, &style)])?;
        }
        Ok(())
    }

    /// Забрать текущий буфер, оставив на его месте пустой.
    fn take_current(&mut self) -> ParkedBuffer {
        ParkedBuffer {
            text: std::mem::replace(&mut self.buffer, TextBuffer::new()),
            filepath: self.filepath.take(),
            dirty: std::mem::replace(&mut self.dirty, false),
            scroll_row: std::mem::replace(&mut self.scroll_row, 0),
            undo_stack: std::mem::take(&mut self.undo_stack),
        }
    }

    /// Сделать припаркованный буфер активным.
    fn activate(&mut self, p: ParkedBuffer) {
        self.buffer = p.text;
        self.filepath = p.filepath;
        self.dirty = p.dirty;
        self.scroll_row = p.scroll_row;
        self.undo_stack = p.undo_stack;
        self.anchor = None;
        self.mode = Mode::Normal;
        self.pending = None;
        self.diagnostics.clear();
        self.lsp_diagnostics = 0;
        self.refresh_lsp_for_buffer();
    }

    /// Открыть файл (или переключиться, если уже открыт).
    fn open_file(&mut self, path: PathBuf) -> io::Result<()> {
        if self.filepath.as_ref() == Some(&path) {
            return Ok(());
        }

        // Уже открыт среди припаркованных?
        if let Some(idx) = self.parked.iter().position(|p| p.filepath.as_ref() == Some(&path)) {
            let target = self.parked.remove(idx);
            let current = self.take_current();
            self.parked.push(current);
            self.activate(target);
            return Ok(());
        }

        // Новый файл: сначала грузим, потом паркуем текущий (чтобы не потерять при ошибке).
        let text = if path.exists() { TextBuffer::load(&path)? } else { TextBuffer::new() };
        let current = self.take_current();
        self.parked.push(current);
        self.buffer = text;
        self.filepath = Some(path);
        self.dirty = false;
        self.scroll_row = 0;
        self.undo_stack.clear();
        self.anchor = None;
        self.mode = Mode::Normal;
        self.pending = None;
        self.diagnostics.clear();
        self.lsp_diagnostics = 0;
        self.refresh_lsp_for_buffer();
        Ok(())
    }

    /// Кольцевое переключение буферов.
    fn cycle_buffer(&mut self, forward: bool) {
        if self.parked.is_empty() { return; }
        let current = self.take_current();
        let target = if forward {
            self.parked.push(current);
            self.parked.remove(0)
        } else {
            self.parked.insert(0, current);
            self.parked.pop().unwrap()
        };
        self.activate(target);
    }

    /// Закрыть текущий буфер.
    fn close_buffer(&mut self, force: bool) {
        if self.dirty && !force {
            self.status = "No write since last change (:bd! to force)".into();
            return;
        }
        if self.parked.is_empty() {
            self.buffer = TextBuffer::new();
            self.filepath = None;
            self.dirty = false;
            self.scroll_row = 0;
            self.undo_stack.clear();
        } else {
            let target = self.parked.remove(0);
            self.activate(target);
        }
        self.anchor = None;
        self.mode = Mode::Normal;
    }

    /// Список буферов в статус-строку.
    fn list_buffers(&mut self) {
        let name = |p: &Option<PathBuf>| {
            p.as_ref()
                .map(|p| p.file_name().unwrap_or_default().to_string_lossy().into_owned())
                .unwrap_or_else(|| "[No Name]".into())
        };
        let mut parts = vec![format!("[{}]", name(&self.filepath))];
        for p in &self.parked {
            parts.push(name(&p.filepath));
        }
        self.status = parts.join("  ");
    }
    
    // ---------- helper ----------
    /// Сколько клеток терминала занимает текст до char_idx.
    /// Таб = 4 пробела, кириллица/emoji считаются по реальной ширине.
    fn display_col(&self, line: &str, char_idx: usize) -> usize {
        line.chars()
            .take(char_idx)
            .map(|c| if c == '\t' { self.config.editor.tab_width } else { c.width().unwrap_or(0) })
            .sum()
    }

    // ---------- undo ----------

    fn snapshot(&mut self) {
        self.undo_stack.push(self.buffer.clone());
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
    }

    // ---------- файловые операции ----------

    fn save(&mut self) {
        // Клонируем путь, чтобы не держать заимствование self.filepath
        // при мутации self.dirty/self.status — borrow checker иначе заругается.
        let path = match self.filepath.clone() {
            Some(p) => p,
            None => {
                self.status = "No file name — use :w <path>".into();
                return;
            }
        };
        match self.buffer.save(&path) {
            Ok(()) => {
                self.dirty = false;
                self.status = format!("Saved: {}", path.display());
            }
            Err(e) => self.status = format!("Save error: {}", e),
        }
    }

    fn try_quit(&mut self, force: bool) {
        if self.dirty && !force {
            self.status = "No write since last change — :q! force, :wq save+quit".into();
        } else {
            self.quit = true;
        }
    }

    // ---------- диспетчер клавиш ----------

    pub fn handle_key(&mut self, key: Key) {
        if self.command.is_some() {
            self.command_key(key);
            return;
        }
        match key {
            Key::Ctrl('s') => return self.save(),
            Key::Ctrl('q') => return self.try_quit(false),
            _ => {}
        }

        if self.show_help {
            self.help_key(key);
            return;
        }

        if self.show_explorer {
            self.explorer_key(key);
            return;
        }

        match self.mode {
            Mode::Normal => self.normal_key(key),
            Mode::Insert => self.insert_key(key),
            Mode::Visual => self.visual_key(key),
        }
    }

    fn enter_insert(&mut self) {
        self.snapshot(); // один снапшот на всю insert-сессию — u откатит её целиком
        self.mode = Mode::Insert;
    }

    // ---------- NORMAL ----------

    fn normal_key(&mut self, key: Key) {
        // Двухклавишные команды: gg, dd, yy
        if let Some(p) = self.pending.take() {
            match (p, &key) {
                ('g', Key::Char('g')) => {
                    self.buffer.row = 0;
                    self.buffer.col = 0;
                }
                ('d', Key::Char('d')) => {
                    self.snapshot();
                    self.clipboard = format!("{}\n", self.buffer.delete_line());
                    self.dirty = true;
                    self.lsp_did_change();
                }
                ('y', Key::Char('y')) => {
                    self.clipboard = format!("{}\n", self.buffer.lines[self.buffer.row]);
                    self.status = "1 line yanked".into();
                } 
                _ => {}
            }
            return;
        }

        match key {
            Key::Char('h') | Key::Left => self.buffer.move_cursor(0, -1, false),
            Key::Char('j') | Key::Down => self.buffer.move_cursor(1, 0, false),
            Key::Char('k') | Key::Up => self.buffer.move_cursor(-1, 0, false),
            Key::Char('l') | Key::Right => self.buffer.move_cursor(0, 1, false),
            Key::PageUp => self.buffer.move_cursor(-(self.view_height as i32), 0, false),
            Key::PageDown => self.buffer.move_cursor(self.view_height as i32, 0, false),
            Key::Char('0') | Key::Home => self.buffer.col = 0,
            Key::Char('$') | Key::End => {
                self.buffer.col = self.buffer.line_len(self.buffer.row).saturating_sub(1);
            }
            Key::Char('?') => {
                self.show_help = true;
                self.help_scroll = 0;
                self.status = "Help: j/k scroll, q/Esc close".into();
            }
            Key::Char('G') => {
                self.buffer.row = self.buffer.lines.len() - 1;
                self.buffer.clamp_col_normal();
            }
            Key::Char(c) if matches!(c, 'g' | 'd' | 'y') => self.pending = Some(c),

            Key::Char('i') => self.enter_insert(),
            Key::Char('a') => {
                self.buffer.col = (self.buffer.col + 1).min(self.buffer.line_len(self.buffer.row));
                self.enter_insert();
            }
            Key::Char('A') => {
                self.buffer.col = self.buffer.line_len(self.buffer.row);
                self.enter_insert();
            }
            Key::Char('I') => {
                self.buffer.col = 0;
                self.enter_insert();
            }

            Key::Char('o') => {
                self.snapshot();
                self.buffer.col = self.buffer.line_len(self.buffer.row);
                self.buffer.insert_newline();
                self.dirty = true;
                self.mode = Mode::Insert;
            }
            Key::Char('O') => {
                self.snapshot();
                self.buffer.col = 0;
                self.buffer.insert_newline();
                self.buffer.row -= 1; // курсор на новую пустую строку сверху
                self.dirty = true;
                self.mode = Mode::Insert;
            }
            Key::Char('x') => {
                self.snapshot();
                self.buffer.delete_char();
                self.buffer.clamp_col_normal();
                self.dirty = true;
                self.lsp_did_change();
            }
            Key::Char('p') => {
                if self.clipboard.is_empty() {
                    return;
                }
                self.snapshot();
                if self.clipboard.contains('\n') {
                    // построчная вставка
                    let text = self.clipboard.trim_end_matches('\n');
                    for (i, l) in text.split('\n').enumerate() {
                        self.buffer.lines.insert(self.buffer.row + 1 + i, l.to_string());
                    }
                    self.buffer.row += 1;
                    self.buffer.col = 0;
                } else {
                    for c in self.clipboard.chars() {
                        self.buffer.insert_char(c);
                    }
                }
                self.dirty = true;
                self.lsp_did_change();
            }
            Key::Char('u') => {
                if let Some(prev) = self.undo_stack.pop() {
                    self.buffer = prev;
                    self.dirty = true;
                    self.status = "undo".into();
                } else {
                    self.status = "Already at oldest change".into();
                }
                self.lsp_did_change();
            }
            Key::Char('v') => {
                self.mode = Mode::Visual;
                self.anchor = Some((self.buffer.row, self.buffer.col));
            }
            Key::Char(':') => self.command = Some(String::new()),
            Key::Char('w') => self.buffer.next_word_start(),
            Key::Char('b') => self.buffer.prev_word_start(),
            Key::Char('e') => self.buffer.word_end(),

            _ => {}
        }
    }

    // ---------- INSERT ----------

    fn insert_key(&mut self, key: Key) {
        match key {
            Key::Esc => {
                self.mode = Mode::Normal;
                self.buffer.clamp_col_normal();
            }
            Key::Char('\t') => {
                // soft tabs: вставляем пробелы — проще и предсказуемее
                for _ in 0..self.config.editor.tab_width {
                    self.buffer.insert_char(' ');
                }
                self.dirty = true;
            }
            Key::Char(c) => {
                self.buffer.insert_char(c);
                self.dirty = true;
            }
            Key::Enter => {
                self.buffer.insert_newline();
                self.dirty = true;
            }
            Key::Backspace => {
                self.buffer.backspace();
                self.dirty = true;
            }
            Key::Delete => {
                self.buffer.delete_char();
                self.buffer.clamp_col_insert();
                self.dirty = true;
            }
            Key::Left => self.buffer.move_cursor(0, -1, true),
            Key::Right => self.buffer.move_cursor(0, 1, true),
            Key::Up => self.buffer.move_cursor(-1, 0, true),
            Key::Down => self.buffer.move_cursor(1, 0, true),
            Key::Home => self.buffer.col = 0,
            Key::End => self.buffer.col = self.buffer.line_len(self.buffer.row),
            _ => {}
        }
    }

    // ---------- VISUAL ----------

    fn visual_key(&mut self, key: Key) {
        match key {
            Key::Esc => {
                self.mode = Mode::Normal;
                self.anchor = None;
            }
            Key::Char('w') => self.buffer.next_word_start(),
            Key::Char('b') => self.buffer.prev_word_start(),
            Key::Char('e') => self.buffer.word_end(),

            Key::Char('h') | Key::Left => self.buffer.move_cursor(0, -1, false),
            Key::Char('j') | Key::Down => self.buffer.move_cursor(1, 0, false),
            Key::Char('k') | Key::Up => self.buffer.move_cursor(-1, 0, false),
            Key::Char('l') | Key::Right => self.buffer.move_cursor(0, 1, false),
            Key::Char('0') => self.buffer.col = 0,
            Key::Char('$') => {
                self.buffer.col = self.buffer.line_len(self.buffer.row).saturating_sub(1);
            }
            Key::Char('G') => {
                self.buffer.row = self.buffer.lines.len() - 1;
                self.buffer.clamp_col_normal();
            }
            Key::Char('d') | Key::Char('x') => {
                if let Some(text) = self.delete_selection() {
                    self.clipboard = text;
                    self.dirty = true;
                }
                self.mode = Mode::Normal;
                self.anchor = None;
            }
            Key::Char('y') => {
                if let Some(text) = self.yank_selection() {
                    self.clipboard = text;
                    self.status = "yanked".into();
                }
                self.mode = Mode::Normal;
                self.anchor = None;
            }
            _ => {}
        }
    }

    /// Нормализованное выделение: (начало, конец), конец включительно.
    fn selection(&self) -> Option<((usize, usize), (usize, usize))> {
        let a = self.anchor?;
        let c = (self.buffer.row, self.buffer.col);
        Some(if a <= c { (a, c) } else { (c, a) })
    }

    fn yank_selection(&self) -> Option<String> {
        let ((r1, c1), (r2, c2)) = self.selection()?;
        let lines = &self.buffer.lines;
        if r1 == r2 {
            let line = &lines[r1];
            let start = TextBuffer::byte_index(line, c1);
            let end = TextBuffer::byte_index(line, c2 + 1);
            Some(line[start..end].to_string())
        } else {
            let mut out = String::new();
            let first = &lines[r1];
            out.push_str(&first[TextBuffer::byte_index(first, c1)..]);
            for r in (r1 + 1)..r2 {
                out.push('\n');
                out.push_str(&lines[r]);
            }
            out.push('\n');
            let last = &lines[r2];
            out.push_str(&last[..TextBuffer::byte_index(last, c2 + 1)]);
            Some(out)
        }
    }

    fn delete_selection(&mut self) -> Option<String> {
        let ((r1, c1), (r2, c2)) = self.selection()?;
        self.snapshot();
        let text;
        if r1 == r2 {
            let line = &mut self.buffer.lines[r1];
            let start = TextBuffer::byte_index(line, c1);
            let end = TextBuffer::byte_index(line, c2 + 1);
            text = line[start..end].to_string();
            line.replace_range(start..end, "");
        } else {
            // собираем удаляемый текст (он же уйдёт в clipboard)
            let mut out = String::new();
            let first = &self.buffer.lines[r1];
            out.push_str(&first[TextBuffer::byte_index(first, c1)..]);
            for r in (r1 + 1)..r2 {
                out.push('\n');
                out.push_str(&self.buffer.lines[r]);
            }
            out.push('\n');
            let last = &self.buffer.lines[r2];
            out.push_str(&last[..TextBuffer::byte_index(last, c2 + 1)]);
            text = out;

            // склеиваем хвост последней строки с началом первой
            let prefix = self.buffer.lines[r1]
                [..TextBuffer::byte_index(&self.buffer.lines[r1], c1)]
                .to_string();
            let suffix = self.buffer.lines[r2]
                [TextBuffer::byte_index(&self.buffer.lines[r2], c2 + 1)..]
                .to_string();
            self.buffer.lines[r1] = prefix + &suffix;
            self.buffer.lines.drain(r1 + 1..=r2);
        }
        self.buffer.row = r1;
        self.buffer.col = c1;
        self.buffer.clamp_col_normal();
        Some(text)
    }

    // ---------- командная строка ----------

    fn command_key(&mut self, key: Key) {
        match key {
            Key::Esc => self.command = None,
            Key::Enter => {
                if let Some(cmd) = self.command.take() {
                    self.execute_command(&cmd);
                }
            }
            Key::Backspace => {
                if let Some(cmd) = &mut self.command {
                    cmd.pop();
                    if cmd.is_empty() {
                        self.command = None;
                    }
                }
            }
            Key::Char(c) => {
                if let Some(cmd) = &mut self.command {
                    cmd.push(c);
                }
            }
            _ => {}
        }
    }

    fn execute_command(&mut self, cmd: &str) {
        let cmd = cmd.trim();
        let (name, arg) = match cmd.split_once(' ') {
            Some((n, a)) => (n, Some(a.trim())),
            None => (cmd, None),
        };
        match name {
            "e" => {
                if let Some(path) = arg {
                    if let Err(e) = self.open_file(PathBuf::from(path)) {
                        self.status = format!("open error: {}", e);
                    }
                } else {
                    self.status = "Usage: :e <path>".into();
                }
            }

            "lsp" => {
                let lang = self
                    .filepath
                    .as_ref()
                    .and_then(|p| language_id(p))
                    .unwrap_or("none");

                self.status = format!(
                    "LSP: {} | lang: {} | ready: {} | diag: {} | ver: {}",
                    if self.lsp.is_some() { "running" } else { "off" },
                    lang,
                    if self.lsp_ready { "yes" } else { "no" },
                    self.lsp_diagnostics,
                    self.lsp_version
                );
            } 

            "Ex" | "ex" => self.open_explorer(),
            "ls" => self.list_buffers(),
            "bn" => self.cycle_buffer(true),
            "bp" => self.cycle_buffer(false),
            "bd" => self.close_buffer(false),
            "bd!" => self.close_buffer(true),
            "w" => {
                if let Some(path) = arg {
                    self.filepath = Some(PathBuf::from(path));
                }
                self.save();
            }
            "q" => self.try_quit(false),
            "q!" => self.try_quit(true),
            "wq" | "x" => {
                self.save();
                if !self.dirty {
                    self.quit = true;
                }
            }
            "h" | "help" => {
                self.show_help = true;
                self.help_scroll = 0;
                self.status = "Help: j/k scroll, q/Esc close".into();
            }
            _ => self.status = format!("Unknown command: {}", cmd),
        }
    }

    // ---------- рендер ----------
    fn render_help(&mut self, screen: &mut Screen, text_rows: usize) -> io::Result<()> {
        let total = HELP.len();
        // не даём проскроллить дальше последней строки
        let max_scroll = total.saturating_sub(text_rows);
        self.help_scroll = self.help_scroll.min(max_scroll);

        let title_badge = format!("{}{}", theme::BG_MAUVE, theme::FG_BASE);
        let header_style = format!("\x1b[1m{}", theme::FG_MAUVE); // bold + mauve

        for i in 0..text_rows {
            let idx = self.help_scroll + i;
            let row = (i + 1) as u16;
            if idx >= total {
                screen.draw_line(row, &[("", "")])?;
                continue;
            }

            // Vec<(String, String)> — владеем строками, чтобы не мучиться с временными ссылками
            let segs: Vec<(String, String)> = match &HELP[idx] {
                HelpLine::Title => vec![
                    ("  Nuta  ".into(), title_badge.clone()),
                    (" справка по командам".into(), theme::FG_LAVENDER.into()),
                ],
                HelpLine::Header(t) => vec![(format!("  {}", t), header_style.clone())],
                HelpLine::Entry(k, d) => vec![
                    (format!("    {:<18}", k), theme::FG_GREEN.into()),
                    ((*d).into(), theme::FG_TEXT.into()),
                ],
                HelpLine::Blank => vec![("".into(), "".into())],
            };

            let refs: Vec<(&str, &str)> =
                segs.iter().map(|(s, st)| (s.as_str(), st.as_str())).collect();
            screen.draw_line(row, &refs)?;
        }
        Ok(())
    }

        /// Самый серьёзный severity на строке: 1 = error, 2 = warning.
    fn line_diagnostic_severity(&self, row: usize) -> Option<u8> {
        self.diagnostics
            .iter()
            .filter(|d| d.start_line <= row && row <= d.end_line)
            .map(|d| d.severity)
            .min()
    }

    /// Первая диагностика на текущей строке курсора.
    fn current_diagnostic(&self) -> Option<&Diagnostic> {
        self.diagnostics
            .iter()
            .find(|d| d.start_line <= self.buffer.row && self.buffer.row <= d.end_line)
    }

    /// Underline поверх уже нарисованной строки.
    fn draw_diagnostics_overlay(
        &self,
        screen: &mut Screen,
        screen_row: u16,
        file_row: usize,
        line: &str,
        gutter: usize,
    ) -> io::Result<()> {
        let chars: Vec<char> = line.chars().collect();

        if self.mode == Mode::Insert {
            return Ok(());
        }

        for d in &self.diagnostics {
            if file_row < d.start_line || file_row > d.end_line {
                continue;
            }

            let start = if file_row == d.start_line { d.start_col } else { 0 };
            let end = if file_row == d.end_line { d.end_col } else { chars.len() };

            let start = start.min(chars.len());
            let mut end = end.min(chars.len());

            // Нулевая ширина: подчеркнём хотя бы один символ.
            if end <= start {
                end = (start + 1).min(chars.len());
            }

            if start >= end {
                continue;
            }

            let text: String = chars[start..end].iter().collect();
            let col = gutter + self.display_col(line, start);

            let style = match d.severity {
                1 => format!("\x1b[4m{}", theme::FG_RED),
                2 => format!("\x1b[4m{}", theme::FG_YELLOW),
                _ => format!("\x1b[4m{}", theme::FG_SUBTEXT),
            };

            screen.draw_overlay(screen_row, col as u16, &text, &style)?;
        }

        Ok(())
    }

    pub fn render(&mut self, screen: &mut Screen) -> io::Result<()> {
        let (rows, cols) = screen.size();
        if rows < 5 || cols < 20 {
            return Ok(());
        }
        screen.begin_frame()?;

        let text_rows = (rows - 3) as usize;
        self.view_height = text_rows;

        let fname = self.filepath.as_ref()
            .map(|p| p.file_name().unwrap_or_default().to_string_lossy().into_owned())
            .unwrap_or_else(|| "[No Name]".into());
        let dirty_mark = if self.dirty { " [+]" } else { "" };

        // --- верхний бар (общее) ---
        let top = format!("  nuta │ {}{}", fname, dirty_mark);
        let top_padded = format!("{:<w$}", top, w = cols as usize);
        let top_style = format!("{}{}", theme::BG_SURFACE1, theme::FG_LAVENDER);
        screen.draw_line(0, &[(&top_padded, &top_style)])?;

        let gutter = if self.config.editor.line_numbers {
            format!("{}", self.buffer.lines.len()).len() + 1
        } else {
            0
        };

        // --- текстовая область: ЕДИНСТВЕННОЕ, что различается ---
        if self.show_help {
            self.render_help(screen, text_rows)?;
        } else if self.show_explorer {
            self.render_explorer(screen, text_rows)?;
        }
        else {
            if self.buffer.row < self.scroll_row {
                self.scroll_row = self.buffer.row;
            }
            if self.buffer.row >= self.scroll_row + text_rows {
                self.scroll_row = self.buffer.row - text_rows + 1;
            }

            let sel = self.selection();
            let sel_style = format!("{}{}", theme::BG_SURFACE2, theme::FG_TEXT);

            let lang = self.highlighter.detect(self.filepath.as_deref());
            
            for i in 0..text_rows {
                let file_row = self.scroll_row + i;
                let screen_row = (i + 1) as u16;

                if file_row >= self.buffer.lines.len() {
                    screen.draw_line(screen_row, &[("~", theme::FG_OVERLAY)])?;
                    continue;
                }

                let num_style = match self.line_diagnostic_severity(file_row) {
                    Some(1) => theme::FG_RED,
                    Some(2) => theme::FG_YELLOW,
                    _ => {
                        if file_row == self.buffer.row {
                            theme::FG_LAVENDER
                        } else {
                            theme::FG_OVERLAY
                        }
                    }
                }; 

                let num = if self.config.editor.line_numbers {
                    format!("{:>w$} ", file_row + 1, w = gutter - 1)
                } else {
                    String::new()
                };

                let line = self.buffer.lines[file_row]
                    .replace('\t', &" ".repeat(self.config.editor.tab_width));

                let mut segments: Vec<(String, &str)> = vec![(num, num_style)];

                let highlighted = sel.and_then(|((r1, c1), (r2, c2))| {
                    if file_row < r1 || file_row > r2 { return None; }
                    let chars: Vec<char> = line.chars().collect();
                    let start = if file_row == r1 { c1 } else { 0 };
                    let end = if file_row == r2 { c2 + 1 } else { chars.len() };
                    let start = start.min(chars.len());
                    let end = end.max(start).min(chars.len());
                    Some((start, end, chars))
                });

                if let Some((start, end, chars)) = highlighted {
                    segments.push((chars[..start].iter().collect(), theme::FG_TEXT));
                    segments.push((chars[start..end].iter().collect(), &sel_style));
                    segments.push((chars[end..].iter().collect(), theme::FG_TEXT));
                } else if self.config.syntax.enabled {
                    for seg in self.highlighter.highlight_line(lang, &line) {
                        segments.push((seg.text, seg.style));
                    }
                }
                else {
                    segments.push((line.clone(), theme::FG_TEXT));
                }

                let segs: Vec<(&str, &str)> =
                    segments.iter().map(|(s, st)| (s.as_str(), *st)).collect();
                screen.draw_line(screen_row, &segs)?;
                self.draw_diagnostics_overlay(screen, screen_row, file_row, &line, gutter)?;
            }
        }

        // --- нижний бар (ОБЩЕЕ) ---
        let mode_bg = match self.mode {
            Mode::Normal => theme::BG_MAUVE,
            Mode::Insert => theme::BG_GREEN,
            Mode::Visual => theme::BG_PINK,
        };
        let badge = format!(" {} ", self.mode.name());
        let badge_style = format!("{}{}", mode_bg, theme::FG_BASE);

        let cur_line = &self.buffer.lines[self.buffer.row];
        let col_display = self.display_col(cur_line, self.buffer.col) + 1;
        let rest = format!(
            " {}{} │ Ln {}, Col {} ",
            fname, dirty_mark, self.buffer.row + 1, col_display
        );
        let rest_padded = format!(
            "{:<w$}", rest,
            w = (cols as usize).saturating_sub(badge.chars().count())
        );
        let rest_style = format!("{}{}", theme::BG_SURFACE0, theme::FG_TEXT);
        screen.draw_line(rows - 2, &[(&badge, &badge_style), (&rest_padded, &rest_style)])?;

        // --- командная строка (ОБЩЕЕ) ---
        let cmd_cursor = match &self.command {
            Some(cmd) => {
                let text = format!(":{}", cmd);
                screen.draw_line(rows - 1, &[(&text, theme::FG_TEXT)])?;
                Some((rows - 1, 1 + cmd.width() as u16))
            }
            None => {
                if self.mode != Mode::Insert {
                    if let Some(d) = self.current_diagnostic() {
                        let style = if d.severity == 1 {
                            theme::FG_RED
                        } else {
                            theme::FG_YELLOW
                        };
                        screen.draw_line(rows - 1, &[(&d.message, style)])?;
                    } else {
                        screen.draw_line(rows - 1, &[(&self.status, theme::FG_SUBTEXT)])?;
                    }
            } else {
                screen.draw_line(rows - 1, &[(&self.status, theme::FG_SUBTEXT)])?;
            }
        None
            }
        };

        // --- курсор (ОБЩЕЕ, но позиция зависит от help) ---
        let (cr, cc) = if self.show_help || self.show_explorer {
            (rows - 1, 0) // в help уводим курсор вниз, чтобы не торчал поверх текста
        } else {
            cmd_cursor.unwrap_or((
                (self.buffer.row - self.scroll_row + 1) as u16,
                (gutter + self.display_col(cur_line, self.buffer.col)) as u16,
            ))
        };

        let shape = if self.command.is_some() {
            CursorShape::Bar
        } else {
            match self.mode {
                Mode::Normal => CursorShape::Block,
                Mode::Insert => CursorShape::Bar,
                Mode::Visual => CursorShape::Underline,
            }
        };

        screen.end_frame(cr, cc.min(cols - 1), shape) // ← общее, с flush, для обоих режимов
    } 
}
