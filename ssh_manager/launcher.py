"""Interactive SSH launcher — runs inside a Windows Terminal tab.

Usage (called automatically by Windows Terminal via the Fragment profile):
    python launcher.py <server-id>

The script decrypts the stored password, connects via Paramiko, then drives
an interactive PTY session.  Keyboard input is handled with msvcrt so that
special keys (arrows, F-keys, etc.) are translated to the correct VT100
escape sequences expected by the remote shell.
"""
import os
import sys
import threading
import time
from pathlib import Path

# Make the package importable when invoked directly by Windows Terminal
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import paramiko  # noqa: E402

from ssh_manager.config import ConfigManager  # noqa: E402

# ── Special-key mapping (Windows console → VT100) ────────────────────────────
_SPECIAL_KEYS = {
    'H': '\x1b[A',   # ↑  Up
    'P': '\x1b[B',   # ↓  Down
    'M': '\x1b[C',   # →  Right
    'K': '\x1b[D',   # ←  Left
    'G': '\x1b[H',   # Home
    'O': '\x1b[F',   # End
    'I': '\x1b[5~',  # Page Up
    'Q': '\x1b[6~',  # Page Down
    'R': '\x1b[2~',  # Insert
    'S': '\x1b[3~',  # Delete
    ';': '\x1bOP',   # F1
    '<': '\x1bOQ',   # F2
    '=': '\x1bOR',   # F3
    '>': '\x1bOS',   # F4
    '?': '\x1b[15~', # F5
    '@': '\x1b[17~', # F6
    'A': '\x1b[18~', # F7
    'B': '\x1b[19~', # F8
}


def _interactive_shell(channel: paramiko.Channel) -> None:
    """Pump stdin → SSH channel and SSH channel → stdout until session ends."""
    import msvcrt

    stop = threading.Event()

    def _recv() -> None:
        """Background thread: SSH channel → stdout."""
        while not stop.is_set():
            try:
                if channel.recv_ready():
                    data = channel.recv(4096)
                    if not data:
                        break
                    sys.stdout.buffer.write(data)
                    sys.stdout.buffer.flush()
                elif channel.closed or channel.exit_status_ready():
                    break
                else:
                    time.sleep(0.005)
            except Exception:
                break
        stop.set()

    recv_thread = threading.Thread(target=_recv, daemon=True)
    recv_thread.start()

    try:
        while not stop.is_set():
            if msvcrt.kbhit():
                ch = msvcrt.getwch()
                if ch in ('\x00', '\xe0'):
                    ch2 = msvcrt.getwch()
                    vt100 = _SPECIAL_KEYS.get(ch2)
                    if vt100:
                        channel.send(vt100.encode())
                else:
                    channel.send(ch.encode('utf-8', errors='replace'))
            else:
                time.sleep(0.005)
    except (KeyboardInterrupt, EOFError):
        pass
    finally:
        stop.set()
        channel.close()
        recv_thread.join(timeout=3)


def _get_terminal_size() -> tuple[int, int]:
    try:
        s = os.get_terminal_size()
        return s.columns, s.lines
    except OSError:
        return 120, 30


def connect(server_id: str) -> None:
    cfg = ConfigManager()
    server = cfg.get_server(server_id)
    if not server:
        print(
            f'\r\n❌  Server "{server_id}" not found.\r\n'
            f'   Run: python main.py list\r\n',
            flush=True,
        )
        sys.exit(1)

    password = cfg.get_password(server)

    print(
        f'\r\n  🔌  Connecting to '
        f'{server.username}@{server.host}:{server.port} ...\r\n',
        flush=True,
    )

    client = paramiko.SSHClient()
    client.set_missing_host_key_policy(paramiko.AutoAddPolicy())

    try:
        client.connect(
            server.host,
            port=server.port,
            username=server.username,
            password=password,
            timeout=15,
            banner_timeout=20,
            auth_timeout=20,
        )
    except paramiko.AuthenticationException:
        print(
            f'\r\n❌  Authentication failed for {server.username}@{server.host}\r\n'
            f'   Update password: python main.py edit {server.id}\r\n',
            flush=True,
        )
        sys.exit(1)
    except Exception as exc:
        print(f'\r\n❌  Connection failed: {exc}\r\n', flush=True)
        sys.exit(1)
    finally:
        del password  # Clear from memory ASAP

    cols, rows = _get_terminal_size()
    channel = client.invoke_shell(term='xterm-256color', width=cols, height=rows)
    channel.settimeout(0)

    print(f'  ✅  Connected!  (type `exit` or Ctrl+D to close)\r\n', flush=True)

    _interactive_shell(channel)
    client.close()
    print('\r\n  🔌  Connection closed.\r\n', flush=True)


if __name__ == '__main__':
    if len(sys.argv) < 2:
        print('Usage: launcher.py <server-id>', flush=True)
        sys.exit(1)
    connect(sys.argv[1])
