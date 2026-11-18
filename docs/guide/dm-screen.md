# The DM screen

[Documentation](../README.md) › The DM screen

The DM screen is the home page while a campaign is open. It has three columns. Drag the borders
between them to resize them (double-click a border to reset it). **☰** in the top bar hides or
shows the left column.

## Left: the navigator

Two lists:

- **Campaign:**
  - sessions;
  - scenes, grouped by status;
  - NPCs, notes, homebrew monsters and trackers;
  - handouts, by folder;
  - the party.

  Each section has a filter and a **+** button to add one.
- **Encounters:** every encounter, with the running one marked.

Click anything to open it in the middle.

## Middle: the workspace

Everything you open becomes a tab, and reads and edits in the same layout (**Edit** switches a
record to editing; it saves as you type).

**Tabs** work like a browser's:
- they shrink to fit as you open more;
- when they're at their smallest, **‹ ›** at the ends page through them;
- the tab you open always scrolls into view;
- drag a tab to move it (Escape while dragging puts it back);
- **×** or a middle-click closes it.

What each kind of record holds:

| Record | What it holds |
|---|---|
| **Campaign overview** | The pitch, "the truth" (DM only), and the campaign's rules edition, which can be switched |
| **Scenes** | Summary, boxed *read-aloud* text, DM notes, and a status: planned, active, completed or skipped |
| **NPCs** | Looks, voice, wants, secret, what they know, and a linked stat block |
| **Encounters** | See below |
| **Sessions** | Their scenes in order, trackers, encounters, prep, recap, a timestamped log, loot that goes straight into characters' inventories, and XP |
| **Notes** | General, session, quest or pin-board notes |
| **Trackers** | Clocks such as "hours until the ritual", counted in steps |
| **Monsters** | Your homebrew stat blocks |
| **Handouts** | An image or a map: see [Maps and handouts](maps-and-handouts.md) |

### Encounters

- **Difficulty:** set the party's size and level to see how hard the encounter is, from the 2024
  rules' low / moderate / high XP budget.
- **Adding combatants:** from the SRD or your homebrew monsters, NPCs, the party (**Add everyone**),
  or anyone you type in. **Roll HP** rolls every monster's hit points from its hit dice.
- **Running it:** **Roll initiative** rolls for everyone before combat starts, and during it for
  anyone added without one (ties go by initiative bonus; edit a number to change the order). **Start combat →** begins, **Next →** moves to the next turn,
  and **Finish** ends it. A "now" banner shows whose turn it is, and the round count goes up as
  you go.
- **HP pills:** click one to change it: type an amount, then **Enter** for damage or
  **Shift+Enter** to heal (or the buttons, plus **Temp** for temporary hit points).
- Conditions on each combatant, and combatants you can mark as hidden.

## Right: the reference panel

Three tabs:

- **Reference:** the rules for the campaign's edition, with filters: monsters, spells, items, rules
  and more, plus your homebrew monsters.
- **View:** whatever you last clicked to look at, such as a stat block, a spell or a linked record.
- **Notes:** quick notes for the session.

Dice are in the 🎲 quick roll, bottom right, as on every page: see [Dice](dice.md).

## Linking and searching

- **Links in notes:** type `[[Captain Vane]]` in any notes field to link to a record, or
  `[[spell:Fireball]]` and `[[monster:Zombie]]` for the rules. Typing `[[` suggests names. Click a
  link to open it.
- **Rolling from text:** attacks and damage in a stat block or note roll when clicked, for example
  `+6` or `2d8+3`.
- **Ctrl K** (⌘K on a Mac) searches everything: the campaign's records, the rules, and quick
  actions (new scene, NPC, encounter or note; the campaign overview; Characters; the rules
  library). Arrow keys move, **Enter** opens, **Escape** closes.
