"""Windows Terminal JSON Fragment generator.

Writes a Fragment JSON file to::

    %LOCALAPPDATA%/Microsoft/Windows Terminal/Fragments/wt-ssh-manager/profiles.json

Each saved server becomes a native Windows Terminal profile.  Opening the
profile runs launcher.py which auto-connects using the stored credentials.

UUID generation follows the official WT namespace spec:
  https://learn.microsoft.com/en-us/windows/terminal/json-fragment-extensions
"""
import json
import os
import sys
import uuid
from pathlib import Path
from typing import List

from .config import ConfigManager, ServerConfig

# WT Fragment storage location (user-scoped installation)
_WT_FRAGMENTS = (
    Path(os.environ.get('LOCALAPPDATA', Path.home() / 'AppData' / 'Local'))
    / 'Microsoft' / 'Windows Terminal' / 'Fragments' / 'wt-ssh-manager'
)

# Namespace GUIDs from the WT spec
_WT_NAMESPACE = uuid.UUID('{f65ddb7e-706b-4499-8a50-40313caf510a}')
_APP_NAME = 'wt-ssh-manager'
_APP_NS = uuid.uuid5(_WT_NAMESPACE, _APP_NAME.encode('UTF-16LE').decode('latin-1'))


def _profile_guid(server_name: str) -> str:
    """Deterministic UUID-5 for a server profile (stable across syncs)."""
    guid = uuid.uuid5(_APP_NS, server_name.encode('UTF-16LE').decode('latin-1'))
    return f'{{{guid}}}'


def _launcher_path() -> Path:
    return Path(__file__).parent / 'launcher.py'


def _make_profile(server: ServerConfig, launcher: Path, python_exe: str) -> dict:
    """Build one WT profile dict for *server*."""
    commandline = f'"{python_exe}" "{launcher}" {server.id}'
    return {
        'guid': _profile_guid(server.name),
        'name': f'\U0001f5a5  {server.name}',
        'commandline': commandline,
        'tabTitle': server.name,
        'colorScheme': server.color,
        'startingDirectory': '%USERPROFILE%',
        'icon': '\U0001f5a5',
    }


def generate_fragment(servers: List[ServerConfig]) -> dict:
    """Return the Fragment JSON dict (not written to disk)."""
    launcher = _launcher_path()
    python_exe = sys.executable.replace('\\', '\\\\')  # json-safe but kept as str
    launcher_str = str(launcher)

    profiles = [_make_profile(s, launcher_str, sys.executable) for s in servers]
    return {'profiles': profiles}


def sync_fragment(config: ConfigManager):
    """Write profiles.json to the WT Fragments directory and return (path, count)."""
    _WT_FRAGMENTS.mkdir(parents=True, exist_ok=True)

    servers = config.list_servers()
    fragment = generate_fragment(servers)

    fragment_file = _WT_FRAGMENTS / 'profiles.json'
    with open(fragment_file, 'w', encoding='utf-8') as f:
        json.dump(fragment, f, indent=2, ensure_ascii=False)

    return fragment_file, len(servers)
