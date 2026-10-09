// @ts-check

// The SQL types SQL/JSON's RETURNING and JSON_TABLE's columns name: a built-in
// type by name, with the size it takes, an enum by `TypeName`, or a range type
// a schema created.
// [spec:pgorm:req:napi.sql-json]

import { arg, Handle, TRUSTED } from "./builder.js";
import { native } from "./operations.js";

/** A column type: pgorm-query's `ColumnType`. */
export class DataType extends Handle {
  /**
   * @param {string | import("./values.js").TypeName | import("./values.js").CreatedRange | import("./values.js").CreatedMultirange} kind
   * @param {{ length?: number, precision?: number, scale?: number }} [options]
   * @param {symbol} [token]
   */
  constructor(kind, options = {}, token) {
    if (token === TRUSTED) {
      super(kind);
      return;
    }
    const { length, precision, scale } = options;
    super(native.dataTypeNew(kind, length, precision, scale));
  }

  /** The array of this type. */
  array() {
    return new DataType(/** @type {any} */ (native.dataTypeArray(arg(this))), {}, TRUSTED);
  }
}
