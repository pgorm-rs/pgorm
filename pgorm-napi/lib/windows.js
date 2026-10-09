// @ts-check

// Windows over pgorm-query's WindowStatement and frame typestate: a frame
// start offers only the ends that may follow it, so no frame whose end comes
// before its start can be built, and a function PostgreSQL computes only over
// a window has no form but `over`.
// [spec:pgorm:req:napi.windows]

import { arg, args, Handle, trusted, TRUSTED } from "./builder.js";
import { native } from "./operations.js";

/** An `OVER` window: PARTITION BY, ORDER BY and a frame. Every method returns a new window. */
export class Window extends Handle {
  /**
   * @param {unknown} [handle]
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    super(token === TRUSTED ? handle : native.windowNew());
  }

  /** @param {...import("./expressions.js").Expr} expressions */
  partitionBy(...expressions) {
    return new Window(native.windowPartitionBy(arg(this), args(expressions)), TRUSTED);
  }

  /** @param {...import("./expressions.js").OrderBy} orderings */
  orderBy(...orderings) {
    return new Window(native.windowOrderBy(arg(this), args(orderings)), TRUSTED);
  }

  /**
   * The frame, replacing any: a `Frame`, or a preceding or current-row start standing alone.
   *
   * @param {Frame | FramePrecedingStart | FrameCurrentRowStart} frame
   */
  frame(frame) {
    return new Window(native.windowFrame(arg(this), arg(frame)), TRUSTED);
  }
}

/** A whole frame: a mode, a start, an end and an optional EXCLUDE. */
export class Frame extends Handle {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "Frame", "a frame start's end");
    super(handle);
  }

  /** @param {"currentRow" | "group" | "ties" | "noOthers"} exclusion */
  exclude(exclusion) {
    return new Frame(native.frameExclude(arg(this), exclusion), TRUSTED);
  }
}

/**
 * @param {Handle} start
 * @param {string} end
 * @param {unknown} [offset]
 */
function end(start, end, offset) {
  return new Frame(native.frameEnd(arg(start), end, arg(offset)), TRUSTED);
}

/** What every frame start offers: the ends after the current row. */
class FrameStart extends Handle {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "frame start", "FrameType's methods");
    super(handle);
  }

  /** @param {unknown} offset */
  andFollowing(offset) {
    return end(this, "following", offset);
  }

  andUnboundedFollowing() {
    return end(this, "unboundedFollowing");
  }
}

/** A start after the current row: only a following end may follow it, and it cannot stand alone. */
export class FrameFollowingStart extends FrameStart {}

/** A start at the current row: any end but a preceding one may follow it, and it stands alone. */
export class FrameCurrentRowStart extends FrameStart {
  andCurrentRow() {
    return end(this, "currentRow");
  }

  /** @param {"currentRow" | "group" | "ties" | "noOthers"} exclusion */
  exclude(exclusion) {
    return new Frame(native.frameExclude(arg(this), exclusion), TRUSTED);
  }
}

/** A start before the current row: every end may follow it, and it stands alone. */
export class FramePrecedingStart extends FrameCurrentRowStart {
  /** @param {unknown} offset */
  andPreceding(offset) {
    return end(this, "preceding", offset);
  }
}

/** A frame's mode, and where a frame of it begins; there is no unbounded-following start. */
export class FrameType {
  /** @type {"rows" | "range" | "groups"} */
  #mode;

  /**
   * @param {"rows" | "range" | "groups"} mode
   * @param {symbol} [token]
   */
  constructor(mode, token) {
    trusted(token, "FrameType", "FrameType.rows, .range and .groups");
    this.#mode = mode;
    Object.freeze(this);
  }

  unboundedPreceding() {
    return new FramePrecedingStart(native.frameStart(this.#mode, "unboundedPreceding"), TRUSTED);
  }

  /**
   * A count under rows and groups, a distance in the ordering column's values under range.
   *
   * @param {unknown} offset
   */
  preceding(offset) {
    return new FramePrecedingStart(native.frameStart(this.#mode, "preceding", arg(offset)), TRUSTED);
  }

  currentRow() {
    return new FrameCurrentRowStart(native.frameStart(this.#mode, "currentRow"), TRUSTED);
  }

  /** @param {unknown} offset */
  following(offset) {
    return new FrameFollowingStart(native.frameStart(this.#mode, "following", arg(offset)), TRUSTED);
  }

  static rows = new FrameType("rows", TRUSTED);
  static range = new FrameType("range", TRUSTED);
  static groups = new FrameType("groups", TRUSTED);
}

/** A call under OVER, a SELECT item. */
export class Windowed extends Handle {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "Windowed", "expr.over(window)");
    super(handle);
  }

  /** @param {string} alias */
  as(alias) {
    return new Windowed(native.windowedAlias(arg(this), alias), TRUSTED);
  }
}

/** A function PostgreSQL computes only over a window; its one method is `over`. */
export class WindowFunction extends Handle {
  /**
   * @param {unknown} handle
   * @param {symbol} [token]
   */
  constructor(handle, token) {
    trusted(token, "WindowFunction", "windowFunction(name, ..)");
    super(handle);
  }

  /** @param {Window | string} window */
  over(window) {
    return new Windowed(native.exprOver(arg(this), arg(window)), TRUSTED);
  }
}

/**
 * `row_number`, `rank`, `dense_rank`, `percent_rank` and `cume_dist` take no
 * argument; `ntile`, `first_value` and `last_value` one; `nth_value` two;
 * `lag` and `lead` one to three.
 *
 * @param {string} name
 * @param {...unknown} operands
 */
export function windowFunction(name, ...operands) {
  return new WindowFunction(native.windowFunction(name, args(operands)), TRUSTED);
}
