use pgorm_sql_macro::sql;

const SELECT: &str = sql!("SELECT id, name FROM cake WHERE id = $1");

const MULTI: &str = sql!(
    "CREATE TABLE cake (id int primary key);
     INSERT INTO cake (id) VALUES (1);"
);

// The grammar has no catalog, so a statement against a table that does not
// exist is still well-formed SQL.
const UNKNOWN: &str = sql!("SELECT nowhere FROM no_such_table");

// Comments and an empty statement list are what the grammar says they are.
const COMMENT_ONLY: &str = sql!("-- nothing to run here");

fn main() {
    assert_eq!(SELECT, "SELECT id, name FROM cake WHERE id = $1");
    assert!(MULTI.contains("INSERT INTO cake"));
    assert_eq!(UNKNOWN, "SELECT nowhere FROM no_such_table");
    assert_eq!(COMMENT_ONLY, "-- nothing to run here");
}
