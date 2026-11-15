// Catppuccin Mocha — мягкая тёмная палитра.
// Формат: \x1b[38;2;R;G;Bm — цвет текста, \x1b[48;2;R;G;Bm — цвет фона.

pub const FG_TEXT: &str     = "\x1b[38;2;205;214;244m"; // Text     #cdd6f4
pub const FG_SUBTEXT: &str  = "\x1b[38;2;166;173;200m"; // Subtext0 #a6adc8
pub const FG_OVERLAY: &str  = "\x1b[38;2;108;112;134m"; // Overlay0 #6c7086
pub const FG_LAVENDER: &str = "\x1b[38;2;180;190;254m"; // Lavender #b4befe
pub const FG_BASE: &str     = "\x1b[38;2;30;30;46m";    // Base     #1e1e2e (тёмный текст)

pub const BG_SURFACE0: &str = "\x1b[48;2;49;50;68m";    // Surface0 #313244
pub const BG_SURFACE1: &str = "\x1b[48;2;69;71;90m";    // Surface1 #45475a
pub const BG_SURFACE2: &str = "\x1b[48;2;88;91;112m";   // Surface2 #585b70
pub const BG_MAUVE: &str    = "\x1b[48;2;203;166;247m"; // Mauve    #cba6f7 ← мягкий фиолетовый
pub const BG_GREEN: &str    = "\x1b[48;2;166;227;161m"; // Green    #a6e3a1
pub const BG_PINK: &str     = "\x1b[48;2;245;194;231m"; // Pink     #f5c2e7

pub const FG_MAUVE: &str = "\x1b[38;2;203;166;247m"; // Mauve #cba6f7
pub const FG_GREEN: &str = "\x1b[38;2;166;227;161m"; // Green #a6e3a1
pub const FG_PEACH: &str  = "\x1b[38;2;250;179;135m"; // Peach  #fab387
pub const FG_YELLOW: &str = "\x1b[38;2;249;226;175m"; // Yellow #f9e2af
pub const FG_BLUE: &str   = "\x1b[38;2;137;180;250m"; // Blue   #89b4fa
pub const FG_TEAL: &str   = "\x1b[38;2;148;226;213m"; // Teal   #94e2d5
pub const FG_PINK: &str   = "\x1b[38;2;245;194;231m"; // Pink   #f5c2e7
pub const FG_RED: &str = "\x1b[38;2;243;139;168m"; // Red #f38ba8
