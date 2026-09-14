DROP SCHEMA IF EXISTS fixture CASCADE;
DROP SCHEMA IF EXISTS other CASCADE;
CREATE SCHEMA fixture;
CREATE SCHEMA other;
SET search_path = fixture, pg_catalog;
CREATE TABLE items (id integer PRIMARY KEY, tenant integer NOT NULL, name text NOT NULL, note text NOT NULL);
INSERT INTO items VALUES (1,1,'alice','first'), (2,1,'bob','second'), (3,2,'alice','sentinel');
CREATE TABLE other.items (LIKE items INCLUDING ALL);
INSERT INTO other.items VALUES (4,2,'other','other-schema');
CREATE TABLE stored (value text NOT NULL);
CREATE TYPE "ReviewStatus" AS ENUM ('ready', 'waiting');
CREATE TYPE other."ReviewStatus" AS ENUM ('other');
-- An enum-typed column gives the enum case a WHERE to filter on, so a boundary that
-- closes the injected type name lands in a truth slot rather than in a projection.
CREATE TABLE reviews (id integer PRIMARY KEY, name text NOT NULL, status "ReviewStatus" NOT NULL);
INSERT INTO reviews VALUES (1,'alice','ready'), (2,'bob','waiting'), (3,'carol','ready');
CREATE DOMAIN "QuotedType" AS text;
CREATE FUNCTION echo(text) RETURNS text LANGUAGE sql AS 'SELECT $1';
CREATE TABLE "quoted""table" ("quoted""column" text);
INSERT INTO "quoted""table" VALUES ('quoted-value');
