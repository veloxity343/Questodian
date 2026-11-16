# Rules audit, 2026-11-16

`rules-lawyer` checked the app's own rules (Python backend and React UI) against the
bundled SRD 5.1 (2014) and SRD 5.2.1 (2024). Entry ids are from
`src/questodian/data/srd*.json.gz`.

**Main finding:** the rules code never reads `campaign.edition`. Only SRD lookups do
(`game.py:84`). So wherever 2014 and 2024 differ, one edition is implemented: some
places follow 2014, others 2024.

## Findings

| # | Issue | Where | SRD | Status |
|---|---|---|---|---|
| 1 | Long rest gives back half the Hit Dice; 2024 gives back all | `game.py:292` | 2014 `rules/resting`; 2024 `rules/rules-glossary-long-rest` "all spent Hit Point Dice" || Fixed: 2024 regains all |
| 2 | 2024 Paladin and Ranger have two 1st-level slots at level 1; the app gives none. `/slots` takes no edition | `rules.py:155`, `characters.tsx:551` | 2024 `classes/paladin`, `classes/ranger` || Fixed: `/slots` and auto-fill take the edition |
| 3 | Warlock pact slots don't come back on a short rest (both editions) | `game.py:293-299`, `characters.tsx:558` | 2014/2024 `classes/warlock` || Fixed for single-class warlocks. Multiclass needs separate pact slots (not in the SRD either) |
| 4 | Encounter budget is the 2024 Low/Moderate/High table for every campaign; SRD 5.1 has no thresholds, so 2014 can't get its own. "Trivial" and "deadly" bands are the app's own, not SRD | `rules.py:69-78, 219-233`, `dm/encounter.tsx:14-28` | 2024 `rules/gameplay-toolbox-combat-encounters` || Labels say "Under half of Low" / "Above High"; tooltip explains the 2024 table is used for 2014 |
| 5 | Exhaustion has no mechanical effect. 2024: −2×level to D20 tests, −5×level ft speed. 2014: the six-step table | `rules.py:176-216`, `lib/derive.ts:21-49` | `conditions/exhaustion` both || Fixed: 2024 −2×level on D20 Tests and speed; 2014 speed and −5 passives from level 1; effects listed on the sheet. 2024 passives left alone (our reading) |
| 6 | 2014 Jack of All Trades should apply to initiative (a Dexterity check); 2024's is skill-only | `rules.py:209`, `derive.ts:41` | 2014 `classes/bard`, `rules/using-each-ability`; 2024 `classes/bard` || Fixed, plus plain ability checks (2014) |
| 7 | Carrying capacity ignores size (`carrying_capacity()` is correct but never called) | `rules.py:214`, `characters.tsx:625` | 2014 `rules/using-each-ability`; 2024 `rules/rules-glossary-carrying-capacity` || No change: every SRD race and species is Small or Medium (checked) |
| 8 | Damage at 0 HP doesn't add a death-save failure; massive damage not detected; healing doesn't reset death saves | `game.py:48-57, 89-99`, `characters.tsx:425-437` | 2014 `rules/damage-and-healing`; 2024 `rules/playing-the-game-damage-and-healing` || Fixed: failure at 0 HP, massive damage marks 3 failures, healing resets. Crit = two failures noted, not detected |
| 9 | Rests allowed at 0 HP; both editions need at least 1 HP | `game.py:285-299` | 2014 `rules/resting`; 2024 short/long rest glossary || Fixed: long rest needs 1 HP (2014); both rests (2024) |
| 10 | 2024 Hit Point Die healing has a minimum of 1 | `characters.tsx:439-445` | 2024 `rules/rules-glossary-short-rest` || Fixed |
| 11 | 2024 lair XP ("XP 5,900, or 7,200 in lair") ignored | `game.py:80` | 2024 `monsters/aboleth` || Open |
| 12 | Initiative ties broken by bonus then name, and every monster rolls separately. SRD: the GM decides ties; one roll per group of identical creatures. Dex tiebreak is fine as a default but isn't SRD. 2024 Surprise (Disadvantage) unsupported | `game.py:20-22, 200-206` | 2014 `rules/the-order-of-combat`; 2024 `playing-the-game-combat` || Open (tiebreak is a default, not SRD) |
| 13 | Background suggestions list non-SRD backgrounds (licensing) | `characters.tsx:19` | 2014 has only `backgrounds/acolyte`; 2024 acolyte, criminal, sage, soldier || Fixed: suggestions come from the SRD |
| 14 | Point-buy hint says "before racial bonuses"; in 2024 increases come from backgrounds | `characters.tsx:341` | 2024 `backgrounds/acolyte`; 2014 `races/dwarf` || Fixed |
| 15 | "Dungeons & Dragons" (a trademark) in the package docstring | `src/questodian/__init__.py:1` | CLAUDE.md: no product identity || Fixed in README, package, CLI and the default campaign system label |
| 16 | Some monster XP in the bundled data disagrees with `CR_XP` (2014 dretch, riding horse, deep gnome, brass wyrmling, frog, sea horse; 2024 archmage 8,000). 2024 long-rest glossary text looks truncated | `scripts/build_srd.py` output | || Open: check `scripts/build_srd*.py`. Also 2024 tiefling size reads "desc or choose or type or from" |

Fixes were checked by `rules-lawyer` against the SRD on 2026-11-16, and its
corrections applied (2014 short rests need no HP; 2014 passives −5 when exhausted;
massive damage marks the character dead).

## Checked and correct

Ability names; all 18 skill→ability pairs; the 15 conditions; hit dice for all 12
classes; full-caster slots 1–20 both editions; half casters 2014 1–20 and 2024 2–20;
proficiency bonus (and `prof()` to level 30); ability modifier; skill bonuses with
proficiency, expertise and half proficiency; saves; passive scores; spell DC and
attack (2014 formula quoted; 2024 text less explicit); `jump()`; long rest HP, temp
HP, death saves, slots, exhaustion −1; temp HP handling; healing cap; critical
damage dice; coin rates; `CR_XP` against stat blocks apart from #16.

## Not in the SRD (can't be verified from bundled data)

`XP_BY_LEVEL`; point-buy costs and budget; the standard array; 4d6-drop-lowest for
PCs; multiclass spell-slot rules and third casters; 2014 encounter thresholds and
multipliers; coin weight; `CR_XP` for CR 18 and 25–29.
