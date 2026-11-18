# Bundled data

[Documentation](../README.md) › Bundled data

`data/` holds what ships inside the app:

| File | What it is | Built by |
|---|---|---|
| `srd.json.gz` | SRD 5.1 (2014 rules) | `scripts/build_srd.py` |
| `srd-2024.json.gz` | SRD 5.2.1 (2024 rules) | `scripts/build_srd_2024.py` |
| `rules-2024.json.gz` | The character builder's rules data | `scripts/build_rules_2024.py` |

Rules, numbers and stat blocks in the app come from these files, for the campaign's edition,
never from memory. `crates/core/src/srd.rs` loads and searches the SRD files;
`crates/core/src/builder.rs` loads the rules data and lays the user's edits over it.

## The SRD files

Both come from [5e-bits/5e-database](https://github.com/5e-bits/5e-database) (MIT). For SRD 5.2.1,
the rules prose (Playing the Game, the Rules Glossary and the Gameplay Toolbox) isn't in that
dataset, so it's taken from a Markdown copy of the SRD 5.2.1.

```bash
git clone --depth 1 https://github.com/5e-bits/5e-database.git
```

```bash
python3 scripts/build_srd.py path/to/5e-database
```

```bash
python3 scripts/build_srd_2024.py path/to/5e-database path/to/SRD-5.2.1.md
```

Each file is a list of entries (category, name, summary, tags, a Markdown body, and a `data` object
with the numbers features need, such as a monster's AC or a spell's level), plus the licence
attribution.

Known conversion errors in `srd-2024.json.gz` are listed in
[Troubleshooting](../guide/troubleshooting.md#known-issues). Fix them in the build script and
rebuild, rather than editing the file by hand.

## The character builder's rules data

`rules-2024.json.gz` holds:
- classes with their level tables;
- species, backgrounds and feats;
- skills, point-buy costs, the standard array and starting gear.

Most of it is from SRD 5.2.1. It's built from a character sheet the project owner keeps
privately:

```bash
python3 scripts/build_rules_2024.py path/to/character-sheet
```

Anyone can change it in the app instead (**Characters → Rules data**). Those edits are stored in
the database (`rule_overrides`), included in backups, and checked against the shape of the bundled
data before they're used.

Some backgrounds and origin feats in it aren't in the SRD. They're there by the project owner's
choice, marked `"srd": false`, and tagged "not SRD" on the Rules data page. Don't add other non-SRD
content without asking.

## Licence

The SRD 5.1 and 5.2.1 are © Wizards of the Coast LLC, under the Creative Commons Attribution 4.0
International License. The attribution is in the README and in each SRD file's `meta`. Don't add
Wizards of the Coast product identity (names, logos, settings that aren't in the SRD).
