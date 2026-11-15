use crate::terminal::Key;
use serde_json::Value;

pub enum Event {
    Key(Key),
    LspMessage(Value),
    LspError(String),
    LspExit,
}
