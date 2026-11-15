use std::fs;
use std::io;
use std::path::Path;

/// Ключевое решение: col — это индекс СИМВОЛА, а не байта.
/// "привет" — 6 символов, но 12 байт. Курсор должен считаться по символам,
/// иначе кириллица сломает и навигацию, и рендер.
#[derive(Clone)]
pub struct TextBuffer {
    pub lines: Vec<String>,
    pub row: usize,
    pub col: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum CharClass {
    Space,
    Word,
    Punct,
}

fn class_of(c: char) -> CharClass {
    if c.is_whitespace() {
        CharClass::Space
    } else if c.is_alphanumeric() || c == '_' {
        CharClass::Word
    } else {
        CharClass::Punct
    }
}

impl TextBuffer {
    pub fn new() -> Self {
        // Инвариант: в буфере всегда хотя бы одна строка.
        Self { lines: vec![String::new()], row: 0, col: 0 }
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        let content = fs::read_to_string(path)?;
        let mut lines: Vec<String> = content.lines().map(String::from).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        Ok(Self { lines, row: 0, col: 0 })
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let mut content = self.lines.join("\n");
        content.push('\n');
        fs::write(path, content)
    }

    pub fn line_len(&self, row: usize) -> usize {
        self.lines.get(row).map(|l| l.chars().count()).unwrap_or(0)
    }

    // Символ на позиции. None, если строки/символа нет.
    fn char_at(&self, row: usize, col: usize) -> Option<char> {
        self.lines.get(row)?.chars().nth(col)
    }

    fn class_at(&self, row: usize, col: usize) -> CharClass {
        match self.char_at(row, col) {
            Some(c) => class_of(c),
            None => CharClass::Space,
        }
    }

    // Следующая позиция (с переходом на другую строку). None = конец файла.
    fn next_pos(&self, row: usize, col: usize) -> Option<(usize, usize)> {
        if col + 1 < self.line_len(row) {
            Some((row, col + 1))
        } else if row + 1 < self.lines.len() {
            Some((row + 1, 0))
        } else {
            None
        }
    }

    // Предыдущая позиция. None = начало файла.
    fn prev_pos(&self, row: usize, col: usize) -> Option<(usize, usize)> {
        if col > 0 {
            Some((row, col - 1))
        } else if row > 0 {
            Some((row - 1, self.line_len(row - 1).saturating_sub(1)))
        } else {
            None
        }
    }

    // Поставить курсор и ограничить его нормальным режимом.
    fn set_cursor_normal(&mut self, row: usize, col: usize) {
        self.row = row;
        self.col = col;
        self.clamp_col_normal();
    }

    pub fn next_word_start(&mut self){
        //bind w
        let (mut r, mut c) = (self.row, self.col);
        let cls = self.class_at(r, c);

        // пропускаем текущее слово (в пределах строки)
        if cls != CharClass::Space {
            while r == self.row && self.class_at(r, c) == cls {
                match self.next_pos(r, c) {
                    Some((nr, nc)) => { r = nr; c = nc; }
                    None => return self.set_cursor_normal(r, c),
                }
            }
        }
        // пропускаем разделители
        while self.class_at(r, c) == CharClass::Space {
            match self.next_pos(r, c) {
                Some((nr, nc)) => { r = nr; c = nc; }
                None => return self.set_cursor_normal(r, c),
            }
        }
        self.set_cursor_normal(r, c);
    }

    pub fn prev_word_start(&mut self){
        //bind b
        let (mut r, mut c) = (self.row, self.col);

        // шаг назад
        match self.prev_pos(r, c) {
            Some((nr, nc)) => { r = nr; c = nc; }
            None => return,
        }
        // пропускаем разделители назад
        while self.class_at(r, c) == CharClass::Space {
            match self.prev_pos(r, c) {
                Some((nr, nc)) => { r = nr; c = nc; }
                None => return self.set_cursor_normal(0, 0),
            }
        }
        // пропускаем слово назад, останавливаемся на его начале
        let cls = self.class_at(r, c);
        let row = r;
        loop {
            match self.prev_pos(r, c) {
                Some((nr, nc)) if nr == row && self.class_at(nr, nc) == cls => {
                    r = nr; c = nc;
                }
                _ => break,
            }
        }
        self.set_cursor_normal(r, c);
    }

