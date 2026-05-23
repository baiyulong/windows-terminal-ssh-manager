/// Interactive SSH session via russh (pure-Rust SSH2, no native dependencies).
///
/// `interactive_connect` puts the terminal in raw mode, then runs a
/// bidirectional I/O loop between the keyboard (crossterm EventStream) and
/// the SSH channel (russh channel.wait / channel.data).
use anyhow::Result;
use async_trait::async_trait;
use crossterm::{
    event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    terminal,
};
use futures::StreamExt;
use russh::{client, ChannelMsg};
use ssh_key::PublicKey;
use std::{io::Write, sync::Arc};

// ── russh client handler ──────────────────────────────────────────────────────

struct ClientHandler;

#[async_trait]
impl client::Handler for ClientHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        _server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true) // auto-accept (equivalent to StrictHostKeyChecking=no)
    }
}

// ── key → VT100 bytes ────────────────────────────────────────────────────────

fn key_to_bytes(key: KeyEvent) -> Vec<u8> {
    match (key.code, key.modifiers) {
        // Ctrl+<letter>  →  control code 0x01–0x1A
        (KeyCode::Char(c), m) if m.contains(KeyModifiers::CONTROL) => {
            vec![(c.to_ascii_lowercase() as u8) & 0x1f]
        }
        (KeyCode::Char(c), _) => {
            let mut buf = [0u8; 4];
            c.encode_utf8(&mut buf).as_bytes().to_vec()
        }
        (KeyCode::Enter, _)     => b"\r".to_vec(),
        (KeyCode::Backspace, _) => vec![0x7f],
        (KeyCode::Tab, _)       => vec![b'\t'],
        (KeyCode::Esc, _)       => vec![0x1b],
        (KeyCode::Delete, _)    => b"\x1b[3~".to_vec(),
        (KeyCode::Up, _)        => b"\x1b[A".to_vec(),
        (KeyCode::Down, _)      => b"\x1b[B".to_vec(),
        (KeyCode::Right, _)     => b"\x1b[C".to_vec(),
        (KeyCode::Left, _)      => b"\x1b[D".to_vec(),
        (KeyCode::Home, _)      => b"\x1b[H".to_vec(),
        (KeyCode::End, _)       => b"\x1b[F".to_vec(),
        (KeyCode::Insert, _)    => b"\x1b[2~".to_vec(),
        (KeyCode::PageUp, _)    => b"\x1b[5~".to_vec(),
        (KeyCode::PageDown, _)  => b"\x1b[6~".to_vec(),
        (KeyCode::F(1), _)      => b"\x1bOP".to_vec(),
        (KeyCode::F(2), _)      => b"\x1bOQ".to_vec(),
        (KeyCode::F(3), _)      => b"\x1bOR".to_vec(),
        (KeyCode::F(4), _)      => b"\x1bOS".to_vec(),
        (KeyCode::F(n), _)      => format!("\x1b[{}~", n + 10).into_bytes(),
        _ => vec![],
    }
}

// ── RAII guard for raw mode ───────────────────────────────────────────────────

struct RawMode;
impl RawMode {
    fn enable() -> Result<Self> {
        terminal::enable_raw_mode()?;
        Ok(Self)
    }
}
impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

// ── public API ────────────────────────────────────────────────────────────────

/// Open an interactive SSH shell (connects, authenticates, then loops I/O).
pub async fn interactive_connect(
    host: &str,
    port: u16,
    username: &str,
    password: &str,
) -> Result<()> {
    let config = Arc::new(client::Config::default());
    let mut handle = client::connect(config, (host, port), ClientHandler).await?;

    let authenticated = handle.authenticate_password(username, password).await?;
    if !authenticated {
        anyhow::bail!("Authentication failed for {}@{}", username, host);
    }

    let mut channel = handle.channel_open_session().await?;
    let (cols, rows) = terminal::size().unwrap_or((80, 24));
    channel.request_pty(true, "xterm-256color", cols as u32, rows as u32, 0, 0, &[]).await?;
    channel.request_shell(true).await?;

    println!("  \u{2705}  Connected!  (type `exit` or Ctrl+D to disconnect)\r\n");

    let _raw = RawMode::enable()?;
    let mut stdout = std::io::stdout();
    let mut events = EventStream::new();

    loop {
        tokio::select! {
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { ref data }) => {
                        stdout.write_all(data)?;
                        stdout.flush()?;
                    }
                    Some(ChannelMsg::ExtendedData { ref data, .. }) => {
                        stdout.write_all(data)?;
                        stdout.flush()?;
                    }
                    Some(ChannelMsg::Eof)
                    | Some(ChannelMsg::Close)
                    | Some(ChannelMsg::ExitStatus { .. })
                    | None => break,
                    _ => {}
                }
            }
            maybe_event = events.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key)))
                        if key.kind == KeyEventKind::Press
                            || key.kind == KeyEventKind::Repeat =>
                    {
                        let bytes = key_to_bytes(key);
                        if !bytes.is_empty() {
                            channel.data(bytes.as_slice()).await?;
                        }
                    }
                    Some(Ok(Event::Resize(cols, rows))) => {
                        let _ = channel.window_change(cols as u32, rows as u32, 0, 0).await;
                    }
                    None => break,
                    _ => {}
                }
            }
        }
    }

    drop(_raw); // restore terminal before printing
    println!("\r\n  \u{1f50c}  Connection closed.");
    let _ = channel.close().await;
    Ok(())
}

/// Lightweight connection test — connects, authenticates, runs `uname -snrm`,
/// returns the banner string and elapsed ms.
pub async fn test_connection(
    host: &str,
    port: u16,
    username: &str,
    password: &str,
) -> Result<(String, u128)> {
    let t0 = std::time::Instant::now();
    let config = Arc::new(client::Config::default());
    let mut handle = client::connect(config, (host, port), ClientHandler).await?;

    let ok = handle.authenticate_password(username, password).await?;
    if !ok {
        anyhow::bail!("Authentication failed");
    }

    let mut channel = handle.channel_open_session().await?;
    channel.exec(true, "uname -snrm 2>/dev/null || ver").await?;

    let mut banner = String::new();
    loop {
        match channel.wait().await {
            Some(ChannelMsg::Data { ref data }) => {
                banner.push_str(&String::from_utf8_lossy(data));
            }
            Some(ChannelMsg::Eof)
            | Some(ChannelMsg::ExitStatus { .. })
            | Some(ChannelMsg::Close)
            | None => break,
            _ => {}
        }
    }
    let ms = t0.elapsed().as_millis();
    Ok((banner.trim().to_string(), ms))
}
