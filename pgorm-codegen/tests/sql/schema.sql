CREATE TYPE task_state AS ENUM ('open', 'closed');

CREATE TABLE owner (
    id serial PRIMARY KEY,
    name varchar(255) NOT NULL,
    email text
);

CREATE TABLE task (
    id bigserial PRIMARY KEY,
    owner_id integer NOT NULL REFERENCES owner (id) ON DELETE CASCADE ON UPDATE RESTRICT,
    title text NOT NULL,
    state task_state NOT NULL,
    weight double precision,
    tags text[],
    due timestamptz,
    body jsonb,
    ref uuid
);

CREATE TABLE label (
    id serial PRIMARY KEY,
    name text NOT NULL
);

CREATE TABLE task_label (
    task_id bigint NOT NULL,
    label_id integer NOT NULL,
    PRIMARY KEY (task_id, label_id),
    CONSTRAINT task_label_task_fk FOREIGN KEY (task_id) REFERENCES task (id),
    CONSTRAINT task_label_label_fk FOREIGN KEY (label_id) REFERENCES label (id)
);

CREATE UNIQUE INDEX owner_email_key ON owner (email);

CREATE INDEX task_due_idx ON task (due);

COMMENT ON TABLE task IS 'work to be done';

COMMENT ON COLUMN task.title IS 'short summary';
