BEGIN TRANSACTION;
CREATE TABLE alembic_version (
	version_num VARCHAR(32) NOT NULL, 
	CONSTRAINT alembic_version_pkc PRIMARY KEY (version_num)
);
INSERT INTO "alembic_version" VALUES('0002');
CREATE TABLE campaigns (
	name VARCHAR NOT NULL, 
	system VARCHAR NOT NULL, 
	description VARCHAR NOT NULL, 
	setting VARCHAR NOT NULL, 
	players JSON NOT NULL, 
	notes VARCHAR NOT NULL, 
	id VARCHAR(64) NOT NULL, 
	created VARCHAR NOT NULL, 
	updated VARCHAR NOT NULL, edition VARCHAR DEFAULT '2014' NOT NULL, 
	CONSTRAINT pk_campaigns PRIMARY KEY (id)
);
INSERT INTO "campaigns" VALUES('Test Campaign','5e (SRD 5.1)','','','[]','','camp1','2026-11-16T06:51:06+00:00','2026-11-16T06:51:06+00:00','2024');
CREATE TABLE characters (
	campaign_id VARCHAR(64), 
	name VARCHAR NOT NULL, 
	player VARCHAR NOT NULL, 
	race VARCHAR NOT NULL, 
	background VARCHAR NOT NULL, 
	alignment VARCHAR NOT NULL, 
	classes JSON NOT NULL, 
	xp INTEGER NOT NULL, 
	abilities JSON NOT NULL, 
	save_proficiencies JSON NOT NULL, 
	skills JSON NOT NULL, 
	jack_of_all_trades BOOLEAN NOT NULL, 
	ac INTEGER NOT NULL, 
	initiative_misc INTEGER NOT NULL, 
	speed INTEGER NOT NULL, 
	hp JSON NOT NULL, 
	hit_dice JSON NOT NULL, 
	death_saves JSON NOT NULL, 
	inspiration BOOLEAN NOT NULL, 
	conditions JSON NOT NULL, 
	exhaustion INTEGER NOT NULL, 
	proficiencies JSON NOT NULL, 
	attacks JSON NOT NULL, 
	spellcasting JSON NOT NULL, 
	resources JSON NOT NULL, 
	inventory JSON NOT NULL, 
	currency JSON NOT NULL, 
	features JSON NOT NULL, 
	personality JSON NOT NULL, 
	appearance VARCHAR NOT NULL, 
	backstory VARCHAR NOT NULL, 
	notes VARCHAR NOT NULL, 
	id VARCHAR(64) NOT NULL, 
	created VARCHAR NOT NULL, 
	updated VARCHAR NOT NULL, 
	CONSTRAINT pk_characters PRIMARY KEY (id), 
	CONSTRAINT fk_characters_campaign_id_campaigns FOREIGN KEY(campaign_id) REFERENCES campaigns (id) ON DELETE SET NULL
);
INSERT INTO "characters" VALUES('camp1','Pip','','','','','[{"name": "Fighter", "subclass": "", "level": 1}]',0,'{"str": 10, "dex": 10, "con": 10, "int": 10, "wis": 10, "cha": 10}','[]','{}',0,10,0,30,'{"max": 10, "current": 10, "temp": 0}','{"total": "1d10", "remaining": 1}','{"successes": 0, "failures": 0}',0,'[]',2,'{"armor": "", "weapons": "", "tools": "", "languages": "Common"}','[]','{"ability": "wis", "slots": {"1": {"max": 2, "used": 1}}, "spells": []}','[]','[]','{"cp": 0, "sp": 0, "ep": 0, "gp": 0, "pp": 0}','[]','{"traits": "", "ideals": "", "bonds": "", "flaws": ""}','','','','char1','2026-11-16T06:51:06+00:00','2026-11-16T06:51:06+00:00');
CREATE TABLE encounters (
	campaign_id VARCHAR(64), 
	session_id VARCHAR(64), 
	scene_id VARCHAR(64), 
	name VARCHAR NOT NULL, 
	round INTEGER NOT NULL, 
	turn INTEGER NOT NULL, 
	active BOOLEAN NOT NULL, 
	combatants JSON NOT NULL, 
	party_size INTEGER NOT NULL, 
	party_level INTEGER NOT NULL, 
	log JSON NOT NULL, 
	notes VARCHAR NOT NULL, 
	id VARCHAR(64) NOT NULL, 
	created VARCHAR NOT NULL, 
	updated VARCHAR NOT NULL, 
	CONSTRAINT pk_encounters PRIMARY KEY (id), 
	CONSTRAINT fk_encounters_campaign_id_campaigns FOREIGN KEY(campaign_id) REFERENCES campaigns (id) ON DELETE CASCADE, 
	CONSTRAINT fk_encounters_scene_id_scenes FOREIGN KEY(scene_id) REFERENCES scenes (id) ON DELETE SET NULL, 
	CONSTRAINT fk_encounters_session_id_sessions FOREIGN KEY(session_id) REFERENCES sessions (id) ON DELETE SET NULL
);
INSERT INTO "encounters" VALUES('camp1','sess1','scene1','New Encounter',1,0,1,'[{"id": "x", "name": "Spy", "kind": "monster", "initiative": 0, "init_bonus": 0, "ac": 10, "hp": 27, "max_hp": 27, "temp_hp": 0, "conditions": [], "notes": "", "character_id": "", "srd_id": "", "monster_id": "", "npc_id": "", "xp": 0, "xp_in_lair": 0, "in_lair": false, "surprised": false, "hidden": false}]',4,1,'[]','','enc1','2026-11-16T06:51:06+00:00','2026-11-16T06:51:06+00:00');
CREATE TABLE handbook (
	campaign_id VARCHAR(64), 
	title VARCHAR NOT NULL, 
	category VARCHAR NOT NULL, 
	tags JSON NOT NULL, 
	body VARCHAR NOT NULL, 
	id VARCHAR(64) NOT NULL, 
	created VARCHAR NOT NULL, 
	updated VARCHAR NOT NULL, 
	CONSTRAINT pk_handbook PRIMARY KEY (id), 
	CONSTRAINT fk_handbook_campaign_id_campaigns FOREIGN KEY(campaign_id) REFERENCES campaigns (id) ON DELETE SET NULL
);
INSERT INTO "handbook" VALUES(NULL,'Lore','House Rules','["lore", "town"]','','hb1','2026-11-16T06:51:06+00:00','2026-11-16T06:51:06+00:00');
CREATE TABLE handouts (
	campaign_id VARCHAR(64), 
	folder VARCHAR NOT NULL, 
	title VARCHAR NOT NULL, 
	filename VARCHAR NOT NULL, 
	mime VARCHAR NOT NULL, 
	body VARCHAR NOT NULL, 
	id VARCHAR(64) NOT NULL, 
	created VARCHAR NOT NULL, 
	updated VARCHAR NOT NULL, 
	CONSTRAINT pk_handouts PRIMARY KEY (id), 
	CONSTRAINT fk_handouts_campaign_id_campaigns FOREIGN KEY(campaign_id) REFERENCES campaigns (id) ON DELETE CASCADE
);
INSERT INTO "handouts" VALUES('camp1','Maps','Map','','','','ho1','2026-11-16T06:51:06+00:00','2026-11-16T06:51:06+00:00');
CREATE TABLE monsters (
	campaign_id VARCHAR(64), 
	name VARCHAR NOT NULL, 
	kind VARCHAR NOT NULL, 
	cr VARCHAR NOT NULL, 
	xp INTEGER NOT NULL, 
	ac INTEGER NOT NULL, 
	hp INTEGER NOT NULL, 
	hp_roll VARCHAR NOT NULL, 
	init_bonus INTEGER NOT NULL, 
	speed VARCHAR NOT NULL, 
	abilities JSON NOT NULL, 
	body VARCHAR NOT NULL, 
	notes VARCHAR NOT NULL, 
	id VARCHAR(64) NOT NULL, 
	created VARCHAR NOT NULL, 
	updated VARCHAR NOT NULL, 
	CONSTRAINT pk_monsters PRIMARY KEY (id), 
	CONSTRAINT fk_monsters_campaign_id_campaigns FOREIGN KEY(campaign_id) REFERENCES campaigns (id) ON DELETE CASCADE
);
INSERT INTO "monsters" VALUES(NULL,'Gate Guard','Medium humanoid, neutral','2',450,10,1,'',0,'30 ft.','{"str": 16}','','','mon1','2026-11-16T06:51:06+00:00','2026-11-16T06:51:06+00:00');
CREATE TABLE notes (
	campaign_id VARCHAR(64), 
	session_id VARCHAR(64), 
	kind VARCHAR NOT NULL, 
	title VARCHAR NOT NULL, 
	body VARCHAR NOT NULL, 
	pinned BOOLEAN NOT NULL, 
	done BOOLEAN NOT NULL, 
	id VARCHAR(64) NOT NULL, 
	created VARCHAR NOT NULL, 
	updated VARCHAR NOT NULL, 
	CONSTRAINT pk_notes PRIMARY KEY (id), 
	CONSTRAINT fk_notes_campaign_id_campaigns FOREIGN KEY(campaign_id) REFERENCES campaigns (id) ON DELETE CASCADE, 
	CONSTRAINT fk_notes_session_id_sessions FOREIGN KEY(session_id) REFERENCES sessions (id) ON DELETE SET NULL
);
INSERT INTO "notes" VALUES('camp1','sess1','quest','New note','',1,0,'note1','2026-11-16T06:51:06+00:00','2026-11-16T06:51:06+00:00');
CREATE TABLE npcs (
	campaign_id VARCHAR(64), 
	name VARCHAR NOT NULL, 
	role VARCHAR NOT NULL, 
	first_appears VARCHAR NOT NULL, 
	looks VARCHAR NOT NULL, 
	voice VARCHAR NOT NULL, 
	wants VARCHAR NOT NULL, 
	secret VARCHAR NOT NULL, 
	knows VARCHAR NOT NULL, 
	stats VARCHAR NOT NULL, 
	stat_ref VARCHAR NOT NULL, 
	notes VARCHAR NOT NULL, 
	tags JSON NOT NULL, 
	id VARCHAR(64) NOT NULL, 
	created VARCHAR NOT NULL, 
	updated VARCHAR NOT NULL, 
	CONSTRAINT pk_npcs PRIMARY KEY (id), 
	CONSTRAINT fk_npcs_campaign_id_campaigns FOREIGN KEY(campaign_id) REFERENCES campaigns (id) ON DELETE CASCADE
);
INSERT INTO "npcs" VALUES('camp1','Mara','','','','','','','','','srd:monsters/spy','','["spy"]','npc1','2026-11-16T06:51:06+00:00','2026-11-16T06:51:06+00:00');
CREATE TABLE scenes (
	campaign_id VARCHAR(64), 
	session_id VARCHAR(64), 
	position INTEGER NOT NULL, 
	title VARCHAR NOT NULL, 
	location VARCHAR NOT NULL, 
	status VARCHAR NOT NULL, 
	summary VARCHAR NOT NULL, 
	read_aloud VARCHAR NOT NULL, 
	body VARCHAR NOT NULL, 
	id VARCHAR(64) NOT NULL, 
	created VARCHAR NOT NULL, 
	updated VARCHAR NOT NULL, 
	CONSTRAINT pk_scenes PRIMARY KEY (id), 
	CONSTRAINT fk_scenes_campaign_id_campaigns FOREIGN KEY(campaign_id) REFERENCES campaigns (id) ON DELETE CASCADE, 
	CONSTRAINT fk_scenes_session_id_sessions FOREIGN KEY(session_id) REFERENCES sessions (id) ON DELETE SET NULL
);
INSERT INTO "scenes" VALUES('camp1','sess1',0,'Gate','','active','','','','scene1','2026-11-16T06:51:06+00:00','2026-11-16T06:51:06+00:00');
CREATE TABLE "sessions" (
	campaign_id VARCHAR(64), 
	number INTEGER NOT NULL, 
	title VARCHAR NOT NULL, 
	date VARCHAR NOT NULL, 
	status VARCHAR NOT NULL, 
	attendance JSON NOT NULL, 
	prep VARCHAR NOT NULL, 
	recap VARCHAR NOT NULL, 
	log JSON NOT NULL, 
	loot JSON NOT NULL, 
	xp_awarded INTEGER NOT NULL, 
	id VARCHAR(64) NOT NULL, 
	created VARCHAR NOT NULL, 
	updated VARCHAR NOT NULL, 
	CONSTRAINT pk_sessions PRIMARY KEY (id), 
	CONSTRAINT fk_sessions_campaign_id_campaigns FOREIGN KEY(campaign_id) REFERENCES campaigns (id) ON DELETE CASCADE
);
INSERT INTO "sessions" VALUES('camp1',1,'New Session','','planned','[]','','','[]','[{"name": "Gem", "qty": 2, "value": "", "assigned_to": ""}]',0,'sess1','2026-11-16T06:51:06+00:00','2026-11-16T06:51:06+00:00');
CREATE TABLE trackers (
	campaign_id VARCHAR(64), 
	session_id VARCHAR(64), 
	name VARCHAR NOT NULL, 
	value INTEGER NOT NULL, 
	max INTEGER NOT NULL, 
	steps JSON NOT NULL, 
	notes VARCHAR NOT NULL, 
	id VARCHAR(64) NOT NULL, 
	created VARCHAR NOT NULL, 
	updated VARCHAR NOT NULL, 
	CONSTRAINT pk_trackers PRIMARY KEY (id), 
	CONSTRAINT fk_trackers_campaign_id_campaigns FOREIGN KEY(campaign_id) REFERENCES campaigns (id) ON DELETE CASCADE, 
	CONSTRAINT fk_trackers_session_id_sessions FOREIGN KEY(session_id) REFERENCES sessions (id) ON DELETE SET NULL
);
INSERT INTO "trackers" VALUES('camp1',NULL,'Alarm',3,7,'[]','','trk1','2026-11-16T06:51:06+00:00','2026-11-16T06:51:06+00:00');
CREATE INDEX ix_characters_campaign_id ON characters (campaign_id);
CREATE INDEX ix_handbook_campaign_id ON handbook (campaign_id);
CREATE INDEX ix_handouts_campaign_id ON handouts (campaign_id);
CREATE INDEX ix_monsters_campaign_id ON monsters (campaign_id);
CREATE INDEX ix_npcs_campaign_id ON npcs (campaign_id);
CREATE INDEX ix_notes_campaign_id ON notes (campaign_id);
CREATE INDEX ix_notes_session_id ON notes (session_id);
CREATE INDEX ix_scenes_campaign_id ON scenes (campaign_id);
CREATE INDEX ix_scenes_session_id ON scenes (session_id);
CREATE INDEX ix_trackers_campaign_id ON trackers (campaign_id);
CREATE INDEX ix_trackers_session_id ON trackers (session_id);
CREATE INDEX ix_encounters_campaign_id ON encounters (campaign_id);
CREATE INDEX ix_encounters_scene_id ON encounters (scene_id);
CREATE INDEX ix_encounters_session_id ON encounters (session_id);
CREATE INDEX ix_sessions_campaign_id ON sessions (campaign_id);
COMMIT;
