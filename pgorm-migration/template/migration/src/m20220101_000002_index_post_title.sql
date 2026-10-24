-- Replace this sample with your own SQL. The file is checked against
-- PostgreSQL's grammar when the crate compiles, and its checksum is recorded
-- when it is applied: once it has run, write a new migration rather than
-- editing this one.
CREATE INDEX post_title_index ON post (title);
