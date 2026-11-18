-- Upgrade from revision 0003 to 0004: maps.
-- A handout can be a map drawn in the app: its grid, legend, painted cells, walls and doors,
-- text, markers, tokens and fog, as the map editor writes them. Empty for image handouts.
ALTER TABLE handouts ADD COLUMN map JSON DEFAULT '{}' NOT NULL;
