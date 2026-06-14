use anyhow::Result;
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::env;
use std::io::Write;

pub struct PtyProcess {
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
    let writer = pty_pair.master.take_writer()?;

    Ok(PtyProcess { 
        writer,
        master: pty_pair.master,
    })
}
