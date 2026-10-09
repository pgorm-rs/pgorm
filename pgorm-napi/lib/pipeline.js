// @ts-check

// pgorm's PRQL-shaped pipeline: relation-to-relation stages over sources,
// compiled through prqlc to PostgreSQL SQL. Each stage returns a new
// pipeline. A stage's `With` form calls its function once with a `Binder`,
// whose placeholders belong to that stage alone, as pgorm brands them.
// [spec:pgorm:req:napi.pipeline]
// [spec:pgorm:req:napi.pipeline-binder]

import { arg, args, Builder, Handle, trusted, TRUSTED } from "./builder.js";
import { native } from "./operations.js";
import { PipelineExpr } from "./pipeline-expressions.js";

export {
  alias,
  average,
  caseWhen,
  col,
  count,
  countDistinct,
  countRows,
  first,
  lag,
  last,
  lead,
  literal,
  max,
  min,
  PipelineExpr,
  rank,
  rankDense,
  rowNumber,
  stddev,
  sum,
  thatColumn,
  thisColumn,
} from "./pipeline-expressions.js";

/**
 * @typedef {import("./select.js").Table | string | Pipeline | Source} SourceInput
 * @typedef {(binder: Binder) => unknown} BinderFunction
 */

/** Mints a stage's placeholders; it binds only while its `With` function runs. */
export class Binder {
  /** @type {unknown} */
  #scope;

  /**
   * @param {unknown} scope
   * @param {symbol} [token]
   */
  constructor(scope, token) {
    trusted(token, "Binder", "a pipeline stage's With form");
    this.#scope = scope;
  }

  /**
   * One placeholder for one value, usable only in the expressions the
   * function returns to its stage.
   *
   * @param {unknown} value
   */
  bind(value) {
    return new PipelineExpr(native.pipelineBind(this.#scope, arg(value)), TRUSTED);
  }
}

/**
 * Call a `With` function with a binder whose scope closes as it returns or
 * throws, and hand its result to `apply` with the scope: one expression, or a
 * list of them for a list-taking stage.
 *
 * @template T
 * @param {BinderFunction} fn
 * @param {boolean} single
 * @param {(result: unknown, scope: unknown) => T} apply
 * @returns {T}
 */
function scoped(fn, single, apply) {
  if (typeof fn !== "function") throw new TypeError("a With stage takes a function of its binder");
  const scope = native.pipelineScopeOpen();
  /** @type {unknown} */
  let result;
  try {
    result = fn(new Binder(scope, TRUSTED));
  } finally {
    native.pipelineScopeClose(scope);
  }
  if (result !== null && typeof result === "object" && typeof Object(result).then === "function") {
    throw new TypeError("a binder's function is synchronous: it returns expressions, not a promise");
  }
  if (single) return apply(arg(result), scope);
  if (result instanceof PipelineExpr) return apply([arg(result)], scope);
  if (!Array.isArray(result)) {
    throw new TypeError("a list stage's function returns an expression or an array of them");
  }
  return apply(args(result), scope);
}

/** @type {(handle: unknown) => Pipeline} */
let pipelineOf;

/** A pipeline grouped and not yet aggregated, which is no relation. */
export class Grouped extends Handle {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "Grouped", "pipeline.group(..)");
    super(handle);
  }

  /** @param {...unknown} aggregates */
  aggregate(...aggregates) {
    return pipelineOf(native.pipelineAggregate(arg(this), args(aggregates)));
  }

  /** @param {BinderFunction} fn */
  aggregateWith(fn) {
    return scoped(fn, false, (list, scope) => pipelineOf(native.pipelineAggregate(arg(this), list, scope)));
  }
}

/** A relation read under a name of its own. */
export class Source extends Handle {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "Source", "pipeline.source(relation)");
    super(handle);
  }

  /** @param {string} name */
  named(name) {
    return new Source(native.pipelineNamed(arg(this), name), TRUSTED);
  }
}

/** A window: its partition, ordering and frame. */
export class Over extends Handle {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "Over", "pipeline.over()");
    super(handle);
  }

  /** @param {...unknown} keys */
  by(...keys) {
    return new Over(native.pipelineOverKeys(arg(this), "by", args(keys)), TRUSTED);
  }

  /** @param {...unknown} keys */
  sortBy(...keys) {
    return new Over(native.pipelineOverKeys(arg(this), "sortBy", args(keys)), TRUSTED);
  }

  /**
   * @param {number | bigint | null} start
   * @param {number | bigint | null} end
   */
  rows(start, end) {
    return new Over(native.pipelineOverFrame(arg(this), "rows", start, end), TRUSTED);
  }

  /**
   * @param {number | bigint | null} start
   * @param {number | bigint | null} end
   */
  range(start, end) {
    return new Over(native.pipelineOverFrame(arg(this), "range", start, end), TRUSTED);
  }
}

