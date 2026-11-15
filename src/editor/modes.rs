#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Normal,
    Insert,
    Visual,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::Visual => "VISUAL",
        }
    }
}
