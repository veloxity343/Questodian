"""Import and export: JSON bundles, single records, and Markdown."""

from __future__ import annotations

import base64
import re
from pathlib import Path

from . import models, rules
from .repo import KINDS, LINKS, SINGULAR, Repo, normalize, valid_id

FORMAT = "questodian"
VERSION = 2  # 2: DM screen records; sessions no longer hold an encounter
IMPORT_MODES = ("overwrite", "skip", "copy")


class TransferError(ValueError):
    pass


# --------------------------------------------------------------------------- export


def export_bundle(repo: Repo, campaign_id: str | None = None, files_dir: Path | None = None) -> dict:
    """Everything, or one campaign together with the records linked to it.

    With ``files_dir``, handout images are embedded (base64) so the bundle is self-contained.
    """
    if campaign_id:
        campaign = repo.get("campaigns", campaign_id)
        linked_chars = {p.get("character_id") for p in campaign["players"] if p.get("character_id")}
        collections = {"campaigns": [campaign]}
        for kind in KINDS[1:]:
            docs = repo.list(kind, campaign_id=campaign_id)
            if kind == "characters":
                docs = [c for c in repo.list("characters") if c["campaign_id"] == campaign_id or c["id"] in linked_chars]
            collections[kind] = docs
    else:
        collections = {kind: repo.list(kind) for kind in KINDS}
    if files_dir is not None:
        for h in collections.get("handouts", []):
            path = files_dir / h["filename"] if h["filename"] else None
            if path and path.is_file():
                h["file_b64"] = base64.b64encode(path.read_bytes()).decode("ascii")
    return {"format": FORMAT, "version": VERSION, "exported": models.now(), "collections": collections}


def export_one(repo: Repo, kind: str, doc_id: str) -> dict:
    return {
        "format": FORMAT,
        "version": VERSION,
        "exported": models.now(),
        "kind": SINGULAR[kind],
        "data": repo.get(kind, doc_id),
    }


# --------------------------------------------------------------------------- import

_KIND_BY_SINGULAR = {v: k for k, v in SINGULAR.items()}


def _collections_from(payload) -> dict[str, list[dict]]:
    if not isinstance(payload, dict):
        raise TransferError("expected a JSON object")
    if payload.get("format") not in (None, FORMAT):
        raise TransferError(f"not a Questodian file (format={payload.get('format')!r})")
    if int(payload.get("version") or VERSION) > VERSION:
        raise TransferError("this file was made by a newer version of Questodian")
    if "collections" in payload:
        cols = payload["collections"]
        if not isinstance(cols, dict):
            raise TransferError("'collections' must be an object")
        out = {}
        for kind, docs in cols.items():
            if kind not in KINDS:
                continue
            if not isinstance(docs, list):
                raise TransferError(f"'{kind}' must be a list")
            out[kind] = [d for d in docs if isinstance(d, dict)]
        return _upgrade_v1(out)
    if "kind" in payload and "data" in payload:
        kind = _KIND_BY_SINGULAR.get(payload["kind"]) or (payload["kind"] if payload["kind"] in KINDS else None)
        if not kind:
            raise TransferError(f"unknown record kind {payload['kind']!r}")
        docs = payload["data"] if isinstance(payload["data"], list) else [payload["data"]]
        return _upgrade_v1({kind: [d for d in docs if isinstance(d, dict)]})
    raise TransferError("file has neither 'collections' nor 'kind'/'data'")


def _upgrade_v1(cols: dict[str, list[dict]]) -> dict[str, list[dict]]:
    """Version 1 sessions carried one `encounter`; turn those into encounter records."""
    for s in cols.get("sessions", []):
        enc = s.get("encounter")
        if isinstance(enc, dict) and enc.get("combatants"):
            cols.setdefault("encounters", []).append({
                **{k: enc.get(k) for k in ("round", "turn", "active", "combatants") if k in enc},
                "id": f"{s.get('id') or models.new_id()}-enc"[:64],
                "campaign_id": s.get("campaign_id", ""),
                "session_id": s.get("id", ""),
                "name": f"Session {s.get('number', '')}: {s.get('title', '')}".strip(),
            })
    return cols


