"""Table-side game logic: the initiative tracker, rewards and rests."""

from __future__ import annotations

from . import dice, models, rules, schemas, srd
from .repo import NotFound, Repo


def _combatant(data: dict) -> dict:
    return schemas.Combatant.model_validate(data).model_dump()


def _log(doc: dict, text: str, kind: str) -> None:
    doc["log"].append({"id": models.new_id(), "time": models.now(), "kind": kind, "text": text})


# --------------------------------------------------------------------------- encounter helpers


def sort_initiative(encounter: dict) -> None:
    """Order combatants by initiative, breaking ties on initiative bonus, then name.

    The SRD leaves ties to the GM; this is just the starting order.
    """
    encounter["combatants"].sort(key=lambda c: (-c["initiative"], -c["init_bonus"], c["name"]))


def advance_turn(encounter: dict, step: int = 1) -> None:
    combatants = encounter["combatants"]
    if not combatants:
        encounter.update(round=0, turn=0, active=False)
        return
    if not encounter["active"]:
        sort_initiative(encounter)
        encounter.update(active=True, round=1, turn=0)
        return
    turn = encounter["turn"] + step
    rnd = encounter["round"]
    while turn >= len(combatants):
        turn -= len(combatants)
        rnd += 1
    while turn < 0:
        if rnd <= 1:
            turn = 0
            break
        turn += len(combatants)
        rnd -= 1
    encounter.update(turn=turn, round=rnd)


def apply_hp_change(combatant: dict, amount: int) -> None:
    """Negative amount = damage (temp HP absorbs first), positive = healing."""
    if amount < 0:
        dmg = -amount
        absorbed = min(combatant.get("temp_hp", 0), dmg)
        combatant["temp_hp"] = combatant.get("temp_hp", 0) - absorbed
        combatant["hp"] = max(0, combatant["hp"] - (dmg - absorbed))
    else:
        cap = combatant["max_hp"] or combatant["hp"] + amount
        combatant["hp"] = min(cap, combatant["hp"] + amount)


def _roll_hp(hp: int, hp_roll: str, average: bool) -> int:
    if not average and hp_roll:
        try:
            return max(1, int(dice.roll(hp_roll).total))
        except dice.DiceError:
            pass
    return hp


def _stat_source(repo: Repo, body: schemas.EncounterAction, edition: str) -> dict:
    """The monster to add: an SRD entry or a homebrew stat block, in one shape."""
    if body.monster_id:
        m = repo.get("monsters", body.monster_id)
        return {"name": m["name"], "ac": m["ac"], "hp": m["hp"], "hp_roll": m["hp_roll"], "init_bonus": m["init_bonus"],
                "xp": m["xp"], "xp_in_lair": 0, "notes": m["kind"], "srd_id": "", "monster_id": m["id"]}
    entry = srd.get(body.srd_id, edition)
    if not entry or entry["category"] != "monsters":
        raise NotFound("monster not found")
    d = entry["data"]
    return {"name": entry["name"], "ac": d["ac"], "hp": d.get("hp", 1), "hp_roll": d.get("hp_roll", ""),
            "init_bonus": d["init_bonus"], "xp": d.get("xp", 0), "xp_in_lair": d.get("xp_in_lair", 0),
            "notes": entry["summary"], "srd_id": entry["id"], "monster_id": ""}


def _edition(repo: Repo, doc: dict) -> str:
    cid = doc.get("campaign_id")
    return repo.get("campaigns", cid)["edition"] if cid else srd.DEFAULT_EDITION


def character_edition(repo: Repo, character: dict) -> str:
    """The rules edition of a character's campaign (2014 without one)."""
    return _edition(repo, character)


def _sync_pc(repo: Repo, combatant: dict, death_save_failures: int = 0, healed_from_zero: bool = False) -> None:
    """Keep a linked character sheet's HP (and death saves) in step with the tracker.

    ``death_save_failures`` of 3 marks the character dead (massive damage).
    """
    if combatant["kind"] != "pc" or not combatant["character_id"]:
        return
    try:
        ch = repo.get("characters", combatant["character_id"])
    except NotFound:
        return
    ch["hp"]["current"] = combatant["hp"]
    ch["hp"]["temp"] = combatant["temp_hp"]
    saves = ch["death_saves"]
    # Healing from 0 resets death saves, unless the character is already dead (3 failures):
    # that takes revival magic, which the DM records by hand.
    if healed_from_zero and saves["failures"] < 3:
        saves.update(successes=0, failures=0)
    saves["failures"] = min(3, saves["failures"] + death_save_failures)
    repo.save("characters", ch)


# --------------------------------------------------------------------------- encounter actions


