"""Server configuration model and persistence.

Config is stored at ~/.wt-ssh-manager/config.json.
Passwords are stored encrypted (Windows DPAPI) — never in plain text.
"""
import json
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import List, Optional

from .crypto import decrypt_password, encrypt_password

CONFIG_DIR = Path.home() / '.wt-ssh-manager'
CONFIG_FILE = CONFIG_DIR / 'config.json'


@dataclass
class ServerConfig:
    id: str
    name: str
    host: str
    port: int
    username: str
    encrypted_password: str
    description: str = ''
    tags: List[str] = field(default_factory=list)
    color: str = 'One Half Dark'


class ConfigManager:
    def __init__(self):
        CONFIG_DIR.mkdir(parents=True, exist_ok=True)
        self.servers: List[ServerConfig] = []
        self._load()

    # ── persistence ──────────────────────────────────────────────────────────

    def _load(self):
        if CONFIG_FILE.exists():
            with open(CONFIG_FILE, 'r', encoding='utf-8') as f:
                data = json.load(f)
            self.servers = [ServerConfig(**s) for s in data.get('servers', [])]

    def _save(self):
        data = {'servers': [asdict(s) for s in self.servers]}
        with open(CONFIG_FILE, 'w', encoding='utf-8') as f:
            json.dump(data, f, indent=2, ensure_ascii=False)

    # ── CRUD ─────────────────────────────────────────────────────────────────

    def add_server(
        self,
        name: str,
        host: str,
        port: int,
        username: str,
        password: str,
        description: str = '',
        tags: Optional[List[str]] = None,
        color: str = 'One Half Dark',
    ) -> ServerConfig:
        server_id = name.lower().replace(' ', '-')
        if self.get_server(server_id):
            raise ValueError(f"Server '{name}' already exists")

        server = ServerConfig(
            id=server_id,
            name=name,
            host=host,
            port=port,
            username=username,
            encrypted_password=encrypt_password(password),
            description=description,
            tags=tags or [],
            color=color,
        )
        self.servers.append(server)
        self._save()
        return server

    def get_server(self, name_or_id: str) -> Optional[ServerConfig]:
        for s in self.servers:
            if s.id == name_or_id or s.name == name_or_id:
                return s
        return None

    def list_servers(self) -> List[ServerConfig]:
        return list(self.servers)

    def remove_server(self, name_or_id: str) -> ServerConfig:
        server = self.get_server(name_or_id)
        if not server:
            raise ValueError(f"Server '{name_or_id}' not found")
        self.servers = [s for s in self.servers if s.id != server.id]
        self._save()
        return server

    def update_server(self, name_or_id: str, **kwargs) -> ServerConfig:
        server = self.get_server(name_or_id)
        if not server:
            raise ValueError(f"Server '{name_or_id}' not found")

        if 'password' in kwargs:
            kwargs['encrypted_password'] = encrypt_password(kwargs.pop('password'))

        for key, value in kwargs.items():
            if hasattr(server, key):
                setattr(server, key, value)

        self._save()
        return server

    def get_password(self, server: ServerConfig) -> str:
        """Decrypt and return the server's password (call site should clear after use)."""
        return decrypt_password(server.encrypted_password)
