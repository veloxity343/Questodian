"""Bundled System Reference Document content, per rules edition: loading and search.

``"2014"`` is SRD 5.1 (the 2014 rules) and ``"2024"`` is SRD 5.2.1 (the 2024 rules).
"""

from __future__ import annotations

import gzip
import json
import re
from functools import cache
from importlib import resources

EDITIONS = {"2014": "data/srd.json.gz", "2024": "data/srd-2024.json.gz"}
EDITION_LABELS = {"2014": "5e (2014)", "2024": "5.5e (2024)"}
DEFAULT_EDITION = "2014"

CATEGORY_LABELS = {
    "rules": "Rules",
    "conditions": "Conditions",
    "abilities": "Ability Scores",
    "skills": "Skills",
    "classes": "Classes",
    "races": "Races",
    "species": "Species",
    "backgrounds": "Backgrounds",
    "feats": "Feats",
    "spells": "Spells",
    "monsters": "Monsters",
    "equipment": "Equipment",
    "magic-items": "Magic Items",
    "weapon-mastery": "Weapon Mastery",
    "weapon-properties": "Weapon Properties",
    "poisons": "Poisons",
    "damage-types": "Damage Types",
    "magic-schools": "Schools of Magic",
    "alignments": "Alignments",
    "languages": "Languages",
}


def check_edition(edition: str | None) -> str:
    edition = edition or DEFAULT_EDITION
    if edition not in EDITIONS:
        raise ValueError(f"unknown rules edition {edition!r} (use {' or '.join(EDITIONS)})")
    return edition


@cache
def _load(edition: str = DEFAULT_EDITION) -> dict:
    raw = resources.files("questodian").joinpath(EDITIONS[check_edition(edition)]).read_bytes()
    data = json.loads(gzip.decompress(raw))
    data["by_id"] = {e["id"]: e for e in data["entries"]}
    for e in data["entries"]:
        e["_name"] = e["name"].lower()
        e["_text"] = " ".join([e["name"], e["summary"], " ".join(e["tags"]), e["body"]]).lower()
    return data


def meta(edition: str = DEFAULT_EDITION) -> dict:
    return {**_load(edition)["meta"], "edition": edition, "edition_label": EDITION_LABELS[edition]}


def categories(edition: str = DEFAULT_EDITION) -> list[dict]:
    counts: dict[str, int] = {}
    for e in _load(edition)["entries"]:
        counts[e["category"]] = counts.get(e["category"], 0) + 1
    return [{"id": k, "label": CATEGORY_LABELS.get(k, k), "count": counts.get(k, 0)} for k in CATEGORY_LABELS if k in counts]


def _public(e: dict, full: bool) -> dict:
    out = {k: v for k, v in e.items() if not k.startswith("_")}
    if not full:
        out.pop("body", None)
    return out


def get(entry_id: str, edition: str = DEFAULT_EDITION) -> dict | None:
    e = _load(edition)["by_id"].get(entry_id)
    return _public(e, True) if e else None


def search(
    query: str = "",
    category: str = "",
    limit: int = 50,
    offset: int = 0,
    edition: str = DEFAULT_EDITION,
    **filters,
) -> dict:
    """Rank entries: exact name > name prefix > name contains > text contains.

    ``filters`` match against an entry's structured ``data`` (e.g. ``level=3``
    for spells, ``cr_label="1/4"`` for monsters, ``classes="Wizard"``).
    """
    q = query.strip().lower()
    words = [w for w in re.split(r"\s+", q) if w]
    results = []
    for e in _load(edition)["entries"]:
        if category and e["category"] != category:
            continue
        if not _match_filters(e["data"], filters):
            continue
        if not words:
            results.append((9, e["_name"], e))
            continue
        name = e["_name"]
        if name == q:
            rank = 0
        elif name.startswith(q):
            rank = 1
        elif q in name:
            rank = 2
        elif all(w in name for w in words):
            rank = 3
        elif all(w in e["_text"] for w in words):
            rank = 5
        else:
            continue
        results.append((rank, name, e))
    results.sort(key=lambda r: (r[0], r[1]))
    page = results[offset : offset + limit]
    return {"total": len(results), "results": [_public(e, False) for _, _, e in page]}


def _match_filters(data: dict, filters: dict) -> bool:
    for key, want in filters.items():
        if want in (None, ""):
            continue
        have = data.get(key)
        if isinstance(have, list):
            if str(want).lower() not in (str(h).lower() for h in have):
                return False
        elif str(have).lower() != str(want).lower():
            return False
    return True
