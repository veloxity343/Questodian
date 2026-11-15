"""SQLAlchemy ORM tables.

Top-level fields are real columns and links between records are foreign keys.
Nested structures that are always read and written as a whole (a character's
inventory, a session's encounter, and so on) are stored in JSON columns.
"""

from __future__ import annotations

import uuid
from datetime import UTC, datetime
from typing import Any

from sqlalchemy import JSON, ForeignKey, MetaData, String
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column, relationship


def new_id() -> str:
    return uuid.uuid4().hex[:12]


def now() -> str:
    return datetime.now(UTC).isoformat(timespec="seconds")


class Base(DeclarativeBase):
    # Named constraints let Alembic's batch mode alter them on SQLite.
    metadata = MetaData(
        naming_convention={
            "ix": "ix_%(column_0_label)s",
            "uq": "uq_%(table_name)s_%(column_0_name)s",
            "ck": "ck_%(table_name)s_%(constraint_name)s",
            "fk": "fk_%(table_name)s_%(column_0_name)s_%(referred_table_name)s",
            "pk": "pk_%(table_name)s",
        }
    )
    type_annotation_map = {dict[str, Any]: JSON, list[Any]: JSON}


class Record(Base):
    __abstract__ = True

    id: Mapped[str] = mapped_column(String(64), primary_key=True, default=new_id)
    created: Mapped[str] = mapped_column(default=now)
    updated: Mapped[str] = mapped_column(default=now)


def _campaign_fk(ondelete: str) -> Mapped[str | None]:
    return mapped_column(ForeignKey("campaigns.id", ondelete=ondelete), index=True)


def _fk(table: str, ondelete: str = "SET NULL") -> Mapped[str | None]:
    return mapped_column(ForeignKey(f"{table}.id", ondelete=ondelete), index=True)


class Campaign(Record):
    __tablename__ = "campaigns"

    name: Mapped[str]
    system: Mapped[str]
    edition: Mapped[str] = mapped_column(server_default="2014")  # "2014" (SRD 5.1) or "2024" (SRD 5.2.1)
    description: Mapped[str]
    setting: Mapped[str]
    players: Mapped[list[Any]]
    notes: Mapped[str]

    sessions: Mapped[list[Session]] = relationship(
        back_populates="campaign", cascade="all, delete-orphan", passive_deletes=True
    )
    characters: Mapped[list[Character]] = relationship(back_populates="campaign", passive_deletes=True)
    handbook: Mapped[list[HandbookEntry]] = relationship(back_populates="campaign", passive_deletes=True)


class Session(Record):
    __tablename__ = "sessions"

    campaign_id: Mapped[str | None] = _campaign_fk("CASCADE")
    number: Mapped[int]
    title: Mapped[str]
    date: Mapped[str]
    status: Mapped[str]
    attendance: Mapped[list[Any]]
    prep: Mapped[str]
    recap: Mapped[str]
    log: Mapped[list[Any]]
    loot: Mapped[list[Any]]
    xp_awarded: Mapped[int]

    campaign: Mapped[Campaign | None] = relationship(back_populates="sessions")


class Character(Record):
    __tablename__ = "characters"

    campaign_id: Mapped[str | None] = _campaign_fk("SET NULL")
    name: Mapped[str]
    player: Mapped[str]
    race: Mapped[str]
    background: Mapped[str]
    alignment: Mapped[str]
    classes: Mapped[list[Any]]
    xp: Mapped[int]
    abilities: Mapped[dict[str, Any]]
    save_proficiencies: Mapped[list[Any]]
    skills: Mapped[dict[str, Any]]
    jack_of_all_trades: Mapped[bool]
    ac: Mapped[int]
    initiative_misc: Mapped[int]
    speed: Mapped[int]
    hp: Mapped[dict[str, Any]]
    hit_dice: Mapped[dict[str, Any]]
    death_saves: Mapped[dict[str, Any]]
    inspiration: Mapped[bool]
    conditions: Mapped[list[Any]]
    exhaustion: Mapped[int]
    proficiencies: Mapped[dict[str, Any]]
    attacks: Mapped[list[Any]]
    spellcasting: Mapped[dict[str, Any]]
    resources: Mapped[list[Any]]
    inventory: Mapped[list[Any]]
    currency: Mapped[dict[str, Any]]
    features: Mapped[list[Any]]
    personality: Mapped[dict[str, Any]]
    appearance: Mapped[str]
    backstory: Mapped[str]
    notes: Mapped[str]

    campaign: Mapped[Campaign | None] = relationship(back_populates="characters")


