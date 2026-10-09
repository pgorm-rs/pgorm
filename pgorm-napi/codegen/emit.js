// @ts-check

// Write a project's typed module from its built native library: one export
// per registration — an entity, a graph shape, a source tuple — and
// declarations typing each from what the library describes, its records'
// columns, their nullability and an enum's labels, a graph's and a source
// tuple's rows. The module checks the library against what it was written
// from as it loads.
// [spec:pgorm:req:napi.codegen]
// [spec:pgorm:req:napi.codegen-types]

import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

import { CodegenError } from "./config.js";
import { MARKER } from "./scaffold.js";
import { columnType, propertyKey } from "./types.js";

const HEADER = "// Written by pgorm-napi's codegen from the application's native module; regenerate rather than edit.";

/**
 * @typedef {{ name: string, kind: string | null, nullable: boolean, values?: readonly string[] | null }} Column
 * @typedef {{ name: string, columns: readonly Column[] }} EntityDescription
 */

/**
 * The record and input interfaces of one entity.
 *
 * @param {string} name
 * @param {string} exported
 * @param {EntityDescription} description
 */
function entityTypes(name, exported, description) {
  const fields = (/** @type {boolean} */ write) =>
    description.columns.map((column) => `  readonly ${propertyKey(column.name)}: ${columnType(column, write)};`);
  return [
    `/** A record of \`${name}\`: its columns by SQL name, as its reads and writes give them. */`,
    `export interface ${exported}Record {`,
    ...fields(false),
    "}",
    "",
    `/** What each column of \`${name}\` takes, written or compared. */`,
    `export interface ${exported}Input {`,
    ...fields(true),
    "}",
    "",
    `/** The entity registered as \`${name}\`. */`,
    `export declare const ${exported}: Entity<{ record: ${exported}Record; input: ${exported}Input }>;`,
    "",
  ];
}

/**
 * The record type of the entity registered as `name`.
 *
 * @param {Map<string, string>} exports
 * @param {string} name
 * @param {string} what
 */
function recordOf(exports, name, what) {
  const exported = exports.get(name);
  if (exported === undefined) throw new CodegenError(`${what} reads ${name}, which the build description does not export`);
  return `${exported}Record`;
}

/**
 * Write the project's module from its built library.
 *
 * @param {string} project
 */
export async function emit(project) {
  const metadata = join(project, MARKER);
  if (!existsSync(metadata)) throw new CodegenError(`${project} is no scaffolded project`);
  const description = JSON.parse(readFileSync(metadata, "utf8"));
  const lib = join(project, "lib");
  if (!existsSync(join(lib, "pgorm_napi.node"))) {
    throw new CodegenError(`${project} has no native library: build it before emitting`);
  }
  const registrations = await import(pathToFileURL(join(lib, "registrations.js")).href);
  /** @type {{ entities: { name: string, rust: string, typescript: string }[], graphs: { name: string, typescript: string }[], sources: { name: string, typescript: string }[] }} */
  const { entities, graphs, sources } = description;
  const named = {
    entities: entities.map((entry) => entry.name),
    graphs: graphs.map((entry) => entry.name),
    sources: sources.map((entry) => entry.name),
  };
  const described = registrations.describeRegistrations(named);
  const fingerprint = registrations.registrationsFingerprint(named);
  const exports = new Map(entities.map((entry) => [entry.name, entry.typescript]));
  /** @type {string[]} */
  const declarations = [];
  entities.forEach((entry, index) => declarations.push(...entityTypes(entry.name, entry.typescript, described.entities[index])));
  graphs.forEach((entry, index) => {
    /** @type {{ entity: string, slot: string }[]} */
    const slots = described.graphs[index].sources;
    const records = slots.map((source) => {
      const record = recordOf(exports, source.entity, entry.name);
      return source.slot === "Opt" ? `${record} | null` : record;
    });
    const row = records.length === 1 ? records[0] : `[${records.join(", ")}]`;
    declarations.push(
      `/** A row of the graph \`${entry.name}\`: ${records.length === 1 ? "its root's record" : "each source's record, an absent optional slot null"}. */`,
      `export type ${entry.typescript}Row = ${row};`,
      `/** The graph shape registered as \`${entry.name}\`. */`,
      `export declare const ${entry.typescript}: EntityGraph<${entry.typescript}Row>;`,
      "",
    );
  });
  sources.forEach((entry, index) => {
    /** @type {string[]} */
    const listed = described.sources[index].entities;
    const records = listed.map((name) => `${recordOf(exports, name, entry.name)} | null`);
    declarations.push(
      `/** A row the source tuple \`${entry.name}\` selects: each source's record, or null where the row lacks it. */`,
      `export type ${entry.typescript}Row = [${records.join(", ")}];`,
      `/** The source tuple registered as \`${entry.name}\`, for a pipeline's selectSources. */`,
      `export declare const ${entry.typescript}: SourceSelection<${entry.typescript}Row>;`,
      "",
    );
  });
  const body = declarations.join("\n");
  const used = (/** @type {string[]} */ names) => names.filter((name) => new RegExp(`\\b${name}\\b`).test(body));
  const types = used(["Decimal", "Entity", "EntityGraph", "Interval", "JsonValue", "Multirange", "PlainValue", "Range", "Uuid"]);
  const imports = [`import type { ${types.join(", ")} } from "./index.js";`];
  if (sources.length > 0) imports.push('import type { SourceSelection } from "./pipeline.d.ts";');
  const module = description.module;
  const code = [
    `// @ts-self-types="./${module}.d.ts"`,
    HEADER,
    "",
    'import { entity, graph, pipeline } from "./index.js";',
    'import { checkRegistrations } from "./registrations.js";',
    "",
    `checkRegistrations(${JSON.stringify(named)}, ${JSON.stringify(fingerprint)}, ${JSON.stringify(module)});`,
    "",
    ...entities.map((entry) => `export const ${entry.typescript} = entity(${JSON.stringify(entry.name)});`),
    ...graphs.map((entry) => `export const ${entry.typescript} = graph(${JSON.stringify(entry.name)});`),
    ...sources.map((entry) => `export const ${entry.typescript} = pipeline.sources(${JSON.stringify(entry.name)});`),
    "",
  ];
  writeFileSync(join(lib, `${module}.d.ts`), `${[HEADER, "", ...imports, "", body.trimEnd()].join("\n")}\n`);
  writeFileSync(join(lib, `${module}.js`), code.join("\n"));
  return join(lib, `${module}.js`);
}
