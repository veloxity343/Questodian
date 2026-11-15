"""Build the bundled SRD 5.2.1 (D&D 2024 rules) reference data.

Usage:
    git clone --depth 1 https://github.com/5e-bits/5e-database.git
    uv run python scripts/build_srd_2024.py path/to/5e-database path/to/SRD-5.2.1.md

Structured data (monsters, spells, classes, species, equipment...) comes from
5e-bits/5e-database (src/2024/en). The rules prose (Playing the Game, the Rules
Glossary and the Gameplay Toolbox) isn't in that dataset, so it is split out of
a Markdown conversion of the SRD 5.2.1 PDF.

Writes src/pocket_dm/data/srd-2024.json.gz in the same format as build_srd.py.
"""

from __future__ import annotations

import gzip
import json
import re
import sys
from collections import defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from build_srd import ABILITIES, entry, fmt_cr, mod, names, ordinal  # noqa: E402

OUT = Path(__file__).resolve().parent.parent / "src" / "pocket_dm" / "data" / "srd-2024.json.gz"

ATTRIBUTION = (
    "This work includes material from the System Reference Document 5.2.1 "
    '("SRD 5.2.1") by Wizards of the Coast LLC, available at https://www.dndbeyond.com/srd. '
    "The SRD 5.2.1 is licensed under the Creative Commons Attribution 4.0 International "
    "License, available at https://creativecommons.org/licenses/by/4.0/legalcode."
)


def text(desc) -> str:
    """5e-bits 2024 uses `description` strings with "\\n  " paragraph breaks."""
    if desc is None:
        return ""
    if isinstance(desc, list):
        desc = "\n".join(desc)
    return re.sub(r"\n\s*", "\n\n", str(desc).strip())


def load(src: Path, name: str):
    return json.loads((src / f"5e-SRD-{name}.json").read_text(encoding="utf-8"))


# --------------------------------------------------------------------------- simple lists


def build_simple(src, file, category, summary_fn=None, name_fn=None):
    out = []
    for item in load(src, file):
        body = text(item.get("description") or item.get("desc"))
        name = name_fn(item) if name_fn else item.get("full_name") or item["name"]
        out.append(entry(category, item["index"], name, body, summary_fn(item) if summary_fn else ""))
    return out


def build_skills(src):
    out = []
    for s in load(src, "Skills"):
        ability = s["ability_score"]["name"]
        out.append(entry("skills", s["index"], s["name"], text(s.get("description")), ability, [ability],
                         {"ability": ability.lower()}))
    return out


def build_languages(src):
    out = []
    for lang in load(src, "Languages"):
        kind = "Rare language" if lang.get("is_rare") else "Standard language"
        body = f"*{kind}*" + (f"\n\n{lang['note']}" if lang.get("note") else "")
        out.append(entry("languages", lang["index"], lang["name"], body, kind))
    return out


# --------------------------------------------------------------------------- spells


def build_spells(src):
    out = []
    for s in load(src, "Spells"):
        lvl = s["level"]
        school = s["school"]["name"]
        head = f"{school} cantrip" if lvl == 0 else f"Level {lvl} {school}"
        classes = [c["name"] for c in s.get("classes", [])]
        head += f" ({', '.join(classes)})" if classes else ""
        comps = ", ".join(s.get("components", []))
        if s.get("material"):
            comps += f" ({s['material']})"
        time = s.get("casting_time", "") + (" or Ritual" if s.get("ritual") else "")
        duration = s.get("duration", "")
        if s.get("concentration") and not duration.lower().startswith("concentration"):
            duration = f"Concentration, {duration}"
        body = (
            f"*{head}*\n\n"
            f"**Casting Time:** {time}  \n"
            f"**Range:** {s.get('range', '')}  \n"
            f"**Components:** {comps}  \n"
            f"**Duration:** {duration}\n\n"
            f"{text(s.get('description'))}"
        )
        if s.get("higher_level"):
            label = "Cantrip Upgrade" if lvl == 0 else "Using a Higher-Level Spell Slot"
            body += f"\n\n***{label}.*** {text(s['higher_level'])}"
        summary = f"{school} cantrip" if lvl == 0 else f"Level {lvl} {school.lower()}"
        out.append(
            entry(
                "spells", s["index"], s["name"], body, summary,
                [school, *classes, "ritual" if s.get("ritual") else "", "concentration" if s.get("concentration") else ""],
                {
                    "level": lvl, "school": school, "classes": classes,
                    "ritual": bool(s.get("ritual")), "concentration": bool(s.get("concentration")),
                    "casting_time": s.get("casting_time", ""), "range": s.get("range", ""),
                    "duration": s.get("duration", ""), "components": s.get("components", []),
                },
            )
        )
    return out


