CREATE TYPE floatrange AS RANGE (SUBTYPE = float8, SUBTYPE_DIFF = float8mi);
CREATE TYPE booking.slot AS RANGE (SUBTYPE = int4);
CREATE TABLE ledger (
    id integer PRIMARY KEY,
    span interval NOT NULL,
    maybe_span interval,
    months interval YEAR TO MONTH NOT NULL,
    spans interval[] NOT NULL,
    ints int4range NOT NULL,
    bigs int8range,
    nums numrange NOT NULL,
    days daterange NOT NULL,
    stamps tsrange NOT NULL,
    instants tstzrange,
    int_set int4multirange NOT NULL,
    big_set int8multirange,
    num_set nummultirange NOT NULL,
    day_set datemultirange NOT NULL,
    stamp_set tsmultirange NOT NULL,
    instant_set tstzmultirange,
    int_ranges int4range[] NOT NULL,
    day_sets datemultirange[],
    float_set floatmultirange,
    floats floatrange NOT NULL,
    slot booking.slot,
    slots booking.slot_multirange NOT NULL
);
