-- Upgrade from revision 0002 to 0003: the character builder.
-- `build` holds a character's choices (ability method, base scores, background bonuses,
-- species and lineage, skill picks...); the sheet's other fields are worked out from it.
ALTER TABLE characters ADD COLUMN build JSON DEFAULT '{}' NOT NULL;

-- Your edits to the bundled rules data (data/rules-2024.json.gz). One row per edited entry;
-- `data` is JSON text (TEXT, so numbers keep their exact form); NULL means the entry was hidden.
CREATE TABLE rule_overrides (
	kind VARCHAR NOT NULL, 
	"key" VARCHAR NOT NULL, 
	data TEXT, 
	updated VARCHAR NOT NULL, 
	CONSTRAINT pk_rule_overrides PRIMARY KEY (kind, "key")
);
