CREATE TABLE interval_span (
    id integer PRIMARY KEY,
    plain interval NOT NULL,
    maybe interval,
    precise interval(3) NOT NULL,
    months interval YEAR TO MONTH NOT NULL,
    days interval DAY NOT NULL,
    clock interval DAY TO SECOND(3),
    minutes interval HOUR TO MINUTE NOT NULL,
    seconds interval SECOND(0) NOT NULL,
    history interval[] NOT NULL,
    budgets interval YEAR TO MONTH[]
);