# --------------------------------------------------------------------------- monsters


def _signed(n: int) -> str:
    return f"+{n}" if n >= 0 else f"−{abs(n)}"


def build_monsters(src):
    out = []
    for m in load(src, "Monsters"):
        ac = m["armor_class"][0]["value"] if m.get("armor_class") else 10
        speed = ", ".join(
            (str(v) if k == "walk" else f"{k.title()} {v}") for k, v in m.get("speed", {}).items() if k != "hover"
        )
        if m.get("speed", {}).get("hover"):
            speed += " (hover)"
        kind = f"{m['size']} {m['type'].title()}, {m['alignment'].title()}"
        cr = fmt_cr(m["challenge_rating"])
        dex_mod = (m["dexterity"] - 10) // 2
        saves = {
            p["proficiency"]["index"].split("-")[-1]: p["value"]
            for p in m.get("proficiencies", [])
            if p["proficiency"]["index"].startswith("saving-throw")
        }
        skills = [p for p in m.get("proficiencies", []) if p["proficiency"]["index"].startswith("skill")]

        lines = [f"*{kind}*", ""]
        lines.append(f"**AC** {ac} · **Initiative** {_signed(dex_mod)} ({10 + dex_mod}) · "
                     f"**HP** {m['hit_points']} ({m.get('hit_points_roll') or m.get('hit_dice')}) · **Speed** {speed}")
        lines += ["", "| | Score | Mod | Save |", "|---|:---:|:---:|:---:|"]
        for a in ABILITIES:
            short = a[:3]
            score = m[a]
            m_ = (score - 10) // 2
            lines.append(f"| **{short.upper()}** | {score} | {_signed(m_)} | {_signed(saves.get(short, m_))} |")
        lines.append("")
        if skills:
            lines.append("**Skills** " + ", ".join(
                f"{p['proficiency']['name'].split(': ')[1]} {_signed(p['value'])}" for p in skills) + "  ")
        for key, label in [("damage_vulnerabilities", "Vulnerabilities"), ("damage_resistances", "Resistances")]:
            if m.get(key):
                lines.append(f"**{label}** {', '.join(v.title() for v in m[key])}  ")
        immune = [v.title() for v in m.get("damage_immunities", [])]
        cond = [c["name"] for c in m.get("condition_immunities", [])]
        if immune or cond:
            lines.append("**Immunities** " + "; ".join(x for x in [", ".join(immune), ", ".join(cond)] if x) + "  ")
        if m.get("gear"):
            lines.append("**Gear** " + ", ".join(g if isinstance(g, str) else g.get("name") or (g.get("equipment") or {}).get("name", "")
                                     for g in m["gear"]) + "  ")
        senses = m.get("senses", {})
        sense_txt = ", ".join(f"{k.replace('_', ' ').title()} {v}" for k, v in senses.items() if k != "passive_perception")
        pp = senses.get("passive_perception")
        lines.append("**Senses** " + "; ".join(x for x in [sense_txt, f"Passive Perception {pp}" if pp else ""] if x) + "  ")
        lines.append(f"**Languages** {m.get('languages') or 'None'}  ")
        xp = f"XP {m.get('xp', 0):,}" + (f", or {m['xp_in_lair']:,} in lair" if m.get("xp_in_lair") else "")
        lines.append(f"**CR** {cr} ({xp}; PB +{m.get('proficiency_bonus', 2)})")
        for key, label in [
            ("special_abilities", "Traits"), ("actions", "Actions"), ("bonus_actions", "Bonus Actions"),
            ("reactions", "Reactions"), ("legendary_actions", "Legendary Actions"),
        ]:
            items = m.get(key) or []
            if items:
                lines.append(f"\n### {label}")
                for a in items:
                    lines.append(f"\n***{a['name']}.*** {text(a.get('desc'))}")
        out.append(
            entry(
                "monsters", m["index"], m["name"], "\n".join(lines),
                f"CR {cr} {m['size'].lower()} {m['type']}",
                [m["type"], m["size"], f"CR {cr}"],
                {
                    "cr": m["challenge_rating"], "cr_label": cr, "xp": m.get("xp", 0), "type": m["type"],
                    "size": m["size"], "ac": ac, "hp": m["hit_points"],
                    "hp_roll": m.get("hit_points_roll") or m.get("hit_dice"), "init_bonus": dex_mod,
                    "abilities": {a[:3]: m[a] for a in ABILITIES},
                },
            )
        )
    return out


