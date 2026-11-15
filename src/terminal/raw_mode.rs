use libc::{self, termios, tcgetattr, tcsetattr, STDIN_FILENO, TCSAFLUSH};
use std::io;
use std::mem::MaybeUninit;

/// RAII-обёртка над POSIX termios.
/// При создании сохраняет текущие настройки терминала и включает raw mode.
/// При drop — восстанавливает исходные настройки (даже при панике).
pub struct RawModeGuard {
    original: termios,
}

impl RawModeGuard {
    pub fn new() -> io::Result<Self> {
        let mut original = MaybeUninit::<termios>::zeroed();
        // SAFETY: tcgetattr заполняет структуру при успехе.
        if unsafe { tcgetattr(STDIN_FILENO, original.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let original = unsafe { original.assume_init() };

        let mut raw = original;
        // Выключаем канонический режим, эхо и сигналы (Ctrl-C).
        raw.c_lflag &= !(libc::ECHO | libc::ICANON | libc::IEXTEN | libc::ISIG);
        // Выключаем обработку специальных символов ввода.
        raw.c_iflag &= !(libc::BRKINT | libc::ICRNL | libc::INPCK | libc::ISTRIP | libc::IXON);
        // Отключаем постобработку вывода (чтобы '\n' не превращался в CRLF).
        raw.c_oflag &= !libc::OPOST;
        // Читаем по одному байту, без таймаута.
        raw.c_cc[libc::VMIN] = 1;
        raw.c_cc[libc::VTIME] = 0;

        if unsafe { tcsetattr(STDIN_FILENO, TCSAFLUSH, &raw) } != 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(Self { original })
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        // Восстанавливаем исходный режим — обязательно, иначе терминал "сломается".
        unsafe { tcsetattr(STDIN_FILENO, TCSAFLUSH, &self.original); }
    }
}