def encounter_action(repo: Repo, encounter_id: str, body: schemas.EncounterAction) -> dict:
    enc = repo.get("encounters", encounter_id)
    combatants = enc["combatants"]
    action = body.action

    def find(cid: str) -> dict:
        for c in combatants:
            if c["id"] == cid:
                return c
        raise NotFound("combatant not found")

    if action == "add":
        c = _combatant(body.combatant)
        c["id"] = models.new_id()
        c["max_hp"] = c["max_hp"] or c["hp"]
        combatants.append(c)
    elif action == "add_monster":
        src = _stat_source(repo, body, _edition(repo, enc))
        same = [c for c in combatants if (src["srd_id"] and c["srd_id"] == src["srd_id"])
                or (src["monster_id"] and c["monster_id"] == src["monster_id"])]
        if len(same) == 1 and same[0]["name"] == src["name"]:
            same[0]["name"] = f"{src['name']} 1"  # numbering starts once there's more than one
        for i in range(body.count):
            hp = _roll_hp(src["hp"], src["hp_roll"], body.average_hp)
            n = len(same) + i + 1
            combatants.append(
                _combatant(
                    {
                        **{k: src[k] for k in ("ac", "init_bonus", "xp", "xp_in_lair", "notes", "srd_id", "monster_id")},
                        "id": models.new_id(),
                        "name": src["name"] if body.count == 1 and not same else f"{src['name']} {n}",
                        "kind": "monster",
                        "hp": hp,
                        "max_hp": hp,
                    }
                )
            )
    elif action == "add_npc":
        npc = repo.get("npcs", body.npc_id)
        base = {"ac": 10, "hp": 4, "hp_roll": "", "init_bonus": 0, "xp": 0, "xp_in_lair": 0, "srd_id": "", "monster_id": ""}
        ref = npc["stat_ref"]
        if ref.startswith(("srd:", "monster:")):
            kind, _, ref_id = ref.partition(":")
            linked = schemas.EncounterAction(action="add_monster", srd_id=ref_id if kind == "srd" else "",
                                             monster_id=ref_id if kind == "monster" else "")
            base = _stat_source(repo, linked, _edition(repo, enc))
        combatants.append(
            _combatant(
                {
                    **{k: base[k] for k in ("ac", "init_bonus", "xp", "xp_in_lair", "srd_id", "monster_id")},
                    "id": models.new_id(), "name": npc["name"], "kind": "npc", "hp": base["hp"], "max_hp": base["hp"],
                    "notes": npc["role"] or npc["stats"], "npc_id": npc["id"],
                }
            )
        )
    elif action == "add_character":
        for cid in body.character_ids or [body.character_id]:
            ch = repo.get("characters", cid)
            combatants.append(
                _combatant(
                    {
                        "id": models.new_id(),
                        "name": ch["name"],
                        "kind": "pc",
                        "ac": ch["ac"],
                        "hp": ch["hp"]["current"],
                        "max_hp": ch["hp"]["max"],
                        "temp_hp": ch["hp"]["temp"],
                        "init_bonus": rules.derive(ch, character_edition(repo, ch))["initiative"],
                        "character_id": ch["id"],
                        "conditions": list(ch["conditions"]),
                    }
                )
            )
    elif action == "update":
        c = find(body.id)
        changes = {k: v for k, v in body.changes.items() if k in c and k != "id"}
        c.update(_combatant({**c, **changes}))
    elif action == "remove":
        c = find(body.id)
        idx = combatants.index(c)
        combatants.remove(c)
        if enc["active"] and idx < enc["turn"]:
            enc["turn"] -= 1
        if enc["turn"] >= len(combatants):
            enc["turn"] = 0
    elif action == "hp":
        c = find(body.id)
        before, temp_before = c["hp"], c["temp_hp"]
        apply_hp_change(c, body.amount)
        verb = "takes" if body.amount < 0 else "heals"
        _log(enc, f"{c['name']} {verb} {abs(body.amount)} ({before} → {c['hp']} HP)", "combat")
        failures = 0
        if body.amount < 0 and c["kind"] == "pc":
            # Damage left over after reaching 0 HP; at least the hit point maximum kills outright
            # (rules/damage-and-healing). Damage while already at 0 is a death save failure.
            overflow = -body.amount - temp_before - before
            if c["hp"] == 0 and c["max_hp"] and overflow >= c["max_hp"]:
                failures = 3
                _log(enc, f"{c['name']} dies from massive damage", "combat")
            elif before == 0 and overflow > 0:
                failures = 1
                _log(enc, f"{c['name']} takes damage at 0 HP: a death save failure (two if it was a critical hit)", "combat")
        if c["hp"] == 0 and before > 0:
            _log(enc, f"{c['name']} drops to 0 HP", "combat")
        _sync_pc(repo, c, failures, healed_from_zero=before == 0 and c["hp"] > 0)
    elif action == "roll_initiative":
        # With `group`, identical monsters (same SRD entry or stat block) share one roll and act
        # together: "The GM makes one roll for an entire group of identical creatures" (SRD).
        group_rolls: dict[tuple, int] = {}
        for c in combatants:
            if body.which == "monsters" and c["kind"] == "pc":
                continue
            if body.which == "unset" and c["initiative"]:
                continue
            key = (c["srd_id"], c["monster_id"], c["init_bonus"]) if body.group and c["kind"] == "monster" and (c["srd_id"] or c["monster_id"]) else None
            if key is not None and key in group_rolls:
                c["initiative"] = group_rolls[key]
                continue
            c["initiative"] = int(dice.roll(f"d20{c['init_bonus']:+d}").total)
            if key is not None:
                group_rolls[key] = c["initiative"]
        sort_initiative(enc)
    elif action == "roll_hp":
        edition = _edition(repo, enc)
        for c in combatants:
            if c["kind"] != "monster" or not (c["srd_id"] or c["monster_id"]):
                continue
            try:
                src = _stat_source(repo, schemas.EncounterAction(action="add_monster", srd_id=c["srd_id"], monster_id=c["monster_id"]), edition)
            except NotFound:
                continue
            hp = _roll_hp(src["hp"], src["hp_roll"], average=False)
            c.update(hp=hp, max_hp=hp)
        _log(enc, "Rolled hit points for the monsters", "combat")
    elif action == "sort":
        sort_initiative(enc)
    elif action == "next":
        was_active, prev_round = enc["active"], enc["round"]
        advance_turn(enc, 1)
        if not was_active and enc["active"]:
            _log(enc, "Combat begins — round 1", "combat")
        elif enc["round"] != prev_round:
            _log(enc, f"Round {enc['round']} begins", "combat")
    elif action == "prev":
        advance_turn(enc, -1)
    elif action == "end":
        if enc["active"]:
            _log(enc, f"Combat ends after {enc['round']} round(s)", "combat")
        for c in combatants:
            _sync_pc(repo, c)
        enc.update(active=False, round=0, turn=0)
    elif action == "clear":
        kept = [] if body.all else [c for c in combatants if c["kind"] == "pc"]
        enc.update(active=False, round=0, turn=0, combatants=kept)
    return repo.save("encounters", enc)