class HandbookEntry(Record):
    __tablename__ = "handbook"

    campaign_id: Mapped[str | None] = _campaign_fk("SET NULL")
    title: Mapped[str]
    category: Mapped[str]
    tags: Mapped[list[Any]]
    body: Mapped[str]

    campaign: Mapped[Campaign | None] = relationship(back_populates="handbook")


# --------------------------------------------------------------------------- DM screen
# Everything below belongs to one campaign and goes with it when the campaign is deleted.
# Links to a session or scene are optional and are cleared if that record is deleted.


class Scene(Record):
    """A prepared beat of a session: summary, read-aloud text and DM notes."""

    __tablename__ = "scenes"

    campaign_id: Mapped[str | None] = _campaign_fk("CASCADE")
    session_id: Mapped[str | None] = _fk("sessions")
    position: Mapped[int]
    title: Mapped[str]
    location: Mapped[str]
    status: Mapped[str]  # planned | active | completed | skipped
    summary: Mapped[str]
    read_aloud: Mapped[str]
    body: Mapped[str]


class Npc(Record):
    __tablename__ = "npcs"

    campaign_id: Mapped[str | None] = _campaign_fk("CASCADE")
    name: Mapped[str]
    role: Mapped[str]
    first_appears: Mapped[str]
    looks: Mapped[str]
    voice: Mapped[str]
    wants: Mapped[str]
    secret: Mapped[str]
    knows: Mapped[str]
    stats: Mapped[str]  # free text, e.g. "Commoner"
    stat_ref: Mapped[str]  # "srd:monsters/commoner" or "monster:<id>", for the tracker and stat block
    notes: Mapped[str]
    tags: Mapped[list[Any]]


class Monster(Record):
    """A homebrew stat block. Campaign-less monsters are shared by every campaign."""

    __tablename__ = "monsters"

    campaign_id: Mapped[str | None] = _campaign_fk("CASCADE")
    name: Mapped[str]
    kind: Mapped[str]  # "Medium undead, neutral evil"
    cr: Mapped[str]
    xp: Mapped[int]
    ac: Mapped[int]
    hp: Mapped[int]
    hp_roll: Mapped[str]
    init_bonus: Mapped[int]
    speed: Mapped[str]
    abilities: Mapped[dict[str, Any]]
    body: Mapped[str]  # traits, actions etc. in Markdown
    notes: Mapped[str]


class Note(Record):
    __tablename__ = "notes"

    campaign_id: Mapped[str | None] = _campaign_fk("CASCADE")
    session_id: Mapped[str | None] = _fk("sessions")
    kind: Mapped[str]  # general | session | quest | pin
    title: Mapped[str]
    body: Mapped[str]
    pinned: Mapped[bool]
    done: Mapped[bool]


class Encounter(Record):
    __tablename__ = "encounters"

    campaign_id: Mapped[str | None] = _campaign_fk("CASCADE")
    session_id: Mapped[str | None] = _fk("sessions")
    scene_id: Mapped[str | None] = _fk("scenes")
    name: Mapped[str]
    round: Mapped[int]
    turn: Mapped[int]
    active: Mapped[bool]
    combatants: Mapped[list[Any]]
    party_size: Mapped[int]
    party_level: Mapped[int]
    log: Mapped[list[Any]]
    notes: Mapped[str]


class Tracker(Record):
    """A clock or counter, e.g. "Tolls 3 of 7" with a label for each step."""

    __tablename__ = "trackers"

    campaign_id: Mapped[str | None] = _campaign_fk("CASCADE")
    session_id: Mapped[str | None] = _fk("sessions")
    name: Mapped[str]
    value: Mapped[int]
    max: Mapped[int]
    steps: Mapped[list[Any]]
    notes: Mapped[str]


class Handout(Record):
    """An image (map, letter, portrait) stored as a file next to the database."""

    __tablename__ = "handouts"

    campaign_id: Mapped[str | None] = _campaign_fk("CASCADE")
    folder: Mapped[str]
    title: Mapped[str]
    filename: Mapped[str]  # stored file name, or "" before an image is uploaded
    mime: Mapped[str]
    body: Mapped[str]
