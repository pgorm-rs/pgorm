// @ts-check

// The graph shapes the application's own module registers: a
// `SelectGraph<E, S>` whose root and slots are fixed in Rust, found by name,
// queried under the aliases JavaScript chooses, decoded by Rust's
// `GraphRow` — a slotless row as the root's record, otherwise a tuple of
// each source's record, an absent `Opt` slot null — and paged by keyset
// through `SelectGraph::cursor_by`.
// [spec:pgorm:req:napi.entity-graphs]

import { arg, TRUSTED } from "./builder.js";
import { runJob } from "./connections.js";
import { predicateOf, recordOf } from "./entities.js";
import { Expr, OrderBy } from "./expressions.js";
import { native } from "./operations.js";

/** Marks a construction from within the module. */
const MADE = Symbol("pgorm-napi entity graph");

/**
 * The rows of a graph job, `[columns per source, ([values, model] | null)[][]]`.
 *
 * @param {readonly string[]} entities
 * @param {[string[][], ([unknown[], unknown] | null)[][]]} outcome
 */
function rowsOf(entities, [columns, rows]) {
  return rows.map((row) => {
    const records = row.map((pair, index) =>
      pair === null ? null : recordOf(/** @type {string} */ (entities[index]), /** @type {string[]} */ (columns[index]), pair)
    );
    return records.length === 1 ? records[0] : records;
  });
}

/** The names of the graph shapes this module registers. */
export function graphs() {
  return /** @type {string[]} */ (native.graphNames(native.registry));
}

/**
 * The graph shape this module registers as `name`.
 *
 * @param {string} name
 */
export function graph(name) {
  if (typeof name !== "string") throw new TypeError("a graph's name is a string");
  return new EntityGraph(native.graphGet(native.registry, name), name, MADE);
}

/** A registered graph shape. */
export class EntityGraph {
  /** @type {unknown} */
  #native;
  /** @type {string} */
  #name;
  /** @type {readonly string[]} */
  #entities;

  /**
   * @param {unknown} handle
   * @param {string} name
   * @param {symbol} token
   */
  constructor(handle, name, token) {
    if (token !== MADE) throw new TypeError("an EntityGraph comes from graph(name)");
    this.#native = handle;
    this.#name = name;
    this.#entities = Object.freeze(this.describe().sources.map((/** @type {{ entity: string }} */ source) => source.entity));
    Object.freeze(this);
  }

  get name() {
    return this.#name;
  }