def import_data(repo: Repo, payload, mode: str = "overwrite", files_dir: Path | None = None) -> dict:
    """Import a bundle or single-record export.

    ``overwrite`` keeps ids and replaces existing records, ``skip`` keeps ids but
    leaves existing records alone, and ``copy`` gives every imported record a
    fresh id (rewriting the links between them) so nothing is ever replaced.
    Embedded handout images are written to ``files_dir``.
    """
    if mode not in IMPORT_MODES:
        raise TransferError(f"mode must be one of {', '.join(IMPORT_MODES)}")
    cols = _collections_from(payload)
    images = {}
    for h in cols.get("handouts", []):
        if h.get("file_b64"):
            try:
                images[id(h)] = base64.b64decode(h["file_b64"], validate=True)
            except ValueError as exc:
                raise TransferError(f"handout {h.get('title', '')!r} has a broken image") from exc
    # validate everything before writing anything
    try:
        docs = {kind: [(normalize(kind, d), images.get(id(d))) for d in cols.get(kind, [])] for kind in KINDS}
    except ValueError as exc:
        raise TransferError(f"invalid record: {exc}") from exc

    if mode == "copy":
        _remap_ids({kind: [d for d, _ in pairs] for kind, pairs in docs.items()})

    summary = {"imported": {}, "skipped": {}, "mode": mode}
    for kind in KINDS:
        n_in = n_skip = 0
        for d, image in docs[kind]:
            if not valid_id(d["id"]):
                d["id"] = models.new_id()
            if mode == "skip" and repo.exists(kind, d["id"]):
                n_skip += 1
                continue
            # linked kinds are written first, so a link that still dangles points outside this import
            for field, target in LINKS.items():
                if d.get(field) and not repo.exists(target, d[field]):
                    d[field] = ""
            if kind == "handouts":
                d["filename"] = _store_image(files_dir, d, image) if image else ""
            repo.save(kind, d, keep_timestamps=True)
            n_in += 1
        if n_in:
            summary["imported"][kind] = n_in
        if n_skip:
            summary["skipped"][kind] = n_skip
    return summary


def _store_image(files_dir: Path | None, doc: dict, data: bytes) -> str:
    if files_dir is None:
        return ""
    files_dir.mkdir(parents=True, exist_ok=True)
    ext = Path(doc.get("filename") or "").suffix or ".png"
    name = f"{doc['id']}-{models.new_id()[:6]}{ext}"
    (files_dir / name).write_bytes(data)
    return name


def _remap_ids(docs: dict[str, list[dict]]) -> None:
    """Give every record a new id and rewrite the references between them."""
    remap: dict[str, str] = {}
    for kind in KINDS:
        for d in docs[kind]:
            old = d.get("id") or models.new_id()
            d["id"] = remap[old] = models.new_id()

    def swap(value: str) -> str:
        return remap.get(value, value)

    for kind in KINDS:
        for d in docs[kind]:
            for field in LINKS:
                if d.get(field):
                    d[field] = swap(d[field])
    for c in docs["campaigns"]:
        for p in c["players"]:
            p["character_id"] = swap(p["character_id"])
    for s in docs["sessions"]:
        s["attendance"] = [swap(a) for a in s["attendance"]]
    for e in docs["encounters"]:
        for cb in e["combatants"]:
            for field in ("character_id", "monster_id", "npc_id"):
                if cb.get(field):
                    cb[field] = swap(cb[field])
    for n in docs["npcs"]:
        if n["stat_ref"].startswith("monster:"):
            n["stat_ref"] = "monster:" + swap(n["stat_ref"].split(":", 1)[1])


# --------------------------------------------------------------------------- markdown


def markdown_to_handbook(text: str, filename: str = "") -> dict:
    """Turn a Markdown document into a handbook entry (title = first heading)."""
    title = ""
    body = text.replace("\r\n", "\n")
    m = re.match(r"\s*#\s+(.+?)\s*\n", body)
    if m:
        title = m.group(1).strip()
        body = body[m.end():]
    if not title:
        title = re.sub(r"\.(md|markdown|txt)$", "", filename or "", flags=re.I).replace("_", " ").strip() or "Imported"
    return {"title": title, "body": body.strip(), "category": "Imported"}


def _signed(n: int) -> str:
    return f"+{n}" if n >= 0 else str(n)


