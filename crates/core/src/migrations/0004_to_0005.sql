-- Upgrade from revision 0004 to 0005: saves made once.
-- The web version's service worker gives each save an id, and sends it again if the window
-- that holds the database closed before answering. A save whose id is here has happened:
-- it's answered with what was answered the first time instead of being made twice. Rows
-- are kept for a day. Unused by the desktop app.
CREATE TABLE saved_requests (
	id VARCHAR(64) NOT NULL, 
	saved VARCHAR NOT NULL, 
	status INTEGER, 
	headers JSON, 
	body BLOB, 
	CONSTRAINT pk_saved_requests PRIMARY KEY (id)
);
