use crate::terminal::{CursorShape, Key};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
    QueueableCommand,
};
use std::io::{self, Write};

// ---------- RAW MODE ----------

pub struct RawModeGuard;

impl RawModeGuard {
    pub fn new() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

// ---------- SCREEN ----------

pub struct Screen {
    stdout: io::Stdout,
}

impl Screen {
    pub fn new() -> io::Result<Self> {
        let mut stdout = io::stdout();
        stdout.queue(EnterAlternateScreen)?;
        stdout.queue(cursor::Hide)?;
        stdout.flush()?;
        Ok(Self { stdout })
    }

    /// ВАЖНО: возвращаем (rows, cols) — как в старом Unix-коде.
    /// crossterm отдаёт (cols, rows), поэтому меняем местами.
    pub fn size(&self) -> (u16, u16) {
        let (cols, rows) = terminal::size().unwrap_or((80, 24));
        (rows, cols)
    }

    pub fn begin_frame(&mut self) -> io::Result<()> {
        self.stdout.queue(cursor::MoveTo(0, 0))?;
        self.stdout.queue(cursor::Hide)?;
        Ok(())
    }

    pub fn draw_line(&mut self, row: u16, segments: &[(&str, &str)]) -> io::Result<()> {
        self.stdout.queue(cursor::MoveTo(0, row))?;

        let (_, cols) = self.size();
        let mut remaining = cols as usize;

        for (text, style) in segments {
            if remaining == 0 {
                break;
            }

            let visible: String = text.chars().take(remaining).collect();
            remaining = remaining.saturating_sub(visible.chars().count());

            // Печатаем твои готовые ANSI-строки как есть.
            if style.is_empty() {
                write!(self.stdout, "{}", visible)?;
            } else {
                write!(self.stdout, "{}{}\x1b[0m", style, visible)?;
            }
        }

        self.stdout.queue(Clear(ClearType::UntilNewLine))?;
        Ok(())
    }

    pub fn draw_overlay(&mut self, row: u16, col: u16, text: &str, style: &str) -> io::Result<()> {
        let (_, cols) = self.size();
        if col >= cols {
            return Ok(());
        }

        let max = (cols - col) as usize;
        let visible: String = text.chars().take(max).collect();
        if visible.is_empty() {
            return Ok(());
        }

        self.stdout.queue(cursor::MoveTo(col, row))?;
        write!(self.stdout, "{}{}\x1b[0m", style, visible)?;
        Ok(())
    }

    pub fn end_frame(&mut self, row: u16, col: u16, shape: CursorShape) -> io::Result<()> {
        self.stdout.queue(cursor::MoveTo(col, row))?;
        write!(self.stdout, "{}", shape.ansi())?;
        self.stdout.queue(cursor::Show)?;
        self.stdout.flush()
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let _ = write!(self.stdout, "\x1b[0 q");
        let _ = self.stdout.queue(cursor::Show);
        let _ = self.stdout.queue(LeaveAlternateScreen);
        let _ = self.stdout.flush();
    }
}

// ---------- INPUT ----------

pub struct Input;

impl Input {
    pub fn new() -> Self {
        Self
    }

    pub fn read_key(&mut self) -> io::Result<Key> {
        loop {
            let ev = event::read()?;

            let key_event = match ev {
                Event::Key(k) => k,
                // Resize / Mouse / Focus / Paste — не клавиши, ждём дальше.
                _ => continue,
            };

            // Windows шлёт Press И Release. Без этого фильтра каждый
            // символ вводился бы дважды.
            if key_event.kind == KeyEventKind::Release {
                continue;
            }

            if key_event.modifiers.contains(KeyModifiers::CONTROL) {
                if let KeyCode::Char(c) = key_event.code {
                    return Ok(Key::Ctrl(c));
                }
            }

            return Ok(match key_event.code {
                KeyCode::Char(c) => Key::Char(c),
                KeyCode::Tab => Key::Char('\t'),
                KeyCode::Enter => Key::Enter,
                KeyCode::Backspace => Key::Backspace,
                KeyCode::Delete => Key::Delete,
                KeyCode::Esc => Key::Esc,
                KeyCode::Up => Key::Up,
                KeyCode::Down => Key::Down,
                KeyCode::Left => Key::Left,
                KeyCode::Right => Key::Right,
                KeyCode::Home => Key::Home,
                KeyCode::End => Key::End,
                KeyCode::PageUp => Key::PageUp,
                KeyCode::PageDown => Key::PageDown,
                _ => Key::Unknown(vec![]),
            });
        }
    }
}