# --------------------------------------------------------------------------- character options


def build_classes(src):
    features = load(src, "Features")
    levels = defaultdict(list)
    for lv in load(src, "Levels"):
        if "subclass" not in lv:
            levels[lv["class"]["index"]].append(lv)
    subclasses = defaultdict(list)
    for sc in load(src, "Subclasses"):
        subclasses[sc["class"]["index"]].append(sc)

    out = []
    for c in load(src, "Classes"):
        idx = c["index"]
        saves = [s["name"] for s in c.get("saving_throws", [])] or [
            p["name"].split(": ")[1] for p in c.get("proficiencies", []) if p["index"].startswith("saving-throw")
        ]
        profs = [p["name"] for p in c.get("proficiencies", []) if not p["index"].startswith("saving-throw")]
        lines = [
            f"**Primary Ability:** {(c.get('primary_ability') or {}).get('desc', '—')}  ",
            f"**Hit Point Die:** d{c['hit_die']} per {c['name']} level  ",
            f"**Saving Throw Proficiencies:** {', '.join(saves)}  ",
            f"**Proficiencies:** {', '.join(profs) or '—'}  ",
        ]
        for choice in c.get("proficiency_choices", []):
            if choice.get("desc"):
                lines.append(f"**Skills:** {choice['desc']}  ")
        for opt in c.get("starting_equipment_options", []):
            if opt.get("desc"):
                lines.append(f"**Starting Equipment:** {opt['desc']}  ")
        rows = sorted(levels[idx], key=lambda lv: lv["level"])
        casts = any(lv.get("spellcasting") for lv in rows)
        header, sep = "| Level | PB | Features |", "|---:|:---:|---|"
        if casts:
            header += " Cantrips | Prepared | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 |"
            sep += "---:|---:|" + "---:|" * 9
        lines += ["", f"### {c['name']} Features", "", header, sep]
        for lv in rows:
            feats = ", ".join(f["name"] for f in lv.get("features", [])) or "—"
            row = f"| {lv['level']} | +{lv.get('prof_bonus', '')} | {feats} |"
            if casts:
                sp = lv.get("spellcasting") or {}
                row += f" {sp.get('cantrips_known', '—')} | {sp.get('prepared_spells', '—')} |" + "".join(
                    f" {sp.get(f'spell_slots_level_{i}', 0) or '—'} |" for i in range(1, 10))
            lines.append(row)
        own = sorted((f for f in features if f.get("class", {}).get("index") == idx and not f.get("subclass")),
                     key=lambda f: int(re.sub(r"\D", "", f["level"]["index"]) or 0) if isinstance(f.get("level"), dict) else f.get("level", 0))
        seen = set()
        for f in own:
            if f["name"] in seen:
                continue
            seen.add(f["name"])
            lv = f["level"]["name"].split()[-1] if isinstance(f.get("level"), dict) else f.get("level", "")
            lines.append(f"\n#### Level {lv}: {f['name']}\n\n{text(f.get('description'))}")
        for sc in subclasses.get(idx, []):
            lines.append(f"\n### {sc['name']}" + (f": {sc['summary']}" if sc.get("summary") else "")
                         + f"\n\n{text(sc.get('description'))}")
            for f in (f for f in features if (f.get("subclass") or {}).get("index") == sc["index"]):
                lv = f["level"]["name"].split()[-1] if isinstance(f.get("level"), dict) else f.get("level", "")
                lines.append(f"\n#### Level {lv}: {f['name']}\n\n{text(f.get('description'))}")
        slots = {lv["level"]: [lv["spellcasting"].get(f"spell_slots_level_{i}", 0) for i in range(1, 10)]
                 for lv in rows if lv.get("spellcasting")}
        ability = ((c.get("spellcasting") or {}).get("spellcasting_ability") or {}).get("index")
        out.append(
            entry(
                "classes", idx, c["name"], "\n".join(lines),
                f"d{c['hit_die']} hit die · saves {', '.join(saves)}", [c["name"]],
                {
                    "hit_die": c["hit_die"], "saves": [s.lower()[:3] for s in saves],
                    "spellcasting_ability": ability, "spell_slots": slots,
                    "subclasses": [sc["name"] for sc in subclasses.get(idx, [])],
                },
            )
        )
    return out


