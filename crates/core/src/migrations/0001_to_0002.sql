-- Upgrade from Alembic revision 0001 to 0002, part 1: the DM screen tables and the
-- campaign edition. Statements captured from the result of the Python app's Alembic migration 0002's
-- result; don't edit by hand. Part 2 (moving session encounters, then dropping
-- sessions.encounter) is in db.rs.

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

CREATE INDEX ix_handouts_campaign_id ON handouts (campaign_id);

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

CREATE INDEX ix_monsters_campaign_id ON monsters (campaign_id);

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

CREATE INDEX ix_npcs_campaign_id ON npcs (campaign_id);

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

CREATE INDEX ix_notes_campaign_id ON notes (campaign_id);

CREATE INDEX ix_notes_session_id ON notes (session_id);

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

CREATE INDEX ix_scenes_campaign_id ON scenes (campaign_id);

CREATE INDEX ix_scenes_session_id ON scenes (session_id);

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

CREATE INDEX ix_trackers_campaign_id ON trackers (campaign_id);

CREATE INDEX ix_trackers_session_id ON trackers (session_id);

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

CREATE INDEX ix_encounters_campaign_id ON encounters (campaign_id);

CREATE INDEX ix_encounters_scene_id ON encounters (scene_id);

CREATE INDEX ix_encounters_session_id ON encounters (session_id);

ALTER TABLE campaigns ADD COLUMN edition VARCHAR DEFAULT '2014' NOT NULL;
