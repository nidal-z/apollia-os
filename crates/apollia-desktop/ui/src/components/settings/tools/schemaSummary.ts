/**
 * Turns a JSON Schema document into a flat list of readable field rows.
 *
 * The tool contract sheet must show an operator what a tool accepts, not a wall
 * of braces. This module walks `properties` in declaration order, resolves a
 * short type label per field, tracks the `required` set of the owning object,
 * and flattens nested objects (and object-shaped array items) into indented
 * rows keyed by a dotted path.
 *
 * Anything the walker cannot read as an object schema yields an empty list, and
 * the caller falls back to the raw JSON view.
 */

/** One displayable field of a JSON Schema. */
export interface SchemaField {
  /** Dotted path from the schema root, e.g. `options.depth`. Unique per row. */
  path: string;
  /** Leaf name shown in the row. */
  name: string;
  /** Nesting level, 0 for a top-level property. */
  depth: number;
  /** Short type label, e.g. `string`, `integer`, `string[]`, `object`. */
  type: string;
  /** True when the owning object lists this field in its `required` array. */
  required: boolean;
  /** Schema `description`, empty when absent. */
  description: string;
  /** Allowed values when the schema constrains them, `null` otherwise. */
  enumValues: string[] | null;
  /** Serialized `default`, `null` when the schema declares none. */
  defaultValue: string | null;
}

/** Maximum nesting the walker descends into before stopping. */
const MAX_DEPTH = 3;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function asStringList(value: unknown): string[] {
  return Array.isArray(value)
    ? value.filter((v): v is string => typeof v === "string")
    : [];
}

function scalarLabel(value: unknown): string {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  if (value === null) return "null";
  return JSON.stringify(value) ?? "";
}

/** Resolves the short type label of a schema node. */
export function typeLabel(schema: unknown): string {
  if (!isRecord(schema)) return "any";

  const variants = schema.anyOf ?? schema.oneOf;
  if (Array.isArray(variants) && variants.length > 0) {
    const labels = variants.map(typeLabel).filter((l) => l !== "any");
    if (labels.length > 0) return [...new Set(labels)].join(" | ");
  }

  const raw = schema.type;
  if (Array.isArray(raw)) {
    const labels = asStringList(raw);
    if (labels.length > 0) return labels.join(" | ");
  }

  if (raw === "array") {
    return `${typeLabel(schema.items)}[]`;
  }

  if (typeof raw === "string") return raw;
  if (Array.isArray(schema.enum)) return "enum";
  if (isRecord(schema.properties)) return "object";
  return "any";
}

/** Object node a nested field row descends into, when there is one. */
function childObject(schema: Record<string, unknown>): {
  node: Record<string, unknown>;
  suffix: string;
} | null {
  if (isRecord(schema.properties)) return { node: schema, suffix: "" };
  if (schema.type === "array" && isRecord(schema.items)) {
    const items = schema.items;
    if (isRecord(items.properties)) return { node: items, suffix: "[]" };
  }
  return null;
}

function walk(
  node: Record<string, unknown>,
  prefix: string,
  depth: number,
  out: SchemaField[],
): void {
  const properties = node.properties;
  if (!isRecord(properties)) return;
  const required = new Set(asStringList(node.required));

  for (const [name, rawField] of Object.entries(properties)) {
    const field = isRecord(rawField) ? rawField : {};
    const path = prefix ? `${prefix}.${name}` : name;
    const enumValues = Array.isArray(field.enum)
      ? field.enum.map(scalarLabel)
      : null;

    out.push({
      path,
      name,
      depth,
      type: typeLabel(field),
      required: required.has(name),
      description:
        typeof field.description === "string" ? field.description : "",
      enumValues: enumValues && enumValues.length > 0 ? enumValues : null,
      defaultValue:
        field.default === undefined ? null : scalarLabel(field.default),
    });

    if (depth + 1 >= MAX_DEPTH) continue;
    const child = childObject(field);
    if (child) walk(child.node, `${path}${child.suffix}`, depth + 1, out);
  }
}

/**
 * Flattens a JSON Schema into indented field rows.
 *
 * Returns an empty array when the document is not an object schema with
 * `properties`, which is the caller's signal to render the raw JSON instead.
 */
export function flattenSchema(schema: unknown): SchemaField[] {
  if (!isRecord(schema)) return [];
  const out: SchemaField[] = [];
  walk(schema, "", 0, out);
  return out;
}

/** True when the schema document carries something worth displaying. */
export function hasSchema(schema: unknown): boolean {
  if (schema === null || schema === undefined) return false;
  if (isRecord(schema)) return Object.keys(schema).length > 0;
  return true;
}

/** Pretty-prints a schema document for the raw JSON view. */
export function prettyJson(value: unknown): string {
  try {
    return JSON.stringify(value, null, 2) ?? "";
  } catch {
    return "";
  }
}
