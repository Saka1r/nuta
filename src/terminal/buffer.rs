use libc::{ioctl, winsize, STDOUT_FILENO, TIOCGWINSZ};
use std::io::{self, Write};

#[derive(Clone, Copy)]
pub enum CursorShape {
    Block,     // Normal
    Bar,       // Insert
    Underline, // Visual
}

impl CursorShape {
    fn ansi(self) -> &'static str {
        match self {
            CursorShape::Block => "\x1b[2 q",
            CursorShape::Bar => "\x1b[6 q",
            CursorShape::Underline => "\x1b[4 q",
        }
    }
}
 
pub struct Screen {
    rows: u16,
    cols: u16,
    out: io::Stdout,
}

impl Screen {
    pub fn new() -> io::Result<Self> {
        let mut screen = Self { rows: 0, cols: 0, out: io::stdout() };
        screen.refresh_size()?;
        write!(screen.out, "\x1b[?1049h\x1b[?25l")?;
        screen.out.flush()?;
        Ok(screen)
    }

    /// Накладывает текст поверх уже нарисованной строки, не стирая остальное.
    /// Нужно для underline диагностики поверх подсветки синтаксиса.
    pub fn draw_overlay(&mut self, row: u16, col: u16, text: &str, style: &str) -> io::Result<()> {
        if col >= self.cols {
            return Ok(());
        }

        let max = (self.cols - col) as usize;
        let visible: String = text.chars().take(max).collect();

        if visible.is_empty() {
            return Ok(());
        }

        write!(
            self.out,
            "\x1b[{};{}H{}{}\x1b[0m",
            row + 1,
            col + 1,
            style,
            visible
        )
    }

    pub fn refresh_size(&mut self) -> io::Result<()> {
        let mut ws: winsize = unsafe { std::mem::zeroed() };
        if unsafe { ioctl(STDOUT_FILENO, TIOCGWINSZ, &mut ws) } != 0 {
            return Err(io::Error::last_os_error());
        }
        self.rows = ws.ws_row;
        self.cols = ws.ws_col;
        Ok(())
    }

    pub fn size(&self) -> (u16, u16) {
        (self.rows, self.cols)
    }

    pub fn begin_frame(&mut self) -> io::Result<()> {
        self.refresh_size()?;
        //write!(self.out, "\x1b[H\x1b[?25l")
        write!(self.out, "\x1b[?2026h\x1b[H\x1b[?25l")
    }

    pub fn draw_line(&mut self, row: u16, segments: &[(&str, &str)]) -> io::Result<()> {
        write!(self.out, "\x1b[{};1H", row + 1)?;
        let mut remaining = self.cols as usize;
        for (text, style) in segments {
            if remaining == 0 {
                break;
            }
            let visible: String = text.chars().take(remaining).collect();
            remaining -= visible.chars().count();
            if style.is_empty() {
                write!(self.out, "{}", visible)?;
            } else {
                write!(self.out, "{}{}\x1b[0m", style, visible)?;
            }
        }
        write!(self.out, "\x1b[K")
    }

    /// Конец кадра: позиция + ФОРМА + показать курсор.
    pub fn end_frame(&mut self, row: u16, col: u16, shape: CursorShape) -> io::Result<()> {
        write!(self.out, "\x1b[{};{}H", row + 1, col + 1)?;
        write!(self.out, "{}", shape.ansi())?; //
        //write!(self.out, "\x1b[?25h")?;
        write!(self.out, "\x1b[?2026l\x1b[?25h")?;
        self.out.flush()
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        // \x1b[0 q — вернуть курсор терминала по умолчанию,
        // иначе после выхода из редактора в шелле останется "палочка".
        let _ = write!(self.out, "\x1b[0 q\x1b[?25h\x1b[?1049l");
        let _ = self.out.flush();
    }
}
