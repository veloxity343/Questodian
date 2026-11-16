BEGIN TRANSACTION;
CREATE TABLE alembic_version (
	version_num VARCHAR(32) NOT NULL, 
	CONSTRAINT alembic_version_pkc PRIMARY KEY (version_num)
);
INSERT INTO "alembic_version" VALUES('0001');
CREATE TABLE campaigns (
	name VARCHAR NOT NULL, 
	system VARCHAR NOT NULL, 
	description VARCHAR NOT NULL, 
	setting VARCHAR NOT NULL, 
	players JSON NOT NULL, 
	notes VARCHAR NOT NULL, 
	id VARCHAR(64) NOT NULL, 
	created VARCHAR NOT NULL, 
	updated VARCHAR NOT NULL, 
	CONSTRAINT pk_campaigns PRIMARY KEY (id)
);
INSERT INTO "campaigns" VALUES('Lost Mine','5e (SRD 5.1)','','','[{"name": "Ann", "contact": "", "character_id": "char1", "notes": ""}]','','camp1','2025-06-01T10:00:00+00:00','2025-06-01T10:00:00+00:00');
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
INSERT INTO "characters" VALUES('camp1','Ann''s Rogue','','','','','[{"name": "Rogue", "subclass": "", "level": 3}]',0,'{"str": 10, "dex": 10, "con": 10, "int": 10, "wis": 10, "cha": 10}','[]','{"stealth": "expertise"}',0,10,0,30,'{"max": 21, "current": 9, "temp": 0}','{"total": "1d10", "remaining": 1}','{"successes": 0, "failures": 0}',0,'[]',0,'{"armor": "", "weapons": "", "tools": "", "languages": "Common"}','[]','{"ability": "", "slots": {}, "spells": []}','[]','[{"name": "Rope", "qty": 1, "weight": 10.0, "equipped": false, "notes": ""}]','{"cp": 0, "sp": 0, "ep": 0, "gp": 0, "pp": 0}','[]','{"traits": "", "ideals": "", "bonds": "", "flaws": ""}','','','','char1','2025-06-01T10:00:00+00:00','2025-06-01T10:00:00+00:00');
INSERT INTO "characters" VALUES(NULL,'Unlinked','','','','','[{"name": "Fighter", "subclass": "", "level": 1}]',0,'{"str": 10, "dex": 10, "con": 10, "int": 10, "wis": 10, "cha": 10}','[]','{}',0,10,0,30,'{"max": 10, "current": 10, "temp": 0}','{"total": "1d10", "remaining": 1}','{"successes": 0, "failures": 0}',0,'[]',0,'{"armor": "", "weapons": "", "tools": "", "languages": "Common"}','[]','{"ability": "", "slots": {}, "spells": []}','[]','[]','{"cp": 0, "sp": 0, "ep": 0, "gp": 0, "pp": 0}','[]','{"traits": "", "ideals": "", "bonds": "", "flaws": ""}','','','','char2','2025-06-01T10:00:00+00:00','2025-06-01T10:00:00+00:00');
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
INSERT INTO "handbook" VALUES('camp1','Crits','House Rules','["house"]','','hb1','2025-06-01T10:00:00+00:00','2025-06-01T10:00:00+00:00');
CREATE TABLE sessions (
	campaign_id VARCHAR(64), 
	number INTEGER NOT NULL, 
	title VARCHAR NOT NULL, 
	date VARCHAR NOT NULL, 
	status VARCHAR NOT NULL, 
	attendance JSON NOT NULL, 
	prep VARCHAR NOT NULL, 
	recap VARCHAR NOT NULL, 
	log JSON NOT NULL, 
	encounter JSON NOT NULL, 
	loot JSON NOT NULL, 
	xp_awarded INTEGER NOT NULL, 
	id VARCHAR(64) NOT NULL, 
	created VARCHAR NOT NULL, 
	updated VARCHAR NOT NULL, 
	CONSTRAINT pk_sessions PRIMARY KEY (id), 
	CONSTRAINT fk_sessions_campaign_id_campaigns FOREIGN KEY(campaign_id) REFERENCES campaigns (id) ON DELETE CASCADE
);
INSERT INTO "sessions" VALUES('camp1',1,'Part 1','','planned','[]','','','[{"id": "l1", "time": "2025-06-01T10:00:00+00:00", "kind": "note", "text": "Started"}]','{"round": 2, "turn": 1, "active": true, "combatants": [{"id": "c1", "name": "Ann''s Rogue", "kind": "pc", "initiative": 17, "hp": 9, "max_hp": 21, "character_id": "char1"}, {"id": "c2", "name": "Goblin 1", "kind": "monster", "initiative": 12, "hp": 7, "max_hp": 7, "srd_id": "monsters/goblin"}]}','[]',0,'sess1','2025-06-01T10:00:00+00:00','2025-06-01T10:00:00+00:00');
INSERT INTO "sessions" VALUES('camp1',2,'Part 2','','planned','[]','','','[{"id": "l1", "time": "2025-06-01T10:00:00+00:00", "kind": "note", "text": "Started"}]','{"round": 0, "turn": 0, "active": false, "combatants": []}','[]',0,'sess2','2025-06-01T10:00:00+00:00','2025-06-01T10:00:00+00:00');
CREATE INDEX ix_characters_campaign_id ON characters (campaign_id);
CREATE INDEX ix_handbook_campaign_id ON handbook (campaign_id);
CREATE INDEX ix_sessions_campaign_id ON sessions (campaign_id);
COMMIT;
