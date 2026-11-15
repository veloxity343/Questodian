"""Database migrations (Alembic), applied automatically when the app opens a database."""

from __future__ import annotations

import sqlite3
from datetime import datetime
from importlib import resources
from pathlib import Path

from alembic import command
from alembic.config import Config
from alembic.migration import MigrationContext
from alembic.script import ScriptDirectory
from sqlalchemy import Engine, inspect

from .models import Base

# The revision that matches the schema Pocket DM created before migrations existed.
BASELINE = "0001"


def alembic_config(engine: Engine | None = None) -> Config:
    cfg = Config()
    cfg.set_main_option("script_location", str(resources.files("pocket_dm").joinpath("migrations")))
    if engine is not None:
        cfg.attributes["engine"] = engine
    return cfg


def current_revision(engine: Engine) -> str | None:
    with engine.connect() as conn:
        return MigrationContext.configure(conn).get_current_revision()


def upgrade(engine: Engine, revision: str = "head", *, backup: Path | None = None) -> Path | None:
    """Bring the database schema up to date.

    If ``backup`` is the database file and a migration is about to change an
    existing database, a copy is saved next to it first. Returns that copy's path.
    """
    cfg = alembic_config(engine)
    tables = set(inspect(engine).get_table_names())
    if "alembic_version" not in tables and tables & set(Base.metadata.tables):
        # A database created before migrations were added: it already has the baseline schema.
        command.stamp(cfg, BASELINE)
    current = current_revision(engine)
    target = ScriptDirectory.from_config(cfg).get_current_head() if revision == "head" else revision
    saved = None
    if backup is not None and current is not None and current != target and backup.exists():
        saved = backup_database(backup, current)
    command.upgrade(cfg, revision)
    return saved


def backup_database(path: Path, label: str) -> Path:
    """Copy a live SQLite database safely (WAL included) to ``<name>.backup-<label>-<time>``."""
    stamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    dest = path.with_name(f"{path.name}.backup-{label}-{stamp}")
    with sqlite3.connect(path) as src, sqlite3.connect(dest) as out:
        src.backup(out)
    return dest
