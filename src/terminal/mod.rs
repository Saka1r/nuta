// src/terminal/mod.rs

// 1. Определяем Key здесь, чтобы он был доступен и редактору, и бэкенду
#[derive(Debug, Clone)]
pub enum Key {
    Char(char),
    Ctrl(char),
    Enter,
    Esc,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Unknown(Vec<u8>),
}

// 2. Форма курсора тоже здесь
#[derive(Clone, Copy)]
pub enum CursorShape {
    Block,
    Bar,
    Underline,
}

impl CursorShape {
    pub fn ansi(self) -> &'static str {
        match self {
            CursorShape::Block => "\x1b[2 q",
            CursorShape::Bar => "\x1b[6 q",
            CursorShape::Underline => "\x1b[4 q",
        }
    }
}

// 3. Экспортируем бэкенд
mod backend;
pub use backend::{Input, RawModeGuard, Screen};
