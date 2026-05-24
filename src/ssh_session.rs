/// Interactive SSH session via russh (pure-Rust SSH2, no native dependencies).
///
/// `interactive_connect` puts the terminal in raw mode, then runs a
/// bidirectional I/O loop between the keyboard (crossterm EventStream) and
/// the SSH channel (russh channel.wait / channel.data).
///
/// Mouse events are forwarded to the SSH channel when the remote application
/// enables VT mouse tracking (detected by scanning channel output for the
/// `\x1b[?1000h` / `?1002h` / `?1003h` enable sequences).  SGR extended
/// mouse encoding (`\x1b[?1006h`) is tracked so the forwarded sequences use
/// the right format.  When the remote disables mouse tracking the forwarding
/// is suspended and Windows Terminal resumes its native selection behaviour.
use anyhow::Result;
use async_trait::async_trait;
use crossterm::{
    event::{
        DisableBracketedPaste, DisableFocusChange, EnableFocusChange,
        Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
        MouseButton, MouseEvent, MouseEventKind,
    },
    execute,
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

// ── mouse → VT escape bytes ───────────────────────────────────────────────────

/// Convert a crossterm mouse event into the appropriate VT escape sequence.
///
/// * `sgr = false` → classic X10 encoding  (`ESC [ M <b> <x> <y>`, 1-indexed,
///   limited to col/row ≤ 223)
/// * `sgr = true`  → SGR encoding (`ESC [ < Ps ; Px ; Py M/m`), unlimited range
fn mouse_to_bytes(mouse: MouseEvent, sgr: bool) -> Vec<u8> {
    let col = mouse.column + 1; // 1-indexed
    let row = mouse.row + 1;

    let (btn, is_release) = match mouse.kind {
        MouseEventKind::Down(MouseButton::Left)   => (0u16, false),
        MouseEventKind::Down(MouseButton::Middle) => (1u16, false),
        MouseEventKind::Down(MouseButton::Right)  => (2u16, false),
        MouseEventKind::Up(MouseButton::Left)     => (0u16, true),
        MouseEventKind::Up(MouseButton::Middle)   => (1u16, true),
        MouseEventKind::Up(_)                     => (2u16, true), // Right + unknown
        MouseEventKind::Drag(MouseButton::Left)   => (32u16, false),
        MouseEventKind::Drag(MouseButton::Middle) => (33u16, false),
        MouseEventKind::Drag(MouseButton::Right)  => (34u16, false),
        MouseEventKind::ScrollUp                  => (64u16, false),
        MouseEventKind::ScrollDown                => (65u16, false),
        // Moved without button / other — skip
        _ => return vec![],
    };

    let mut btn_code = btn;
    if mouse.modifiers.contains(KeyModifiers::SHIFT)   { btn_code += 4; }
    if mouse.modifiers.contains(KeyModifiers::ALT)     { btn_code += 8; }
    if mouse.modifiers.contains(KeyModifiers::CONTROL) { btn_code += 16; }

    if sgr {
        // SGR: ESC [ < Ps ; Px ; Py M  (press/drag)  or  …m  (release)
        let suffix = if is_release { 'm' } else { 'M' };
        format!("\x1b[<{};{};{}{}", btn_code, col, row, suffix).into_bytes()
    } else {
        // X10: limited to 223 cols/rows (value + 32 must fit in one byte)
        if col > 223 || row > 223 {
            return vec![];
        }
        vec![0x1b, b'[', b'M', btn_code as u8 + 32, col as u8 + 32, row as u8 + 32]
    }
}

// ── VT mouse-mode detector ────────────────────────────────────────────────────

/// Scan a chunk of server output for VT mouse-mode enable/disable sequences
/// and update `mouse_enabled` / `sgr_mode` / `bracketed_paste` accordingly.
///
/// Sequences detected:
///   enable  : `?1000h` (X10), `?1002h` (button), `?1003h` (any-event)
///   disable : `?1000l`, `?1002l`, `?1003l`
///   SGR ext : `?1006h`  (extended SGR mouse encoding)
///   paste   : `?2004h` (bracketed paste enable), `?2004l` (disable)
fn update_mouse_state(
    data: &[u8],
    mouse_enabled: &mut bool,
    sgr_mode: &mut bool,
    bracketed_paste: &mut bool,
) {
    if !data.contains(&0x1b) {
        return; // fast path: no escape sequences
    }
    let s = String::from_utf8_lossy(data);

    if s.contains("\x1b[?1000h")
        || s.contains("\x1b[?1002h")
        || s.contains("\x1b[?1003h")
    {
        *mouse_enabled = true;
    }
    if s.contains("\x1b[?1006h") {
        *sgr_mode = true;
    }
    if s.contains("\x1b[?1000l")
        || s.contains("\x1b[?1002l")
        || s.contains("\x1b[?1003l")
    {
        *mouse_enabled = false;
        *sgr_mode = false;
    }
    if s.contains("\x1b[?2004h") {
        *bracketed_paste = true;
    }
    if s.contains("\x1b[?2004l") {
        *bracketed_paste = false;
    }
}

// ── RAII guard for raw mode ───────────────────────────────────────────────────

struct RawMode;
impl RawMode {
    fn enable() -> Result<Self> {
        terminal::enable_raw_mode()?;
        // Do NOT pre-enable bracketed paste here.  The remote shell (bash) will
        // send `\x1b[?2004h` shortly after login; we forward it to local stdout,
        // which causes Windows Terminal to enable bracketed paste automatically.
        // Pre-enabling it ourselves creates a feedback loop: WT sends
        // `\x1b[?2004h` back into our stdin as KEY_EVENT records, which we then
        // forward to the SSH channel — appearing as garbage in remote input boxes.
        //
        // Enable focus-change reporting so we can forward FocusGained/FocusLost
        // to the remote application (e.g. vim, Copilot).
        let _ = execute!(std::io::stdout(), EnableFocusChange);
        Ok(Self)
    }
}
impl Drop for RawMode {
    fn drop(&mut self) {
        let mut out = std::io::stdout();
        let _ = execute!(out, DisableBracketedPaste, DisableFocusChange);
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
    let mut mouse_enabled = false;
    let mut sgr_mode = false;
    let mut remote_bracketed_paste = false;

    loop {
        tokio::select! {
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { ref data }) => {
                        update_mouse_state(data, &mut mouse_enabled, &mut sgr_mode, &mut remote_bracketed_paste);
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
                    // Paste arrives as a single Event::Paste when bracketed-paste
                    // mode is active locally.  Wrap with the remote's bracketed
                    // paste markers if the remote application expects them.
                    Some(Ok(Event::Paste(text))) => {
                        let bytes: Vec<u8> = if remote_bracketed_paste {
                            let mut v = b"\x1b[200~".to_vec();
                            v.extend_from_slice(text.as_bytes());
                            v.extend_from_slice(b"\x1b[201~");
                            v
                        } else {
                            text.into_bytes()
                        };
                        channel.data(bytes.as_slice()).await?;
                    }
                    Some(Ok(Event::Mouse(mouse))) if mouse_enabled => {
                        let bytes = mouse_to_bytes(mouse, sgr_mode);
                        if !bytes.is_empty() {
                            channel.data(bytes.as_slice()).await?;
                        }
                    }
                    Some(Ok(Event::Resize(cols, rows))) => {
                        let _ = channel.window_change(cols as u32, rows as u32, 0, 0).await;
                    }
                    Some(Ok(Event::FocusGained)) => {
                        let _ = channel.data(b"\x1b[I" as &[u8]).await;
                    }
                    Some(Ok(Event::FocusLost)) => {
                        let _ = channel.data(b"\x1b[O" as &[u8]).await;
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
