CREATE SCHEMA sales;
CREATE TYPE sales.stage AS ENUM ('lead', 'won', 'lost');
CREATE TABLE sales.deal (
    id integer PRIMARY KEY,
    title text NOT NULL,
    stage sales.stage NOT NULL,
    history sales.stage[]
);
