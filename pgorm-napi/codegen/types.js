// @ts-check

// What a registered column's values are in TypeScript, from the kind its
// registration describes: the type a record reads it as, and the type a
// write takes, nullability and an enum's labels included.
// [spec:pgorm:req:napi.codegen-types]

/** What each scalar kind reads as. */
const READS = /** @type {Record<string, string>} */ ({
  bool: "boolean",
  i8: "number",
  i16: "number",
  i32: "number",
  u32: "number",
  f32: "number",
  f64: "number",
  i64: "bigint",
  u64: "bigint",
  text: "string",
  char: "string",
  bytes: "Uint8Array",
  json: "JsonValue",
  decimal: "Decimal",
  uuid: "Uuid",
  date: "Temporal.PlainDate",
  time: "Temporal.PlainTime",
  datetime: "Temporal.PlainDateTime",
  datetime_utc: "Temporal.Instant",
  interval: "Interval",
  ipnetwork: "string",
  mac_address: "Uint8Array",
  vector: "Float32Array",
});

/** What a write takes beyond what a record reads. */
const WRITES = /** @type {Record<string, string>} */ ({
  i64: "bigint | number",
  u64: "bigint | number",
  interval: "Interval | Temporal.Duration",
});

/** Each built-in range's bound. */
const BOUNDS = /** @type {Record<string, string>} */ ({
  int4: "number",
  int8: "bigint",
  num: "Decimal",
  date: "Temporal.PlainDate",
  ts: "Temporal.PlainDateTime",
  tstz: "Temporal.Instant",
});

/**
 * One value of `kind`, not an array, as `table` maps it.
 *
 * @param {string} kind
 * @param {readonly string[] | null} labels
 * @param {Record<string, string>} table
 */
function scalar(kind, labels, table) {
  if (kind.startsWith("enum ")) {
    return labels && labels.length > 0 ? labels.map((label) => JSON.stringify(label)).join(" | ") : "string";
  }
  const created = /^(range|multirange) .* of (\w+)$/.exec(kind);
  if (created) {
    const bound = READS[/** @type {string} */ (created[2])] ?? "PlainValue";
    return created[1] === "range" ? `Range<${bound}>` : `Multirange<${bound}>`;
  }
  const builtIn = /^(int4|int8|num|date|ts|tstz)(range|multirange)$/.exec(kind);
  if (builtIn) {
    const bound = BOUNDS[/** @type {string} */ (builtIn[1])];
    return builtIn[2] === "range" ? `Range<${bound}>` : `Multirange<${bound}>`;
  }
  return table[kind] ?? READS[kind] ?? "PlainValue";
}

/**
 * Whether an array column's items may be NULL: unless the Rust field the
 * entity decodes it into is a `Vec` of a type that is no `Option`.
 *
 * @param {string | null | undefined} rust
 */
function nullableItems(rust) {
  const vec = /^(?:Option<\s*)?(?:std::vec::|alloc::vec::)?Vec<\s*(.*)>\s*>?$/.exec(rust ?? "");
  return vec === null || /^(?:core::option::|std::option::)?Option</.test(/** @type {string} */ (vec[1]));
}

/**
 * The TypeScript type of a column described as `column`: what a record reads
 * it as, or with `write` what a write takes. An array's items are `| null`
 * unless the entity's Rust field holds none.
 *
 * @param {{ kind: string | null, nullable: boolean, values?: readonly string[] | null, rustType?: string | null }} column
 * @param {boolean} write
 */
export function columnType(column, write) {
  const table = write ? WRITES : {};
  let type;
  if (column.kind === null) {
    type = write ? "unknown" : "PlainValue";
  } else if (column.kind.endsWith("[]")) {
    const item = scalar(column.kind.slice(0, -2), column.values ?? null, table);
    type = nullableItems(column.rustType) ? `(${item} | null)[]` : `${item.includes(" | ") ? `(${item})` : item}[]`;
  } else {
    type = scalar(column.kind, column.values ?? null, table);
  }
  return column.nullable && type !== "unknown" ? `${type} | null` : type;
}

/**
 * A property key as TypeScript writes it: bare when it is an identifier,
 * quoted otherwise.
 *
 * @param {string} name
 */
export function propertyKey(name) {
  return /^[A-Za-z_$][A-Za-z0-9_$]*$/.test(name) ? name : JSON.stringify(name);
}