  /** The shape as data: its Rust type, and each source's entity and slot kind. */
  describe() {
    return JSON.parse(native.graphDescribe(this.#native));
  }

  /**
   * The graph built by its application factory, each joined slot under the
   * alias given for it — `g1`, `g2`, .. when none are.
   *
   * @param {{ aliases?: readonly string[] }} [options]
   */
  find({ aliases } = {}) {
    if (aliases !== undefined && !Array.isArray(aliases)) throw new TypeError("aliases is an array of names");
    return new EntityGraphQuery(native.graphFind(this.#native, aliases ?? null), this.#entities, MADE);
  }
}

/** A query over a registered graph. Every method returns a new query. */
export class EntityGraphQuery {
  /** @type {unknown} */
  #native;
  /** @type {readonly string[]} */
  #entities;

  /**
   * @param {unknown} handle
   * @param {readonly string[]} entities
   * @param {symbol} token
   */
  constructor(handle, entities, token) {
    if (token !== MADE) throw new TypeError("an EntityGraphQuery comes from a graph's find()");
    this.#native = handle;
    this.#entities = entities;
  }

  /** The aliases its slots are joined under, in slot order. */
  get aliases() {
    return /** @type {string[]} */ (native.graphQueryAliases(this.#native));
  }

  /**
   * @param {string} step
   * @param {unknown} argument
   */
  #step(step, argument) {
    return new EntityGraphQuery(native.graphQueryChange(this.#native, step, argument), this.#entities, MADE);
  }

  /** @param {unknown} predicate */
  where(predicate) {
    return this.#step("where", arg(predicateOf(predicate)));
  }

  /** @param {...unknown} orderings */
  orderBy(...orderings) {
    /** @type {EntityGraphQuery} */
    let query = this;
    for (const ordering of orderings) {
      if (!(ordering instanceof OrderBy)) throw new TypeError("an ordering is expr.asc() or expr.desc()");
      query = query.#step("orderBy", arg(ordering));
    }
    return query;
  }

  /**
   * A column of decoded source `source` — 0 the root, i the i-th slot —
   * qualified as the query names it.
   *
   * @param {number} source
   * @param {string} column
   */
  col(source, column) {
    return new Expr(native.graphQueryCol(this.#native, source, column), TRUSTED);
  }

  /**
   * The SQL and values a terminal sends, `oneOpt` with its `LIMIT 1`.
   *
   * @param {"all" | "oneOpt"} [terminal]
   */
  inspect(terminal = "all") {
    if (terminal !== "all" && terminal !== "oneOpt") throw new TypeError('a graph inspects "all" or "oneOpt"');
    const [sql, values] = native.graphQueryInspect(this.#native, terminal === "oneOpt");
    return Object.freeze({ sql, values: Object.freeze(values) });
  }

  /**
   * Every row, by `SelectGraph::all`.
   *
   * @param {unknown} db
   * @param {{ signal?: AbortSignal }} [options]
   */
  async all(db, options) {
    return rowsOf(this.#entities, await runJob(db, native.graphQueryJob(this.#native, false), options));
  }

  /**
   * The first row or null, by `SelectGraph::one_opt`: no row is apart from
   * a row whose optional slot is absent.
   *
   * @param {unknown} db
   * @param {{ signal?: AbortSignal }} [options]
   */
  async oneOpt(db, options) {
    return rowsOf(this.#entities, await runJob(db, native.graphQueryJob(this.#native, true), options))[0] ?? null;
  }

  /**
   * A keyset cursor ordered by a root column, every source's primary key
   * after it as `cursor_by` installs them.
   *
   * @param {string} column
   */
  cursor(column) {
    return new EntityGraphCursor(native.graphCursorNew(this.#native, column), this.#entities, MADE);
  }
}

/** A keyset cursor over a registered graph. Every method returns a new cursor. */
export class EntityGraphCursor {
  /** @type {unknown} */
  #native;
  /** @type {readonly string[]} */
  #entities;

  /**
   * @param {unknown} handle
   * @param {readonly string[]} entities
   * @param {symbol} token
   */
  constructor(handle, entities, token) {
    if (token !== MADE) throw new TypeError("an EntityGraphCursor comes from a graph query's cursor()");
    this.#native = handle;
    this.#entities = entities;
  }

  /** @param {unknown} handle */
  #next(handle) {
    return new EntityGraphCursor(handle, this.#entities, MADE);
  }

  /**
   * Rows short of the one whose order-column value this is.
   *
   * @param {unknown} value
   */
  before(value) {
    return this.#next(native.graphCursorBound(this.#native, true, [value], false));
  }

  /**
   * Rows past the one whose order-column value this is.
   *
   * @param {unknown} value
   */
  after(value) {
    return this.#next(native.graphCursorBound(this.#native, false, [value], false));
  }

  /**
   * Rows short of the one whose whole key this is: the order column, the
   * root's other key columns, then each slot's.
   *
   * @param {...unknown} values
   */
  beforeWith(...values) {
    return this.#next(native.graphCursorBound(this.#native, true, values, true));
  }

  /** @param {...unknown} values */
  afterWith(...values) {
    return this.#next(native.graphCursorBound(this.#native, false, values, true));
  }

  /** @param {number} rows */
  first(rows) {
    return this.#next(native.graphCursorWindow(this.#native, false, rows));
  }

  /** @param {number} rows */
  last(rows) {
    return this.#next(native.graphCursorWindow(this.#native, true, rows));
  }

  asc() {
    return this.#next(native.graphCursorDirection(this.#native, false));
  }

  desc() {
    return this.#next(native.graphCursorDirection(this.#native, true));
  }

  /**
   * The window's rows by `Cursor::all`, a `last` window's in the cursor's
   * order.
   *
   * @param {unknown} db
   * @param {{ signal?: AbortSignal }} [options]
   */
  async all(db, options) {
    return rowsOf(this.#entities, await runJob(db, native.graphCursorJob(this.#native), options));
  }
}
