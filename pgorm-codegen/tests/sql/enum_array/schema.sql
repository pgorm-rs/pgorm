CREATE TYPE mood AS ENUM ('sad', 'calm', 'glad');
CREATE TYPE crm.tier AS ENUM ('free', 'pro', 'team');
CREATE DOMAIN mood_log AS mood[];
CREATE TABLE survey (
    id integer PRIMARY KEY,
    moods mood[] NOT NULL,
    past_moods mood[],
    tiers crm.tier[] NOT NULL,
    past_tiers crm.tier[],
    log mood_log
);
