// How a parity case's built statement is written down: its SQL, and each
// bound value as its kind and a text the Rust test writes the same way for
// the pgorm value it builds, so two runtimes and Rust compare one file.

import type { Builder, Value } from "../../lib/index.js";

export interface Built {
  readonly sql: string;
  readonly values: readonly (readonly [string, string])[];
}

function text(value: unknown): string {
  if (value === null) return "null";
  if (Array.isArray(value)) return `[${value.map(text).join(",")}]`;
  if (typeof value === "object" && !(value instanceof Uint8Array)) {
    const shown = String(value);
    return shown === "[object Object]" ? JSON.stringify(value) : shown;
  }
  return String(value);
}

export function canonical(value: Value): readonly [string, string] {
  return [value.kind, text(value.value)];
}

export function built(builder: Builder): Built {
  const { sql, values } = builder.inspect();
  return { sql, values: values.map(canonical) };
}
