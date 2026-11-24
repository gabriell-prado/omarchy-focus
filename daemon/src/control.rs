//! The control socket: a thread per connection reads request lines, hands
//! them to the core and writes each reply back. Subscribed connections also
//! receive events, written by the core through the same locked writer, so
//! lines never interleave.

use crate::core::{write_line, Msg, Writer};
use crate::protocol::{self, Command, MAX_LINE};
use std::io::{BufRead, BufReader, Read};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub fn serve(listener: UnixListener, tx: Sender<Msg>) {
    let mut next_id = 1u64;
    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let id = next_id;
                next_id += 1;
                let tx = tx.clone();
                let _ = thread::Builder::new().name(format!("client-{id}")).spawn(move || client(id, stream, tx));
            }
            Err(e) => eprintln!("omarchy-focus: accept failed: {e}"),
        }
    }
}

fn client(id: u64, stream: UnixStream, tx: Sender<Msg>) {
    // A client that stops reading must not stall the core.
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let writer: Writer = match stream.try_clone() {
        Ok(w) => Arc::new(Mutex::new(w)),
        Err(_) => return,
    };
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.by_ref().take(MAX_LINE as u64 + 1).read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if line.len() > MAX_LINE {
            let _ = write_line(&writer, &protocol::error("request too long"));
            break;
        }
        let text = line.trim();
        if text.is_empty() {
            continue;
        }
        let (reply_tx, reply_rx) = mpsc::channel();
        let msg = match protocol::parse(text) {
            Ok(Command::Subscribe) => Msg::Subscribe { client: id, writer: writer.clone(), reply: reply_tx },
            Ok(cmd) => Msg::Request { client: id, cmd, reply: reply_tx },
            Err(e) => {
                if write_line(&writer, &protocol::error(e)).is_err() {
                    break;
                }
                continue;
            }
        };
        if tx.send(msg).is_err() {
            break;
        }
        match reply_rx.recv() {
            Ok(reply) if write_line(&writer, &reply).is_ok() => {}
            _ => break,
        }
    }
    let _ = tx.send(Msg::Disconnected { client: id });
}
