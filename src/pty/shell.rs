use anyhow::Result;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::env;
use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

pub struct PtyProcess {
    pub receiver: Receiver<Vec<u8>>,
    pub writer: Box<dyn Write + Send>,
    pub master: Box<dyn MasterPty + Send>,
}

pub fn spawn_interactive_shell() -> Result<PtyProcess> {
    let pty_system = native_pty_system();

    let pty_pair = pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let shell_path = env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());

    let mut cmd = CommandBuilder::new(shell_path);
    cmd.env("TERM", "xterm-256color");
    let _child = pty_pair.slave.spawn_command(cmd)?;

    drop(pty_pair.slave);

    let mut reader = pty_pair.master.try_clone_reader()?;
    let writer = pty_pair.master.take_writer()?;

    let (tx, rx): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = mpsc::channel();

    thread::spawn(move || {
        let mut buffer = [0u8; 1024];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    if tx.send(buffer[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    Ok(PtyProcess {
        receiver: rx,
        writer,
        master: pty_pair.master,
    })
}