def character_markdown(c: dict, edition: str = "2014") -> str:
    d = rules.derive(c, edition)
    classes = " / ".join(
        f"{k['name']}{' (' + k['subclass'] + ')' if k.get('subclass') else ''} {k['level']}" for k in c["classes"]
    )
    out = [
        f"# {c['name']}",
        "",
        f"*{' '.join(x for x in [c['race'], classes] if x)}*"
        + (f" — {c['background']}" if c["background"] else "")
        + (f", {c['alignment']}" if c["alignment"] else ""),
        "",
        f"**Player:** {c['player'] or '—'} · **Level** {d['level']} · **XP** {c['xp']} · "
        f"**Proficiency Bonus** {_signed(d['proficiency_bonus'])}",
        "",
        f"**AC** {c['ac']} · **Initiative** {_signed(d['initiative'])} · **Speed** {c['speed']} ft. · "
        f"**HP** {c['hp']['current']}/{c['hp']['max']}" + (f" (+{c['hp']['temp']} temp)" if c["hp"]["temp"] else "")
        + f" · **Hit Dice** {c['hit_dice']['remaining']} of {c['hit_dice']['total']}",
        "",
        "| " + " | ".join(a.upper() for a in rules.ABILITIES) + " |",
        "|" + ":---:|" * 6,
        "| " + " | ".join(f"{c['abilities'][a]} ({_signed(d['modifiers'][a])})" for a in rules.ABILITIES) + " |",
        "",
        "**Saving Throws:** "
        + ", ".join(
            f"{rules.ABILITY_NAMES[a]} {_signed(d['saves'][a])}{'*' if a in c['save_proficiencies'] else ''}"
            for a in rules.ABILITIES
        ),
        "",
        "**Skills:** "
        + ", ".join(
            f"{s.replace('-', ' ').title()} {_signed(v)}"
            + {"proficient": "*", "expertise": "**", "half": "½"}.get(c["skills"].get(s, "none"), "")
            for s, v in d["skills"].items()
        ),
        "",
        f"**Passive Perception** {d['passive_perception']} · **Passive Investigation** "
        f"{d['passive_investigation']} · **Passive Insight** {d['passive_insight']}",
    ]
    profs = c["proficiencies"]
    prof_lines = [f"**{k.title()}:** {v}" for k, v in profs.items() if v]
    if prof_lines:
        out += ["", "## Proficiencies", "", *[p + "  " for p in prof_lines]]
    if c["attacks"]:
        out += ["", "## Attacks", "", "| Name | Bonus | Damage | Notes |", "|---|---|---|---|"]
        out += [f"| {a.get('name', '')} | {a.get('bonus', '')} | {a.get('damage', '')} | {a.get('notes', '')} |" for a in c["attacks"]]
    sc = c["spellcasting"]
    if sc.get("ability") or sc.get("spells"):
        out += ["", "## Spellcasting", ""]
        if d["spell"]:
            out.append(
                f"**Ability** {rules.ABILITY_NAMES.get(sc['ability'], sc['ability'])} · "
                f"**Save DC** {d['spell']['save_dc']} · **Attack** {_signed(d['spell']['attack_bonus'])}"
            )
        slots = [f"{lvl}: {s.get('max', 0) - s.get('used', 0)}/{s.get('max', 0)}" for lvl, s in sorted(sc["slots"].items()) if s.get("max")]
        if slots:
            out.append("")
            out.append("**Slots** " + " · ".join(slots))
        by_level: dict[int, list[str]] = {}
        for sp in sc["spells"]:
            by_level.setdefault(int(sp.get("level") or 0), []).append(
                sp.get("name", "") + (" ◆" if sp.get("prepared") else "")
            )
        for lvl in sorted(by_level):
            out.append("")
            out.append(f"**{'Cantrips' if lvl == 0 else f'Level {lvl}'}:** {', '.join(by_level[lvl])}")
    if c["resources"]:
        out += ["", "## Resources", ""]
        out += [f"- **{r.get('name', '')}:** {r.get('current', 0)}/{r.get('max', 0)} ({r.get('reset', '')})" for r in c["resources"]]
    if c["features"]:
        out += ["", "## Features & Traits"]
        for f in c["features"]:
            src = f" *({f['source']})*" if f.get("source") else ""
            out += ["", f"**{f.get('name', '')}**{src}. {f.get('description', '')}"]
    if c["inventory"] or any(c["currency"].values()):
        out += ["", "## Equipment", ""]
        out += [
            f"- {i.get('name', '')}" + (f" ×{i['qty']}" if i.get("qty", 1) != 1 else "") + (" (equipped)" if i.get("equipped") else "")
            for i in c["inventory"]
        ]
        coins = ", ".join(f"{v} {k}" for k, v in c["currency"].items() if v)
        if coins:
            out += ["", f"**Coins:** {coins}"]
    pers = c["personality"]
    if any(pers.values()):
        out += ["", "## Personality", ""]
        out += [f"**{k.title()}:** {v}  " for k, v in pers.items() if v]
    for key, label in (("appearance", "Appearance"), ("backstory", "Backstory"), ("notes", "Notes")):
        if c[key]:
            out += ["", f"## {label}", "", c[key]]
    return "\n".join(out) + "\n"


def session_markdown(s: dict, campaign_name: str = "") -> str:
    out = [f"# Session {s['number']}: {s['title']}", ""]
    meta = [x for x in [campaign_name, s["date"], s["status"]] if x]
    if meta:
        out += [" · ".join(meta), ""]
    if s["recap"]:
        out += ["## Recap", "", s["recap"], ""]
    if s["prep"]:
        out += ["## Prep", "", s["prep"], ""]
    if s["log"]:
        out += ["## Log", ""]
        out += [f"- `{e.get('time', '')[11:16]}` **{e.get('kind', 'note')}** {e.get('text', '')}" for e in s["log"]]
        out.append("")
    if s["loot"]:
        out += ["## Loot", ""]
        out += [
            f"- {item.get('name', '')}" + (f" ×{item['qty']}" if item.get("qty", 1) != 1 else "")
            + (f" → {item['assigned_to']}" if item.get("assigned_to") else "")
            for item in s["loot"]
        ]
        out.append("")
    if s["xp_awarded"]:
        out += [f"**XP awarded:** {s['xp_awarded']}", ""]
    return "\n".join(out)