# --------------------------------------------------------------------------- rewards, log, rests


def award(repo: Repo, session_id: str, body: schemas.Award) -> dict:
    """Give XP to characters, or hand a loot item to a character's inventory."""
    session = repo.get("sessions", session_id)
    if body.xp is not None:
        if not body.character_ids:
            raise ValueError("choose at least one character")
        names = []
        for cid in body.character_ids:
            ch = repo.get("characters", cid)
            ch["xp"] = max(0, ch["xp"] + body.xp)
            repo.save("characters", ch)
            names.append(ch["name"])
        session["xp_awarded"] += body.xp
        _log(session, f"{body.xp} XP awarded to {', '.join(names)}", "loot")
    elif body.loot_index is not None:
        if not 0 <= body.loot_index < len(session["loot"]):
            raise NotFound("loot item not found")
        item = session["loot"][body.loot_index]
        ch = repo.get("characters", body.character_id)
        ch["inventory"].append(
            {"name": item["name"], "qty": item["qty"] or 1, "weight": 0, "equipped": False, "notes": item["value"]}
        )
        repo.save("characters", ch)
        item["assigned_to"] = ch["name"]
        _log(session, f"{ch['name']} receives {item['name']}", "loot")
    else:
        raise ValueError("nothing to award")
    return repo.save("sessions", session)


def add_log(repo: Repo, session_id: str, body: schemas.LogIn) -> dict:
    session = repo.get("sessions", session_id)
    text = body.text.strip()
    if not text:
        raise ValueError("empty log entry")
    _log(session, text, body.kind or "note")
    return repo.save("sessions", session)


def rest(repo: Repo, character_id: str, kind: str) -> dict:
    """Apply a short or long rest to a character sheet."""
    ch = repo.get("characters", character_id)
    if kind not in ("short", "long"):
        raise ValueError("rest type must be short or long")
    edition = character_edition(repo, ch)
    # 2014: a long rest needs 1 HP at the start (rules/resting). 2024: both rests do.
    if ch["hp"]["current"] < 1 and (kind == "long" or edition == "2024"):
        raise ValueError(f"a character needs at least 1 hit point to take a {kind} rest")
    if kind == "long":
        ch["hp"]["current"] = ch["hp"]["max"]
        ch["hp"]["temp"] = 0
        level = rules.total_level(ch)
        # 2014: regain up to half your Hit Dice (minimum one). 2024: regain all of them.
        regained = level if edition == "2024" else max(1, level // 2)
        ch["hit_dice"]["remaining"] = min(level, ch["hit_dice"]["remaining"] + regained)
        ch["death_saves"] = {"successes": 0, "failures": 0}
        ch["exhaustion"] = max(0, ch["exhaustion"] - 1)
        resets = ("long", "short")
    else:
        resets = ("short",)
    # Every caster's slots return on a long rest; a warlock's Pact Magic slots on a short one too.
    if kind == "long" or rules.is_warlock_only(ch["classes"]):
        for slot in ch["spellcasting"]["slots"].values():
            slot["used"] = 0
    for r in ch["resources"]:
        if r["reset"] in resets:
            r["current"] = r["max"]
    return repo.save("characters", ch)
