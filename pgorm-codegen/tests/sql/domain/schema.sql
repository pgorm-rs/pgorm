CREATE TYPE mood AS ENUM ('sad', 'calm', 'glad');
CREATE TYPE floatrange AS RANGE (SUBTYPE = float8);
CREATE DOMAIN quantity AS integer NOT NULL DEFAULT 1 CONSTRAINT positive CHECK (VALUE > 0);
CREATE DOMAIN small_quantity AS quantity CHECK (VALUE < 100);
CREATE DOMAIN code AS varchar(8) COLLATE "C" CHECK (VALUE ~ '^[A-Z]+$');
CREATE DOMAIN cheerful AS mood CHECK (VALUE <> 'sad');
CREATE DOMAIN labels AS text[] CHECK (cardinality(VALUE) <= 3);
CREATE DOMAIN span AS floatrange CHECK (NOT isempty(VALUE));
CREATE DOMAIN billing.amount AS numeric(12, 2) CHECK (VALUE >= 0);
CREATE TABLE stock_line (
    id integer PRIMARY KEY,
    qty quantity,
    small small_quantity NOT NULL,
    code code NOT NULL,
    codes code[] NOT NULL,
    mood cheerful NOT NULL,
    moods cheerful[],
    labels labels,
    span span,
    amount billing.amount NOT NULL
);
