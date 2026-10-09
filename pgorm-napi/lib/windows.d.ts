/**
 * Windows and their frames, and the functions PostgreSQL computes only over
 * one.
 *
 * @module
 */

// [spec:pgorm:req:napi.typing]
// [spec:pgorm:req:napi.windows]

import type { Expr, Operand, OrderBy } from "./expressions.d.ts";

/** An `OVER` window; every method returns a new window. */
export declare class Window {
  constructor();
  partitionBy(...expressions: Expr[]): Window;
  orderBy(...orderings: OrderBy[]): Window;
  /** The frame, replacing any: a whole frame, or a preceding or current-row start standing alone. */
  frame(frame: Frame | FramePrecedingStart | FrameCurrentRowStart): Window;
}

/** The rows EXCLUDE removes, relative to the current row's peers. */
export type FrameExclusion = "currentRow" | "group" | "ties" | "noOthers";

/** A whole frame. */
export declare class Frame {
  private constructor();
  exclude(exclusion: FrameExclusion): Frame;
}

/** A start after the current row: only a following end may follow it, and it cannot stand alone. */
export declare class FrameFollowingStart {
  protected constructor();
  andFollowing(offset: Operand): Frame;
  andUnboundedFollowing(): Frame;
}

/** A start at the current row: any end but a preceding one may follow it. */
export declare class FrameCurrentRowStart extends FrameFollowingStart {
  andCurrentRow(): Frame;
  exclude(exclusion: FrameExclusion): Frame;
}

/** A start before the current row: every end may follow it. */
export declare class FramePrecedingStart extends FrameCurrentRowStart {
  andPreceding(offset: Operand): Frame;
}

/**
 * A frame's mode, and where a frame of it begins. An offset is a count under
 * `rows` and `groups`, a distance in the ordering column's values under
 * `range`. There is no unbounded-following start.
 */
export declare class FrameType {
  private constructor();
  static readonly rows: FrameType;
  static readonly range: FrameType;
  static readonly groups: FrameType;
  unboundedPreceding(): FramePrecedingStart;
  preceding(offset: Operand): FramePrecedingStart;
  currentRow(): FrameCurrentRowStart;
  following(offset: Operand): FrameFollowingStart;
}

/** A call under OVER, a SELECT item. */
export declare class Windowed {
  private constructor();
  as(alias: string): Windowed;
}

/** A function PostgreSQL computes only over a window; its one method is `over`. */
export declare class WindowFunction {
  private constructor();
  over(window: Window | string): Windowed;
}

/** The general-purpose window functions, at the argument counts each takes. */
export type WindowFunctionName =
  | "row_number"
  | "rank"
  | "dense_rank"
  | "percent_rank"
  | "cume_dist"
  | "ntile"
  | "first_value"
  | "last_value"
  | "nth_value"
  | "lag"
  | "lead";

export declare function windowFunction(name: WindowFunctionName, ...operands: Operand[]): WindowFunction;
