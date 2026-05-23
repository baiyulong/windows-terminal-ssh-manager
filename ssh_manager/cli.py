"""CLI commands for wt-ssh-manager.

Commands:
    add      — interactive wizard to add a server
    list     — rich table of all servers
    remove   — delete a server (with confirmation)
    edit     — update fields of an existing server
    sync     — regenerate Windows Terminal Fragment profiles
    test     — verify connection to a server
    connect  — open interactive SSH session in current terminal
"""
import subprocess
import sys
from pathlib import Path

import click
from rich import box
from rich.console import Console
from rich.panel import Panel
from rich.prompt import Confirm, IntPrompt, Prompt
from rich.table import Table

from .config import ConfigManager
from .fragment import sync_fragment

console = Console()

# ── helpers ───────────────────────────────────────────────────────────────────


def _get_config() -> ConfigManager:
    return ConfigManager()


def _auto_sync(cfg: ConfigManager) -> None:
    """Regenerate the WT Fragment silently after any config change."""
    fragment_file, count = sync_fragment(cfg)
    console.print(
        f'[dim]   ↺  Fragment updated ({count} profile(s)) — '
        f'restart Windows Terminal to apply changes.[/dim]'
    )


# ── CLI root ──────────────────────────────────────────────────────────────────


@click.group()
def cli():
    """🖥   wt-ssh-manager — Windows Terminal SSH Manager

    Manages SSH server profiles injected into Windows Terminal via
    JSON Fragment Extensions.  Passwords are encrypted with Windows DPAPI
    (bound to your OS user account).
    """


# ── add ───────────────────────────────────────────────────────────────────────


@cli.command()
def add():
    """Add a new SSH server (interactive wizard)."""
    console.print(Panel('[bold cyan]Add New SSH Server[/bold cyan]', expand=False))

    name = Prompt.ask('  Server name (e.g. prod-web)')
    host = Prompt.ask('  Host / IP address')
    port = int(Prompt.ask('  Port', default='22'))
    username = Prompt.ask('  Username')
    password = Prompt.ask('  Password', password=True)
    description = Prompt.ask('  Description (optional)', default='')

    cfg = _get_config()
    try:
        server = cfg.add_server(name, host, port, username, password, description)
    except ValueError as exc:
        console.print(f'[red]❌  {exc}[/red]')
        raise SystemExit(1)

    console.print(
        f'\n[green]✅  Server "[bold]{server.name}[/bold]" '
        f'({server.username}@{server.host}:{server.port}) added.[/green]'
    )
    _auto_sync(cfg)


# ── list ──────────────────────────────────────────────────────────────────────


@cli.command('list')
def list_servers():
    """List all configured SSH servers."""
    cfg = _get_config()
    servers = cfg.list_servers()

    if not servers:
        console.print(
            '[yellow]No servers configured yet.  '
            'Run [bold]ssh-manager add[/bold] to add one.[/yellow]'
        )
        return

    table = Table(box=box.ROUNDED, show_header=True, header_style='bold magenta')
    table.add_column('Name', style='cyan bold', no_wrap=True)
    table.add_column('Host')
    table.add_column('Port', justify='center')
    table.add_column('Username', style='green')
    table.add_column('Description', style='dim')
    table.add_column('Tags', style='dim')

    for s in servers:
        table.add_row(
            s.name,
            s.host,
            str(s.port),
            s.username,
            s.description or '—',
            ', '.join(s.tags) if s.tags else '—',
        )

    console.print(table)


# ── remove ────────────────────────────────────────────────────────────────────


@cli.command()
@click.argument('name')
def remove(name: str):
    """Remove an SSH server (NAME is the server id or name)."""
    cfg = _get_config()
    server = cfg.get_server(name)
    if not server:
        console.print(f'[red]❌  Server "{name}" not found.[/red]')
        raise SystemExit(1)

    if not Confirm.ask(
        f'  Remove [bold]{server.name}[/bold] ({server.username}@{server.host})?'
    ):
        console.print('[dim]Cancelled.[/dim]')
        return

    cfg.remove_server(name)
    console.print(f'[green]✅  Server "{server.name}" removed.[/green]')
    _auto_sync(cfg)


# ── edit ──────────────────────────────────────────────────────────────────────


