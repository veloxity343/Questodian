"""Record access on top of the ORM.

The rest of the app works with plain dicts in the shape of :mod:`schemas`;
this module validates them and maps them to and from ORM rows.
"""

from __future__ import annotations

import re

from pydantic import BaseModel
from sqlalchemy import func, select
from sqlalchemy.orm import Session

from . import models, schemas

# Order matters: records are imported after the records they link to.
KINDS = (
    "campaigns", "sessions", "characters", "handbook",
    "scenes", "npcs", "monsters", "notes", "encounters", "trackers", "handouts",
)
SINGULAR = {
    "campaigns": "campaign", "sessions": "session", "characters": "character", "handbook": "handbook",
    "scenes": "scene", "npcs": "npc", "monsters": "monster", "notes": "note",
    "encounters": "encounter", "trackers": "tracker", "handouts": "handout",
}
TABLES: dict[str, type[models.Record]] = {
    "campaigns": models.Campaign,
    "sessions": models.Session,
    "characters": models.Character,
    "handbook": models.HandbookEntry,
    "scenes": models.Scene,
    "npcs": models.Npc,
    "monsters": models.Monster,
    "notes": models.Note,
    "encounters": models.Encounter,
    "trackers": models.Tracker,
    "handouts": models.Handout,
}
SCHEMAS: dict[str, type[schemas.Record]] = {
    "campaigns": schemas.Campaign,
    "sessions": schemas.Session,
    "characters": schemas.Character,
    "handbook": schemas.HandbookEntry,
    "scenes": schemas.Scene,
    "npcs": schemas.Npc,
    "monsters": schemas.Monster,
    "notes": schemas.Note,
    "encounters": schemas.Encounter,
    "trackers": schemas.Tracker,
    "handouts": schemas.Handout,
}
# Link fields (stored as NULL when empty) and the kind they point to.
LINKS = {"campaign_id": "campaigns", "session_id": "sessions", "scene_id": "scenes"}
# Fields `list` can filter on.
FILTERS = ("campaign_id", "session_id", "scene_id", "kind", "folder")


def _order(kind: str, t) -> list:
    match kind:
        case "sessions":
            return [t.campaign_id, t.number]
        case "scenes":
            return [t.session_id, t.position, func.lower(t.title)]
        case "handbook":
            return [func.lower(t.title)]
        case "notes":
            return [t.pinned.desc(), t.updated.desc()]
        case "encounters":
            return [t.created]
        case "handouts":
            return [func.lower(t.folder), func.lower(t.title)]
        case _:
            return [func.lower(t.name)]
_ID_RE = re.compile(r"^[A-Za-z0-9_-]{1,64}$")


class NotFound(LookupError):
    pass


def check_kind(kind: str) -> None:
    if kind not in KINDS:
        raise ValueError(f"unknown kind {kind!r}")


def valid_id(doc_id: str) -> bool:
    return bool(_ID_RE.match(doc_id or ""))


def normalize(kind: str, data: dict) -> dict:
    """Validate a record and fill in defaults for anything missing."""
    check_kind(kind)
    if not isinstance(data, dict):
        raise ValueError(f"{SINGULAR[kind]} must be an object")
    return SCHEMAS[kind].model_validate(data).model_dump()


def title_of(kind: str, doc: dict) -> str:
    return doc.get("title") or doc.get("name") or ""


def _fields(kind: str) -> list[str]:
    return [f for f in SCHEMAS[kind].model_fields if f not in ("id", "created", "updated")]


class Repo:
    def __init__(self, db: Session):
        self.db = db

    # ------------------------------------------------------------------ mapping

    def _to_dict(self, kind: str, row: models.Record) -> dict:
        data = {f: getattr(row, f) for f in ("id", "created", "updated", *_fields(kind))}
        return SCHEMAS[kind].model_validate(data).model_dump()

    def _row(self, kind: str, doc_id: str) -> models.Record:
        check_kind(kind)
        row = self.db.get(TABLES[kind], doc_id) if valid_id(doc_id) else None
        if row is None:
            raise NotFound(f"{SINGULAR[kind]} {doc_id} not found")
        return row

    # ------------------------------------------------------------------ reads

    def list(self, kind: str, /, **filters: str | None) -> list[dict]:
        """All records of a kind, optionally filtered by FILTERS fields (empty values are ignored)."""
        check_kind(kind)
        table = TABLES[kind]
        stmt = select(table)
        for field, value in filters.items():
            if field not in FILTERS:
                raise ValueError(f"can't filter on {field!r}")
            if value and hasattr(table, field):
                stmt = stmt.where(getattr(table, field) == value)
        return [self._to_dict(kind, row) for row in self.db.scalars(stmt.order_by(*_order(kind, table)))]

    def get(self, kind: str, doc_id: str) -> dict:
        return self._to_dict(kind, self._row(kind, doc_id))

    def exists(self, kind: str, doc_id: str) -> bool:
        check_kind(kind)
        return valid_id(doc_id) and self.db.get(TABLES[kind], doc_id) is not None

    # ------------------------------------------------------------------ writes

    def save(self, kind: str, data: dict | BaseModel, *, keep_timestamps: bool = False) -> dict:
        """Insert or replace a whole record."""
        if isinstance(data, BaseModel):
            data = data.model_dump()
        doc = normalize(kind, data)
        doc["id"] = doc["id"] or models.new_id()
        if not valid_id(doc["id"]):
            raise ValueError(f"invalid id {doc['id']!r}")
        for field, target in LINKS.items():
            ref = doc.get(field)
            if ref and not self.exists(target, ref):
                raise ValueError(f"{SINGULAR[target]} {ref} not found")
        row = self.db.get(TABLES[kind], doc["id"])
        if row is None:
            row = TABLES[kind](id=doc["id"])
            self.db.add(row)
        stamp = models.now()
        row.created = doc["created"] or row.created or stamp
        row.updated = doc["updated"] if keep_timestamps and doc["updated"] else stamp
        for f in _fields(kind):
            value = doc[f]
            setattr(row, f, (value or None) if f in LINKS else value)
        self.db.flush()
        return self._to_dict(kind, row)

    def create(self, kind: str, data: dict | None = None) -> dict:
        data = {**(data or {}), "id": models.new_id()}
        data.pop("created", None)
        return self.save(kind, data)

    def update(self, kind: str, doc_id: str, data: dict) -> dict:
        """Merge ``data`` over the existing record's top-level fields."""
        existing = self.get(kind, doc_id)
        return self.save(kind, {**existing, **data, "id": doc_id, "created": existing["created"]})

    def delete(self, kind: str, doc_id: str) -> None:
        # Sessions cascade with their campaign; characters and handbook entries
        # are unlinked (both handled by the foreign keys).
        self.db.delete(self._row(kind, doc_id))
        self.db.flush()
        self.db.expire_all()
