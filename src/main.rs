mod config;
mod editor;
mod event;
mod lsp;
mod syntax;
mod terminal;
mod theme;

use editor::Editor;
use event::Event;
use std::io;
use std::sync::mpsc;
use std::thread;
use terminal::{Input, RawModeGuard, Screen};

fn main() -> io::Result<()> {
    let filepath = std::env::args().nth(1);
    let config = config::Config::load_or_create()?;

    let _raw = RawModeGuard::new()?;
    let mut screen = Screen::new()?;

    let (tx, rx) = mpsc::channel();
    
    // Ввод — отдельный поток.
    let input_tx = tx.clone();
    thread::spawn(move || {
        let mut input = Input::new();
        loop {
            match input.read_key() {
                Ok(key) => {
                    if input_tx.send(Event::Key(key)).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    let mut editor = Editor::new(filepath, config)?;
    editor.start_lsp(tx.clone());
    drop(tx);

    editor.render(&mut screen)?;

    while let Ok(event) = rx.recv() {
        editor.handle_event(event);

        // Выгребаем всё, что накопилось, и рисуем один кадр.
        while let Ok(event) = rx.try_recv() {
            editor.handle_event(event);
        }

        if editor.quit {
            break;
        }

        editor.render(&mut screen)?;
    } 
    Ok(())
}
