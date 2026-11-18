# Maps and handouts

[Documentation](../README.md) › Maps and handouts

A handout is either an **image** you upload or a **map** you draw. Add one from the navigator's
**Handouts** section on the DM screen: **+ Handout** for an image, **+ Map** for a map. Handouts
can be sorted into folders (set the folder in **Edit**), and each has a **Notes and key** section
for the map key, what the players see, and secrets.

To change your mind, use the buttons at the top of the handout: **Use an image instead…** on a
map, or **Draw a map instead…** on an image. Each asks first, because the drawing or the image is
removed. **Replace image…** swaps an image for another.

## Images

Drop a file onto the handout, paste one, or click to choose it: PNG, JPG, WebP, GIF or SVG, up to
15 MB. The mouse wheel or a touchpad zooms around the pointer, dragging pans, and **Reset view**
goes back to the start.

## Drawing a map

The toolbar along the top:

| Tool | What it does |
|---|---|
| ✋ Move | Drag tokens, markers and text; drag empty space to pan; double-click a token, marker or text to edit it |
| 🖌 Paint | Paint cells with the chosen legend colour, or **Clear** to blank them. Hold the button and drag to paint a line |
| 🪣 Fill | **Same-colour area** fills the connected cells painted like the one you click. **Up to the walls** fills everything the walls and doors enclose, whatever its colour (the whole grid if there are none) |
| 🧽 Erase | Remove colours, walls, text, markers or tokens |
| ▬ Walls | Click or drag along the lines between cells for a **Wall**, **Door** or **Secret door**; click one again to remove it. Square grids only |
| T Text | Click to place a label |
| 📍 Markers | Click to drop the chosen symbol |
| ● Tokens | Click to place a token for a party member, or for anyone else (you name it) |
| ☁ Fog | Turn on **Fog on the players' screen**, then **Reveal** or **Hide** cells; **Hide all** and **Reveal all** do the whole map |

Text and markers can be **DM only**: players never see them.

Also on the toolbar:
- **↶ ↷:** undo and redo (Ctrl+Z, and Ctrl+Shift+Z or Ctrl+Y; ⌘ on a Mac).
- **Fit:** fits the whole map in view.
- **Grid…:** the map's settings:
  - square or hex grid, columns and rows (up to 120 each) and cell size;
  - the background colour;
  - the legend: name each colour; **+ Colour** adds one, and removing a colour blanks its cells;
  - whether the legend shows on the map;
  - whether players can move their tokens.
- **⬇ Export…:** see below.

Wherever you are, the mouse wheel zooms and the middle or right mouse button pans.

## Showing it to players

**📽 Show to players** opens the players' view:

- A second window opens (a pop-up window in a browser). Drag it onto the projector or TV; its ⛶
  button makes it full screen.
- Or, when Questodian runs as a server on your network, open `/project` on another device, such as
  `http://192.168.1.20:8765/project` (see
  [Getting started](getting-started.md#in-a-browser-instead)).

The players' view follows what you show and how you zoom and pan. On a map, fogged cells stay
covered, along with any tokens, markers, text or walls in them; DM-only text and markers don't
show; and secret doors look like walls. If **Players can move their tokens** is on (in **Grid…**),
the players' view can drag the tokens of party characters (not other tokens, and nothing else on
the map).

## Exporting a map

**⬇ Export…** saves the map as:

- **PNG: what the players see:** with fog, no DM-only notes;
- **PNG: the DM's view;**
- **SVG**, the DM's view;
- **Universal VTT (`.dd2vtt`):** the map image with its walls and doors, for Foundry and other
  virtual tabletops that read the format.

**Copy link** at the top of any handout copies `[[its name]]`, to paste into notes.
