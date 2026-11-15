import shutil
from importlib import resources

from alembic import command
from alembic.autogenerate import compare_metadata
from alembic.migration import MigrationContext
from alembic.script import ScriptDirectory
from sqlalchemy import create_engine, inspect, text

from pocket_dm import migrate
from pocket_dm.db import DB_FILENAME, Database
from pocket_dm.models import Base
from pocket_dm.repo import Repo


def _head() -> str:
    return ScriptDirectory.from_config(migrate.alembic_config()).get_current_head()


def _current(engine) -> str:
    with engine.connect() as conn:
        return MigrationContext.configure(conn).get_current_revision()


def test_new_database_is_at_head_and_matches_models(db):
    assert _current(db.engine) == _head()
    with db.engine.connect() as conn:
        diffs = compare_metadata(MigrationContext.configure(conn), Base.metadata)
    assert diffs == [], "models changed without a migration: run `uv run alembic revision --autogenerate`"


def test_database_from_before_migrations_is_adopted(tmp_path):
    engine = create_engine(f"sqlite:///{tmp_path / DB_FILENAME}")
    # Databases made before Alembic had the 0001 schema and no alembic_version table.
    command.upgrade(migrate.alembic_config(engine), "0001")
    with engine.begin() as conn:
        conn.execute(text("DROP TABLE alembic_version"))
        conn.execute(text(
            "INSERT INTO handbook (id, created, updated, campaign_id, title, category, tags, body) "
            "VALUES ('old1', '', '', NULL, 'Kept', 'Lore', '[]', 'still here')"
        ))
    engine.dispose()

    db = Database(tmp_path)
    assert _current(db.engine) == _head()
    with db.session() as s:
        assert Repo(s).get("handbook", "old1")["body"] == "still here"
    db.dispose()


def test_table_rebuild_does_not_cascade_deletes(tmp_path, monkeypatch):
    """Batch-mode rebuilds drop and recreate tables; that must not trigger ON DELETE CASCADE."""
    scripts = tmp_path / "migrations"
    shutil.copytree(str(resources.files("pocket_dm").joinpath("migrations")), scripts)
    (scripts / "versions" / "9999_rebuild_campaigns.py").write_text(
        f'''
from alembic import op
import sqlalchemy as sa

revision = "9999"
down_revision = "{_head()}"

def upgrade():
    with op.batch_alter_table("campaigns", recreate="always") as batch:
        batch.add_column(sa.Column("extra", sa.String(), nullable=True))
'''
    )
    db = Database(tmp_path / "data")
    with db.session() as s:
        repo = Repo(s)
        camp = repo.create("campaigns", {"name": "C"})
        sess = repo.create("sessions", {"campaign_id": camp["id"]})

    real_config = migrate.alembic_config
    monkeypatch.setattr(migrate, "alembic_config", lambda engine=None: _with_location(real_config(engine), scripts))
    migrate.upgrade(db.engine)

    assert "extra" in {c["name"] for c in inspect(db.engine).get_columns("campaigns")}
    with db.session() as s:
        assert Repo(s).get("sessions", sess["id"])["campaign_id"] == camp["id"]
        assert s.execute(text("PRAGMA foreign_keys")).scalar() == 1  # switched back on
    db.dispose()


def _with_location(cfg, path):
    cfg.set_main_option("script_location", str(path))
    return cfg


def test_downgrade_to_base_and_back(db):
    cfg = migrate.alembic_config(db.engine)
    command.downgrade(cfg, "base")
    assert set(inspect(db.engine).get_table_names()) == {"alembic_version"}
    command.upgrade(cfg, "head")
    assert set(Base.metadata.tables) <= set(inspect(db.engine).get_table_names())


def test_0002_moves_session_encounters_and_backs_up(tmp_path):
    path = tmp_path / DB_FILENAME
    engine = create_engine(f"sqlite:///{path}")
    command.upgrade(migrate.alembic_config(engine), "0001")
    with engine.begin() as conn:
        conn.execute(text(
            "INSERT INTO campaigns (id, created, updated, name, system, description, setting, players, notes) "
            "VALUES ('c1', '', '', 'C', '', '', '', '[]', '')"))
        conn.execute(text(
            "INSERT INTO sessions (id, created, updated, campaign_id, number, title, date, status, attendance, prep,"
            " recap, log, encounter, loot, xp_awarded) VALUES ('s1', '', '', 'c1', 2, 'Causeway', '', 'planned', '[]',"
            " '', '', '[]', :enc, '[]', 0)"),
            {"enc": '{"round": 1, "turn": 0, "active": true, "combatants": [{"id": "z", "name": "Zombie", "kind": "monster"}]}'})
    engine.dispose()

    db = Database(tmp_path)
    assert db.backup is not None and db.backup.name.startswith("pocket-dm.db.backup-0001-")
    with db.session() as s:
        repo = Repo(s)
        (enc,) = repo.list("encounters")
        assert enc["session_id"] == "s1" and enc["name"] == "Session 2: Causeway" and enc["active"]
        assert enc["combatants"][0]["name"] == "Zombie"
        assert repo.get("campaigns", "c1")["edition"] == "2014"
    db.dispose()
    assert Database(tmp_path).backup is None  # nothing to migrate the second time
