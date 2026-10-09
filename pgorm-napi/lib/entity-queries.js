// @ts-check

// A registered entity's reads: its columns as expressions whose
// comparisons are the column's own `ColumnTrait` methods, and its
// `Select<E>` with Rust's terminals.
// [spec:pgorm:req:napi.entity-reads]

import { arg, TRUSTED } from "./builder.js";
import { runJob } from "./connections.js";
import { predicateOf, recordsOf } from "./entities.js";
import { Expr, OrderBy } from "./expressions.js";
import { native } from "./operations.js";

/** Marks a construction from a registered entity. */
export const QUERIES = Symbol("pgorm-napi entity query");

/**
 * A registered entity's column. A value it is compared with converts to the
 * column's declared kind and is written through its `save_as`, by the
 * column's own `ColumnTrait` method; an expression is compared as written.
 */
export class EntityColumn extends Expr {
  /** @type {unknown} */
  #entity;
  /** @type {string} */
  #column;

  /**
   * @param {unknown} handle
   * @param {unknown} entity
   * @param {string} column
   * @param {symbol} token
   */
  constructor(handle, entity, column, token) {
    super(handle, token);
    this.#entity = entity;
    this.#column = column;
  }

  /** The column's SQL name. */
  get name() {
    return this.#column;
  }

  /**
   * @param {"eq" | "ne" | "gt" | "gte" | "lt" | "lte"} operator
   * @param {unknown} other
   */
  #compare(operator, other) {
    if (other instanceof Expr) return null;
    return new Expr(native.entityCompare(this.#entity, this.#column, operator, other), TRUSTED);
  }

  /**
   * @override
   * @param {unknown} other
   */
  eq(other) {
    return this.#compare("eq", other) ?? super.eq(other);
  }

  /**
   * @override
   * @param {unknown} other
   */
  ne(other) {
    return this.#compare("ne", other) ?? super.ne(other);
  }

  /**
   * @override
   * @param {unknown} other
   */
  gt(other) {
    return this.#compare("gt", other) ?? super.gt(other);
  }

  /**
   * @override
   * @param {unknown} other
   */
  gte(other) {
    return this.#compare("gte", other) ?? super.gte(other);
  }

  /**
   * @override
   * @param {unknown} other
   */
  lt(other) {
    return this.#compare("lt", other) ?? super.lt(other);
  }

  /**
   * @override
   * @param {unknown} other
   */
  lte(other) {
    return this.#compare("lte", other) ?? super.lte(other);
  }
}

/** A registered entity's `Select<E>`. Every method returns a new query. */
export class EntityQuery {
  /** @type {unknown} */
  #native;
  /** @type {string} */
  #entity;

  /**
   * @param {unknown} handle
   * @param {string} entity
   * @param {symbol} token
   */
  constructor(handle, entity, token) {
    if (token !== QUERIES) throw new TypeError("an EntityQuery comes from an entity's find()");
    this.#native = handle;
    this.#entity = entity;
  }

  /**
   * @param {string} step
   * @param {unknown} argument
   */
  #step(step, argument) {
    return new EntityQuery(native.entitySelectChange(this.#native, step, argument), this.#entity, QUERIES);
  }

  /**
   * Rows that also satisfy `predicate`, by `QueryFilter::filter`.
   *
   * @param {unknown} predicate
   */
  where(predicate) {
    return this.#step("where", arg(predicateOf(predicate)));
  }

  /** @param {...unknown} orderings */
  orderBy(...orderings) {
    /** @type {EntityQuery} */
    let query = this;
    for (const ordering of orderings) {
      if (!(ordering instanceof OrderBy)) throw new TypeError("an ordering is expr.asc() or expr.desc()");
      query = query.#step("orderBy", arg(ordering));
    }
    return query;
  }

  /** @param {number | null} count */
  limit(count) {
    return this.#step("limit", count);
  }

  /** @param {number | null} count */
  offset(count) {
    return this.#step("offset", count);
  }

  /**
   * The SQL and values a terminal sends: `one` and `oneOpt` with the
   * `LIMIT 1` Rust's terminals add.
   *
   * @param {"all" | "one" | "oneOpt"} [terminal]
   */
  inspect(terminal = "all") {
    const [sql, values] = native.entitySelectInspect(this.#native, terminal);
    return Object.freeze({ sql, values: Object.freeze(values) });
  }

  /**
   * @param {"all" | "one" | "oneOpt"} terminal
   * @param {unknown} db
   * @param {unknown} options
   */
  async #read(terminal, db, options) {
    return recordsOf(this.#entity, await runJob(db, native.entitySelectJob(this.#native, terminal), options));
  }

  /**
   * Every row, by `Select::all`.
   *
   * @param {unknown} db
   * @param {{ signal?: AbortSignal }} [options]
   */
  async all(db, options) {
    return await this.#read("all", db, options);
  }

  /**
   * The first row, by `Select::one`; none is a `DecodeError`.
   *
   * @param {unknown} db
   * @param {{ signal?: AbortSignal }} [options]
   */
  async one(db, options) {
    return (await this.#read("one", db, options))[0];
  }

  /**
   * The first row or null, by `Select::one_opt`.
   *
   * @param {unknown} db
   * @param {{ signal?: AbortSignal }} [options]
   */
  async oneOpt(db, options) {
    return (await this.#read("oneOpt", db, options))[0] ?? null;
  }
}

