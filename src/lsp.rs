use crate::event::Event;
use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::thread;

pub struct LspClient {
    child: Child,
    stdin: ChildStdin,
    next_id: i64,
}

impl LspClient {
    pub fn start(
        command: &str,
        args: &[String],
        root_uri: Option<String>,
        tx: mpsc::Sender<Event>,
    ) -> io::Result<Self> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Важно: stderr сервера не должен попадать в TUI.
            .stderr(Stdio::null())
            .spawn()?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "failed to open LSP stdin"))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "failed to open LSP stdout"))?;

        spawn_reader(stdout, tx);

        let mut client = Self {
            child,
            stdin,
            next_id: 1,
        };

        client.request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": root_uri,
                "capabilities": {
                    "textDocument": {
                        "synchronization": {
                            "didSave": true,
                            "dynamicRegistration": false
                        },
                        "publishDiagnostics": {
                            "relatedInformation": true,
                            "versionSupport": true
                        },
                        "completion": {
                            "completionItem": {
                                "snippetSupport": false
                            }
                        },
                        "definition": {
                            "linkSupport": true
                        },
                        "hover": {
                            "contentFormat": ["plaintext"]
                        }
                    },
                    "workspace": {
                        "configuration": true,
                        "didChangeConfiguration": {
                            "dynamicRegistration": false
                        }
                    }
                },
                "clientInfo": {
                    "name": "nuta",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }),
        )?;

        Ok(client)
    }

    fn write_message(&mut self, value: &Value) -> io::Result<()> {
        let body = serde_json::to_string(value)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        write!(
            self.stdin,
            "Content-Length: {}\r\n\r\n{}",
            body.len(),
            body
        )?;

        self.stdin.flush()
    }

    pub fn respond(&mut self, id: Value, result: Value) -> io::Result<()> {
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        });

        self.write_message(&msg)
    }

    pub fn request(&mut self, method: &str, params: Value) -> io::Result<i64> {
        let id = self.next_id;
        self.next_id += 1;

        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        self.write_message(&msg)?;
        Ok(id)
    }

    pub fn notify(&mut self, method: &str, params: Value) -> io::Result<()> {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        self.write_message(&msg)
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        // MVP: грубо завершаем сервер. Потом сделаем graceful shutdown/exit.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn_reader(stdout: std::process::ChildStdout, tx: mpsc::Sender<Event>) {
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);

        loop {
            let mut content_length: Option<usize> = None;

            // Читаем заголовки до пустой строки.
            loop {
                let mut line = String::new();

                match reader.read_line(&mut line) {
                    Ok(0) => {
                        let _ = tx.send(Event::LspExit);
                        return;
                    }
                    Ok(_) => {}
                    Err(e) => {
                        let _ = tx.send(Event::LspError(e.to_string()));
                        return;
                    }
                }

                let line = line.trim_end();

                if line.is_empty() {
                    break;
                }

                if let Some(value) = line.strip_prefix("Content-Length:") {
                    match value.trim().parse::<usize>() {
                        Ok(len) => content_length = Some(len),
                        Err(e) => {
                            let _ = tx.send(Event::LspError(e.to_string()));
                            return;
                        }
                    }
                }
            }

            let Some(len) = content_length else {
                continue;
            };

            let mut body = vec![0u8; len];

            if let Err(e) = reader.read_exact(&mut body) {
                let _ = tx.send(Event::LspError(e.to_string()));
                return;
            }

            match serde_json::from_slice::<Value>(&body) {
                Ok(value) => {
                    if tx.send(Event::LspMessage(value)).is_err() {
                        return;
                    }
                }
                Err(e) => {
                    let _ = tx.send(Event::LspError(e.to_string()));
                }
            }
        }
    });
}
