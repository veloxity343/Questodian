-- Upgrade from revision 0005 to 0006: sync through a cloud folder (sync.rs).
--
-- Every change to a record is noted in sync_outbox, by triggers, until sync has sent it: what
-- changed, and when (milliseconds since 1970). Rows the sync itself writes aren't noted: it sets
-- the 'applying' key in sync_state while it writes, inside the same transaction. A deleted
-- record's row stays, and is sent as a deletion.
CREATE TABLE sync_state (
	"key" VARCHAR NOT NULL, 
	value VARCHAR NOT NULL, 
	CONSTRAINT pk_sync_state PRIMARY KEY ("key")
);

CREATE TABLE sync_outbox (
	kind VARCHAR NOT NULL, 
	id VARCHAR NOT NULL, 
	changed INTEGER NOT NULL, 
	CONSTRAINT pk_sync_outbox PRIMARY KEY (kind, id)
);

-- The version of each record this device holds, as sync knows it: when it was made, and where.
CREATE TABLE sync_stamps (
	kind VARCHAR NOT NULL, 
	id VARCHAR NOT NULL, 
	stamp INTEGER NOT NULL, 
	device VARCHAR NOT NULL, 
	CONSTRAINT pk_sync_stamps PRIMARY KEY (kind, id)
);

-- The other devices' change files this device has applied, by name.
CREATE TABLE sync_applied (
	name VARCHAR NOT NULL, 
	CONSTRAINT pk_sync_applied PRIMARY KEY (name)
);

-- A version that lost to a later one from another device, kept a while to restore.
CREATE TABLE sync_replaced (
	n INTEGER NOT NULL, 
	kind VARCHAR NOT NULL, 
	id VARCHAR NOT NULL, 
	data JSON NOT NULL, 
	replaced VARCHAR NOT NULL, 
	CONSTRAINT pk_sync_replaced PRIMARY KEY (n)
);

CREATE TRIGGER sync_campaigns_insert AFTER INSERT ON campaigns
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('campaigns', NEW.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_campaigns_update AFTER UPDATE ON campaigns
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('campaigns', NEW.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_campaigns_delete AFTER DELETE ON campaigns
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('campaigns', OLD.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_sessions_insert AFTER INSERT ON sessions
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('sessions', NEW.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_sessions_update AFTER UPDATE ON sessions
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('sessions', NEW.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_sessions_delete AFTER DELETE ON sessions
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('sessions', OLD.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_characters_insert AFTER INSERT ON characters
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('characters', NEW.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_characters_update AFTER UPDATE ON characters
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('characters', NEW.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_characters_delete AFTER DELETE ON characters
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('characters', OLD.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_handbook_insert AFTER INSERT ON handbook
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('handbook', NEW.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_handbook_update AFTER UPDATE ON handbook
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('handbook', NEW.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_handbook_delete AFTER DELETE ON handbook
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('handbook', OLD.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_scenes_insert AFTER INSERT ON scenes
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('scenes', NEW.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_scenes_update AFTER UPDATE ON scenes
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('scenes', NEW.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_scenes_delete AFTER DELETE ON scenes
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('scenes', OLD.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_npcs_insert AFTER INSERT ON npcs
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('npcs', NEW.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_npcs_update AFTER UPDATE ON npcs
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('npcs', NEW.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_npcs_delete AFTER DELETE ON npcs
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('npcs', OLD.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_monsters_insert AFTER INSERT ON monsters
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('monsters', NEW.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_monsters_update AFTER UPDATE ON monsters
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('monsters', NEW.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_monsters_delete AFTER DELETE ON monsters
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('monsters', OLD.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_notes_insert AFTER INSERT ON notes
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('notes', NEW.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_notes_update AFTER UPDATE ON notes
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('notes', NEW.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_notes_delete AFTER DELETE ON notes
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('notes', OLD.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_encounters_insert AFTER INSERT ON encounters
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('encounters', NEW.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_encounters_update AFTER UPDATE ON encounters
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('encounters', NEW.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_encounters_delete AFTER DELETE ON encounters
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('encounters', OLD.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_trackers_insert AFTER INSERT ON trackers
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('trackers', NEW.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_trackers_update AFTER UPDATE ON trackers
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('trackers', NEW.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_trackers_delete AFTER DELETE ON trackers
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('trackers', OLD.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_handouts_insert AFTER INSERT ON handouts
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('handouts', NEW.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_handouts_update AFTER UPDATE ON handouts
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('handouts', NEW.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_handouts_delete AFTER DELETE ON handouts
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('handouts', OLD.id, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

-- The rules-data edits, keyed by kind and key together, as a JSON array.

CREATE TRIGGER sync_rule_overrides_insert AFTER INSERT ON rule_overrides
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('rule_overrides', json_array(NEW.kind, NEW."key"), CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_rule_overrides_update AFTER UPDATE ON rule_overrides
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('rule_overrides', json_array(NEW.kind, NEW."key"), CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;

CREATE TRIGGER sync_rule_overrides_delete AFTER DELETE ON rule_overrides
WHEN NOT EXISTS (SELECT 1 FROM sync_state WHERE "key" = 'applying')
BEGIN
	INSERT INTO sync_outbox (kind, id, changed) VALUES ('rule_overrides', json_array(OLD.kind, OLD."key"), CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))
	ON CONFLICT (kind, id) DO UPDATE SET changed = excluded.changed;
END;