def build_species(src):
    traits = {t["index"]: t for t in load(src, "Traits")}
    subs = defaultdict(list)
    for s in load(src, "Subspecies"):
        subs[s["species"]["index"]].append(s)
    out = []
    for sp in load(src, "Species"):
        size = sp.get("size") or " or ".join(sp.get("size_options") or [])
        lines = [f"**Creature Type:** {sp.get('type', 'Humanoid')}  ", f"**Size:** {size}  ", f"**Speed:** {sp['speed']} ft.", ""]
        for ref in sp.get("traits", []):
            t = traits.get(ref["index"])
            if t:
                lines.append(f"***{t['name']}.*** {text(t.get('description'))}\n")
        if subs.get(sp["index"]):
            lines.append(f"### Options\n")
            for s in subs[sp["index"]]:
                extra = f" ({s['damage_type']['name']})" if s.get("damage_type") else ""
                lines.append(f"- **{s['name']}**{extra}")
        out.append(
            entry("species", sp["index"], sp["name"], "\n".join(lines), f"{size} · {sp['speed']} ft.", [size],
                  {"speed": sp["speed"], "size": size, "subraces": [s["name"] for s in subs.get(sp["index"], [])]})
        )
    return out


def build_backgrounds(src):
    out = []
    for b in load(src, "Backgrounds"):
        abilities = [a["name"].title() if len(a["name"]) > 3 else a["name"] for a in b.get("ability_scores", [])]
        feat = b.get("feat") or {}
        feat_txt = feat.get("name", "") + (f" ({feat['note']})" if feat.get("note") else "")
        skills = [p["name"].split(": ")[1] for p in b.get("proficiencies", []) if p["index"].startswith("skill")]
        tools = [p["name"].split(": ", 1)[-1] for p in b.get("proficiencies", []) if p["index"].startswith("tool")]
        tools += [c["desc"] for c in b.get("proficiency_choices", []) if c.get("desc")]
        lines = [
            f"**Ability Scores:** {', '.join(abilities)}  ",
            f"**Feat:** {feat_txt}  ",
            f"**Skill Proficiencies:** {', '.join(skills)}  ",
            f"**Tool Proficiency:** {', '.join(tools) or '—'}  ",
        ]
        for opt in b.get("equipment_options", []):
            if opt.get("desc"):
                lines.append(f"**Equipment:** {opt['desc']}")
        out.append(entry("backgrounds", b["index"], b["name"], "\n".join(lines), f"{feat_txt} · {', '.join(skills)}",
                         abilities, {"abilities": [a[:3].lower() for a in abilities], "feat": feat.get("name", ""), "skills": skills}))
    return out


def build_feats(src):
    out = []
    for f in load(src, "Feats"):
        kind = (f.get("type") or "").replace("-", " ").title()
        label = f"{kind} Feat" if kind else "Feat"
        pre = "; ".join(p.get("desc", "") for p in f.get("prerequisites", []) if isinstance(p, dict) and p.get("desc"))
        body = f"*{label}" + (f" (Prerequisite: {pre})" if pre else "") + "*\n\n" + text(f.get("description"))
        out.append(entry("feats", f["index"], f["name"], body, label, [kind], {"type": f.get("type", "")}))
    return out