/**
 * @param {Pipeline} pipeline
 * @param {"derive" | "select" | "sort"} stage
 * @param {readonly unknown[]} list
 * @param {unknown} [scope]
 */
function listed(pipeline, stage, list, scope) {
  return pipelineOf(native.pipelineList(arg(pipeline), stage, list, scope));
}

/** A pipeline: a relation built stage by stage, run as any statement is. */
export class Pipeline extends Builder {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "Pipeline", "pipeline.from(source)");
    super(handle);
  }

  static {
    pipelineOf = (handle) => new Pipeline(handle, TRUSTED);
  }

  /** @param {unknown} predicate */
  filter(predicate) {
    return pipelineOf(native.pipelineFilter(arg(this), arg(predicate)));
  }

  /** @param {BinderFunction} fn */
  filterWith(fn) {
    return scoped(fn, true, (predicate, scope) => pipelineOf(native.pipelineFilter(arg(this), predicate, scope)));
  }

  /** @param {...unknown} columns */
  derive(...columns) {
    return listed(this, "derive", args(columns));
  }

  /** @param {BinderFunction} fn */
  deriveWith(fn) {
    return scoped(fn, false, (list, scope) => listed(this, "derive", /** @type {unknown[]} */ (list), scope));
  }

  /** @param {...unknown} columns */
  select(...columns) {
    return listed(this, "select", args(columns));
  }

  /** @param {BinderFunction} fn */
  selectWith(fn) {
    return scoped(fn, false, (list, scope) => listed(this, "select", /** @type {unknown[]} */ (list), scope));
  }

  /** @param {...unknown} keys */
  sort(...keys) {
    return listed(this, "sort", args(keys));
  }

  /** @param {BinderFunction} fn */
  sortWith(fn) {
    return scoped(fn, false, (list, scope) => listed(this, "sort", /** @type {unknown[]} */ (list), scope));
  }

  /** @param {...unknown} keys */
  group(...keys) {
    return new Grouped(native.pipelineGroup(arg(this), args(keys)), TRUSTED);
  }

  /** @param {BinderFunction} fn */
  groupWith(fn) {
    return scoped(fn, false, (list, scope) => new Grouped(native.pipelineGroup(arg(this), list, scope), TRUSTED));
  }

  /**
   * @param {Over} over
   * @param {...unknown} columns
   */
  window(over, ...columns) {
    return pipelineOf(native.pipelineWindow(arg(this), arg(over), args(columns)));
  }

  /**
   * @param {Over} over
   * @param {BinderFunction} fn
   */
  windowWith(over, fn) {
    return scoped(fn, false, (list, scope) => pipelineOf(native.pipelineWindow(arg(this), arg(over), list, scope)));
  }

  /** @param {number | bigint} count */
  take(count) {
    return pipelineOf(native.pipelineTake(arg(this), count));
  }

  /**
   * Rows `start` through `end`, counted from 1, both included.
   *
   * @param {number | bigint} start
   * @param {number | bigint} end
   */
  takeRange(start, end) {
    return pipelineOf(native.pipelineTake(arg(this), start, end));
  }

  /**
   * @param {SourceInput} source
   * @param {unknown} on
   * @param {{ kind?: "inner" | "left" | "right" | "full" }} [options]
   */
  join(source, on, { kind = "inner" } = {}) {
    return pipelineOf(native.pipelineJoin(arg(this), kind, arg(source), arg(on)));
  }

  /**
   * @param {SourceInput} source
   * @param {BinderFunction} fn
   * @param {{ kind?: "inner" | "left" | "right" | "full" }} [options]
   */
  joinWith(source, fn, { kind = "inner" } = {}) {
    return scoped(fn, true, (on, scope) => pipelineOf(native.pipelineJoin(arg(this), kind, arg(source), on, scope)));
  }

  /** @param {SourceInput} source */
  append(source) {
    return pipelineOf(native.pipelineSet(arg(this), "append", arg(source)));
  }

  /** @param {SourceInput} source */
  intersect(source) {
    return pipelineOf(native.pipelineSet(arg(this), "intersect", arg(source)));
  }

  /** @param {SourceInput} source */
  remove(source) {
    return pipelineOf(native.pipelineSet(arg(this), "remove", arg(source)));
  }

  distinct() {
    return pipelineOf(native.pipelineDistinct(arg(this)));
  }
}

/** @param {SourceInput} source */
export function from(source) {
  return pipelineOf(native.pipelineFrom(arg(source)));
}

/** @param {SourceInput} relation the relation, which `named` reads under a name of its own */
export function source(relation) {
  return new Source(native.pipelineSource(arg(relation)), TRUSTED);
}

export function over() {
  return new Over(native.pipelineOver(), TRUSTED);
}