@cli.command()
@click.argument('name')
def edit(name: str):
    """Edit an existing server's configuration."""
    cfg = _get_config()
    server = cfg.get_server(name)
    if not server:
        console.print(f'[red]❌  Server "{name}" not found.[/red]')
        raise SystemExit(1)

    console.print(
        Panel(
            f'[bold cyan]Edit: {server.name}[/bold cyan]  '
            '[dim](press Enter to keep current value)[/dim]',
            expand=False,
        )
    )

    updates: dict = {}

    new_host = Prompt.ask('  Host', default=server.host)
    if new_host != server.host:
        updates['host'] = new_host

    new_port = Prompt.ask('  Port', default=str(server.port))
    if new_port != str(server.port):
        updates['port'] = int(new_port)

    new_user = Prompt.ask('  Username', default=server.username)
    if new_user != server.username:
        updates['username'] = new_user

    if Confirm.ask('  Change password?', default=False):
        updates['password'] = Prompt.ask('  New password', password=True)

    new_desc = Prompt.ask('  Description', default=server.description or '')
    if new_desc != (server.description or ''):
        updates['description'] = new_desc

    if updates:
        cfg.update_server(name, **updates)
        console.print(f'[green]✅  Server "{server.name}" updated.[/green]')
        _auto_sync(cfg)
    else:
        console.print('[dim]No changes made.[/dim]')


# ── sync ──────────────────────────────────────────────────────────────────────


@cli.command()
def sync():
    """Regenerate Windows Terminal Fragment profiles."""
    cfg = _get_config()
    fragment_file, count = sync_fragment(cfg)
    console.print(f'[green]✅  Fragment written to:[/green]')
    console.print(f'   [dim]{fragment_file}[/dim]')
    console.print(f'   {count} profile(s) injected.')
    console.print('[dim]   Restart Windows Terminal to apply changes.[/dim]')


# ── test ──────────────────────────────────────────────────────────────────────


@cli.command()
@click.argument('name')
def test(name: str):
    """Test the SSH connection to a server."""
    import time

    import paramiko

    cfg = _get_config()
    server = cfg.get_server(name)
    if not server:
        console.print(f'[red]❌  Server "{name}" not found.[/red]')
        raise SystemExit(1)

    password = cfg.get_password(server)

    with console.status(
        f'Testing [cyan]{server.username}@{server.host}:{server.port}[/cyan] ...'
    ):
        client = paramiko.SSHClient()
        client.set_missing_host_key_policy(paramiko.AutoAddPolicy())
        t0 = time.time()
        try:
            client.connect(
                server.host,
                port=server.port,
                username=server.username,
                password=password,
                timeout=10,
            )
            latency_ms = int((time.time() - t0) * 1000)
            _, stdout, _ = client.exec_command('uname -snrm 2>/dev/null || ver')
            banner = stdout.read().decode(errors='replace').strip()
            client.close()

            console.print(
                f'[green]✅  [bold]{server.name}[/bold] reachable '
                f'({latency_ms} ms)[/green]'
            )
            if banner:
                console.print(f'   [dim]{banner}[/dim]')

        except paramiko.AuthenticationException:
            console.print(
                f'[red]❌  Authentication failed.  '
                f'Update with: ssh-manager edit {server.id}[/red]'
            )
        except Exception as exc:
            console.print(f'[red]❌  Connection failed: {exc}[/red]')
        finally:
            del password


# ── connect ───────────────────────────────────────────────────────────────────


@cli.command()
@click.argument('name', required=False, default=None)
def connect(name: str):
    """Connect to an SSH server (interactive picker when NAME is omitted)."""
    if not name:
        cfg = _get_config()
        servers = cfg.list_servers()
        if not servers:
            console.print(
                '[yellow]No servers configured.  '
                'Run [bold]ssh-manager add[/bold] to add one.[/yellow]'
            )
            raise SystemExit(0)

        table = Table(box=box.SIMPLE, show_header=False, padding=(0, 1))
        table.add_column('#', style='bold cyan', justify='right', no_wrap=True)
        table.add_column('Name', style='cyan bold', no_wrap=True)
        table.add_column('User@Host', style='green')
        table.add_column('Description', style='dim')

        for i, s in enumerate(servers, 1):
            table.add_row(
                str(i),
                s.name,
                f'{s.username}@{s.host}:{s.port}',
                s.description or '',
            )

        console.print(table)
        choice = IntPrompt.ask('  Select server', default=1)
        if not (1 <= choice <= len(servers)):
            console.print('[red]❌  Invalid selection.[/red]')
            raise SystemExit(1)
        name = servers[choice - 1].id

    launcher = Path(__file__).parent / 'launcher.py'
    subprocess.run([sys.executable, str(launcher), name], check=False)