# --------------------------------------------------------------------------- equipment


def build_equipment(src):
    out = []
    for e in load(src, "Equipment"):
        cats = [c["name"] for c in e.get("equipment_categories", [])]
        cat = cats[0] if cats else "Equipment"
        cost = f"{e['cost']['quantity']} {e['cost']['unit'].upper()}" if e.get("cost") else "—"
        lines = [f"*{cat}*", "", f"**Cost:** {cost}  "]
        if e.get("weight") is not None:
            lines.append(f"**Weight:** {e['weight']} lb.  ")
        summary = cost
        if e.get("damage"):
            dmg = f"{e['damage']['damage_dice']} {e['damage']['damage_type']['name']}"
            lines.append(f"**Damage:** {dmg}  ")
            summary = f"{dmg} · {cost}"
        if e.get("two_handed_damage"):
            lines.append(f"**Versatile:** {e['two_handed_damage']['damage_dice']}  ")
        if e.get("range", {}).get("long"):
            lines.append(f"**Range:** {e['range']['normal']}/{e['range']['long']} ft.  ")
        if e.get("throw_range"):
            lines.append(f"**Thrown:** {e['throw_range']['normal']}/{e['throw_range']['long']} ft.  ")
        if e.get("properties"):
            lines.append(f"**Properties:** {names(e['properties'])}  ")
        if e.get("mastery"):
            lines.append(f"**Mastery:** {e['mastery']['name']}  ")
        if e.get("armor_class"):
            ac = e["armor_class"]
            txt = f"+{ac['base']}" if e["index"] == "shield" else f"{ac['base']}"
            if ac.get("dex_bonus") and e["index"] != "shield":
                txt += " + Dex modifier" + (f" (max {ac['max_bonus']})" if ac.get("max_bonus") else "")
            lines.append(f"**Armor Class:** {txt}  ")
            summary = f"AC {txt} · {cost}"
            if e.get("str_minimum"):
                lines.append(f"**Strength:** {e['str_minimum']}  ")
            if e.get("stealth_disadvantage"):
                lines.append("**Stealth:** Disadvantage  ")
            if e.get("don_time"):
                lines.append(f"**Don/Doff:** {e['don_time']} / {e.get('doff_time', '')}  ")
        if e.get("contents"):
            lines.append("\n**Contents:** " + ", ".join(
                f"{c.get('quantity', 1)}× {(c.get('item') or {}).get('name', '')}" for c in e["contents"]))
        if e.get("description"):
            lines.append("\n" + text(e["description"]))
        out.append(
            entry("equipment", e["index"], e["name"], "\n".join(lines), summary,
                  [cat, *(e.get("mastery") or {}).get("name", "").split()],
                  {"category": cat, "weight": e.get("weight"), "cost": cost,
                   "damage": (e.get("damage") or {}).get("damage_dice"), "mastery": (e.get("mastery") or {}).get("name")})
        )
    return out


def build_magic_items(src):
    out = []
    for m in load(src, "Magic-Items"):
        if m.get("variant"):
            continue
        desc = text(m.get("desc") or m.get("description"))
        head, _, rest = desc.partition("\n\n")
        rarity = m["rarity"]["name"]
        body = f"*{head.strip()}*\n\n{rest.strip()}"
        if m.get("variants"):
            body += f"\n\n**Variants:** {names(m['variants'])}"
        out.append(entry("magic-items", m["index"], m["name"], body, head.strip(),
                         [rarity, m["equipment_category"]["name"], "attunement" if m.get("attunement") else ""],
                         {"rarity": rarity, "attunement": bool(m.get("attunement"))}))
    return out


# --------------------------------------------------------------------------- rules prose from the SRD Markdown

CHAPTERS = {"Playing the Game": "##", "Rules Glossary": "#####", "Gameplay Toolbox": "##"}
_PAGE = re.compile(r"^\*\*\d+\*\*\s+System Reference Document 5\.2\.1\s*$")