    pub fn word_end(&mut self){
        //bind e
        let (mut r, mut c) = (self.row, self.col);

        // шаг вперёд
        match self.next_pos(r, c) {
            Some((nr, nc)) => { r = nr; c = nc; }
            None => return,
        }
        // пропускаем разделители
        while self.class_at(r, c) == CharClass::Space {
            match self.next_pos(r, c) {
                Some((nr, nc)) => { r = nr; c = nc; }
                None => return self.set_cursor_normal(r, c),
            }
        }
        // идём до последнего символа слова
        let cls = self.class_at(r, c);
        let row = r;
        loop {
            match self.next_pos(r, c) {
                Some((nr, nc)) if nr == row && self.class_at(nr, nc) == cls => {
                    r = nr; c = nc;
                }
                _ => break,
            }
        }
        self.set_cursor_normal(r, c);
    }

    /// char-индекс → байтовый индекс. Мост между курсором и String.
    pub fn byte_index(line: &str, char_idx: usize) -> usize {
        line.char_indices().nth(char_idx).map(|(i, _)| i).unwrap_or(line.len())
    }

    pub fn insert_char(&mut self, c: char) {
        let idx = Self::byte_index(&self.lines[self.row], self.col);
        self.lines[self.row].insert(idx, c);
        self.col += 1;
    }

    pub fn insert_newline(&mut self) {
        let idx = Self::byte_index(&self.lines[self.row], self.col);
        let rest = self.lines[self.row].split_off(idx);
        self.lines.insert(self.row + 1, rest);
        self.row += 1;
        self.col = 0;
    }

    pub fn backspace(&mut self) {
        if self.col > 0 {
            let start = Self::byte_index(&self.lines[self.row], self.col - 1);
            let end = Self::byte_index(&self.lines[self.row], self.col);
            self.lines[self.row].replace_range(start..end, "");
            self.col -= 1;
        } else if self.row > 0 {
            // В начале строки: склеиваем с предыдущей
            let line = self.lines.remove(self.row);
            self.row -= 1;
            self.col = self.line_len(self.row);
            self.lines[self.row].push_str(&line);
        }
    }

    /// Удалить символ ПОД курсором (команда x / Delete).
    pub fn delete_char(&mut self) {
        if self.col < self.line_len(self.row) {
            let start = Self::byte_index(&self.lines[self.row], self.col);
            let end = Self::byte_index(&self.lines[self.row], self.col + 1);
            self.lines[self.row].replace_range(start..end, "");
        }
    }

    pub fn delete_line(&mut self) -> String {
        let line = self.lines.remove(self.row);
        if self.lines.is_empty() {
            self.lines.push(String::new());
        }
        if self.row >= self.lines.len() {
            self.row = self.lines.len() - 1;
        }
        self.clamp_col_normal();
        line
    }

    /// В Normal курсор стоит НА символе (col ≤ len-1),
    /// в Insert — может стоять ПОСЛЕ последнего (col ≤ len). Как в Vim.
    pub fn clamp_col_normal(&mut self) {
        let len = self.line_len(self.row);
        self.col = self.col.min(len.saturating_sub(1));
    }

    pub fn clamp_col_insert(&mut self) {
        let len = self.line_len(self.row);
        self.col = self.col.min(len);
    }

    pub fn move_cursor(&mut self, dr: i32, dc: i32, insert: bool) {
        self.row = ((self.row as i32 + dr).max(0) as usize).min(self.lines.len() - 1);
        self.col = (self.col as i32 + dc).max(0) as usize;
        if insert {
            self.clamp_col_insert();
        } else {
            self.clamp_col_normal();
        }
    }
}
