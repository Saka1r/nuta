use libc::{poll, pollfd, POLLIN, STDIN_FILENO};
use std::io::{self, Read};

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

pub struct Input {
    stdin: io::Stdin,
}

impl Input {
    pub fn new() -> Self {
        Self { stdin: io::stdin() }
    }

    fn read_byte(&mut self) -> io::Result<u8> {
        let mut b = [0u8; 1];
        loop {
            match self.stdin.read_exact(&mut b) {
                Ok(()) => return Ok(b[0]),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
    }

    /// Ждём байт не дольше timeout_ms. None = таймаут (байтов нет).
    /// Это и есть правильное решение проблемы Esc из Недели 1.
    fn poll_byte(&mut self, timeout_ms: i32) -> io::Result<Option<u8>> {
        let mut pfd = pollfd { fd: STDIN_FILENO, events: POLLIN, revents: 0 };
        let n = unsafe { poll(&mut pfd, 1, timeout_ms) };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        if n == 0 {
            Ok(None)
        } else {
            self.read_byte().map(Some)
        }
    }

    pub fn read_key(&mut self) -> io::Result<Key> {
        let b = self.read_byte()?;
        match b {
            27 => self.read_escape(),
            13 | 10 => Ok(Key::Enter),
            127 | 8 => Ok(Key::Backspace),
            9 => Ok(Key::Char('\t')),
            0 => Ok(Key::Ctrl(' ')),
            // Ctrl+A..Ctrl+Z приходят как байты 1..26 (минус уже обработанные)
            1..=26 => Ok(Key::Ctrl((b - 1 + b'a') as char)),
            _ => self.read_utf8(b),
        }
    }

    /// UTF-8: по первому байту определяем длину символа и дочитываем остальные.
    /// Без этого кириллица (2 байта на символ) приходила бы как мусор.
    fn read_utf8(&mut self, first: u8) -> io::Result<Key> {
        let len = if first < 0x80 {
            1
        } else if first >> 5 == 0b110 {
            2
        } else if first >> 4 == 0b1110 {
            3
        } else if first >> 3 == 0b11110 {
            4
        } else {
            return Ok(Key::Unknown(vec![first]));
        };
        let mut bytes = vec![first];
        for _ in 1..len {
            bytes.push(self.read_byte()?);
        }
        match String::from_utf8(bytes) {
            Ok(s) => Ok(Key::Char(s.chars().next().unwrap())),
            Err(e) => Ok(Key::Unknown(e.into_bytes())),
        }
    }

    fn read_escape(&mut self) -> io::Result<Key> {
        // Если после ESC за 50мс ничего не пришло — это просто Esc.
        // Стрелки приходят как ESC [ A одним пакетом, поэтому успевают.
        let Some(b1) = self.poll_byte(50)? else {
            return Ok(Key::Esc);
        };
        match b1 {
            b'[' | b'O' => {
                let Some(b2) = self.poll_byte(50)? else {
                    return Ok(Key::Esc);
                };
                match b2 {
                    b'A' => Ok(Key::Up),
                    b'B' => Ok(Key::Down),
                    b'C' => Ok(Key::Right),
                    b'D' => Ok(Key::Left),
                    b'H' => Ok(Key::Home),
                    b'F' => Ok(Key::End),
                    b'0'..=b'9' => {
                        // Последовательности вида ESC [ 3 ~ (Delete, PgUp, ...)
                        let mut seq = vec![b2];
                        while let Some(b) = self.poll_byte(50)? {
                            seq.push(b);
                            if b == b'~' {
                                break;
                            }
                        }
                        match seq.as_slice() {
                            [b'3', b'~'] => Ok(Key::Delete),
                            [b'5', b'~'] => Ok(Key::PageUp),
                            [b'6', b'~'] => Ok(Key::PageDown),
                            [b'1', b'~'] | [b'7', b'~'] => Ok(Key::Home),
                            [b'4', b'~'] | [b'8', b'~'] => Ok(Key::End),
                            _ => {
                                let mut full = vec![27, b'['];
                                full.extend(seq);
                                Ok(Key::Unknown(full))
                            }
                        }
                    }
                    _ => Ok(Key::Unknown(vec![27, b'[', b2])),
                }
            }
            _ => Ok(Key::Unknown(vec![27, b1])),
        }
    }
}