def _clean_heading(h: str) -> str:
    h = re.sub(r"[*_]|</?(mark|u)>", "", h).strip()
    if re.search(r"[a-z][A-Z]", h):  # small caps that came through as "ExcEptions supErsEdE"
        h = h.title()
    return h


def _clean_line(line: str) -> str:
    line = re.sub(r"</?(mark|u)>", "", line).replace("<br>", " ").rstrip()
    m = re.match(r"^(#{2,6})\s+(.*)$", line)
    if m:
        return "#" * m.group(1).__len__() + " " + _clean_heading(m.group(2))
    return line


def build_rules(md_path: Path):
    lines = md_path.read_text(encoding="utf-8").splitlines()
    starts = {}
    for i, line in enumerate(lines):
        m = re.match(r"^# \*\*(.+?)\*\*\s*$", line)
        if m:
            starts[m.group(1).strip()] = i
    order = sorted(starts.items(), key=lambda kv: kv[1])
    out = []
    for chapter, level in CHAPTERS.items():
        begin = starts[chapter]
        end = next((i for _, i in order if i > begin), len(lines))
        body = [_clean_line(l) for l in lines[begin + 1:end] if not _PAGE.match(l.strip())]
        current, buf = None, []

        def flush():
            if current and "\n".join(buf).strip():
                name = current
                tags = [chapter]
                tag = re.search(r"\[(.+?)\]$", name)
                if tag:
                    tags.append(tag.group(1))
                    name = name[: tag.start()].strip()
                slug = re.sub(r"[^a-z0-9]+", "-", f"{chapter} {current}".lower()).strip("-")
                content = re.sub(r"\n{3,}", "\n\n", "\n".join(buf)).strip()
                out.append(entry("rules", slug, name, content, chapter, tags))

        prefix = level + " "
        for line in body:
            if line.startswith(prefix) and not line.startswith(level + "#"):
                flush()
                current, buf = line[len(prefix):].strip(), []
            elif current:
                buf.append(line)
        flush()
    return out


# --------------------------------------------------------------------------- main


def main(argv):
    if len(argv) != 3:
        print(__doc__)
        return 2
    src = Path(argv[1]) / "src" / "2024" / "en"
    md = Path(argv[2])
    entries = []
    entries += build_rules(md)
    entries += build_simple(src, "Conditions", "conditions")
    entries += build_simple(src, "Ability-Scores", "abilities", lambda a: a["name"], lambda a: a["full_name"])
    entries += build_skills(src)
    entries += build_classes(src)
    entries += build_species(src)
    entries += build_backgrounds(src)
    entries += build_feats(src)
    entries += build_spells(src)
    entries += build_monsters(src)
    entries += build_equipment(src)
    entries += build_magic_items(src)
    entries += build_simple(src, "Weapon-Mastery-Properties", "weapon-mastery")
    entries += build_simple(src, "Weapon-Properties", "weapon-properties")
    entries += build_simple(src, "Damage-Types", "damage-types")
    entries += build_simple(src, "Magic-Schools", "magic-schools")
    entries += build_simple(src, "Alignments", "alignments", lambda a: a["abbreviation"])
    entries += build_simple(src, "Poisons", "poisons", lambda p: f"{p['type'].title()} · {p['cost']} GP")
    entries += build_languages(src)
    ids = [e["id"] for e in entries]
    dupes = {i for i in ids if ids.count(i) > 1}
    if dupes:
        raise SystemExit(f"duplicate ids: {sorted(dupes)[:10]}")
    payload = {
        "meta": {
            "title": "System Reference Document 5.2.1",
            "edition": "2024",
            "license": "CC-BY-4.0",
            "attribution": ATTRIBUTION,
            "source": "Converted from https://github.com/5e-bits/5e-database (MIT) and the SRD 5.2.1 by scripts/build_srd_2024.py",
        },
        "entries": entries,
    }
    raw = json.dumps(payload, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    OUT.write_bytes(gzip.compress(raw, compresslevel=9, mtime=0))
    counts = defaultdict(int)
    for e in entries:
        counts[e["category"]] += 1
    print(f"wrote {OUT} ({len(entries)} entries, {OUT.stat().st_size // 1024} KiB)")
    for k, v in counts.items():
        print(f"  {k}: {v}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
