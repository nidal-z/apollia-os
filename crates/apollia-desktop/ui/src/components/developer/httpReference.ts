// The HTTP reference of the documentation pane: the pages the reader builds
// from the OpenAPI document of the archive, one per group of operations.
//
// The public site generates these pages when it is built, so the archive
// holds no source for them: it holds the document they come from. Everything
// here is a reading of that document, kept apart from the components so
// Vitest holds it without rendering anything.

import type {
  DocumentationManifest,
  DocumentationNavEntry,
  DocumentationTreeItem,
} from "$lib/ipc/documentation";

// ── The document, as far as the reader reads it ───────────────────────────

/** The HTTP methods an OpenAPI path item declares an operation under. */
export const HTTP_METHODS = [
  "get",
  "post",
  "put",
  "patch",
  "delete",
  "head",
  "options",
  "trace",
] as const;
export type HttpMethod = (typeof HTTP_METHODS)[number];

type Json = Record<string, unknown>;

function isObject(value: unknown): value is Json {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function text(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function list(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

// ── What a page shows ──────────────────────────────────────────────────────

/** Where a parameter travels in a request. */
export type ParameterPlace = "path" | "query" | "header" | "cookie" | "body";

/** A parameter of an operation, or a field of its JSON body. */
export interface ReferenceParameter {
  name: string;
  where: ParameterPlace;
  /** The type as the contract writes it: `string`, `integer`, a schema name. */
  type: string;
  required: boolean;
  description: string;
}

/** The body an operation takes. */
export interface ReferenceBody {
  /** The media type the fields are read from, `application/json` when offered. */
  mediaType: string;
  type: string;
  required: boolean;
}

/** An answer of an operation. */
export interface ReferenceResponse {
  /** The status code, or `default`. */
  status: string;
  description: string;
  /** The type of the answer body, `null` when the answer has none. */
  type: string | null;
}

/** An operation: a method on a path. */
export interface ReferenceOperation {
  /** The name the contract gives it, which the contract may repeat. */
  id: string;
  method: HttpMethod;
  path: string;
  summary: string;
  description: string;
  parameters: ReferenceParameter[];
  body: ReferenceBody | null;
  responses: ReferenceResponse[];
}

/** A group of operations: a tag of the contract, and a page of the reference. */
export interface ReferenceGroup {
  tag: string;
  /** The route of its page, the one the public site serves it at. */
  route: string;
  title: string;
  description: string;
  operations: ReferenceOperation[];
}

/** The whole reference. */
export interface HttpReference {
  title: string;
  description: string;
  version: string;
  /** The route of the overview, the one the public site serves it at. */
  overviewRoute: string;
  groups: ReferenceGroup[];
}

/** The page of the reference a route shows. */
export type ReferencePage =
  | { kind: "overview"; reference: HttpReference }
  | { kind: "group"; reference: HttpReference; group: ReferenceGroup };

// ── Routes and titles ──────────────────────────────────────────────────────

/** The folder the public site serves the reference under. */
export const REFERENCE_ROOT = "/reference/api";

/** The tag of an operation that declares none. */
const UNTAGGED = "default";

/** The tags that are initials, written in capitals. */
const INITIALS = new Set(["a2a", "ai", "api", "llm", "mcp", "stt"]);

/**
 * The slug the public site gives a name: its words in lower case, joined by
 * hyphens, a run of digits being a word of its own (`a2a` reads `a-2-a`).
 */
export function referenceSlug(name: string): string {
  const words = name.match(/[A-Z]?[a-z]+|[A-Z]+(?![a-z])|[0-9]+/g) ?? [];
  return words.map((word) => word.toLowerCase()).join("-");
}

/** The title of a group, from its tag: `model_hub` reads `Model hub`. */
export function groupTitle(tag: string): string {
  const words = tag.split(/[_\s-]+/).filter(Boolean);
  const written = words.map((word, index) => {
    if (INITIALS.has(word.toLowerCase())) return word.toUpperCase();
    return index === 0 ? word.charAt(0).toUpperCase() + word.slice(1) : word;
  });
  return written.length === 0 ? tag : written.join(" ");
}

// ── Types ──────────────────────────────────────────────────────────────────

const REF_PREFIX = "#/components/schemas/";
/** How many references the reader follows before it stops on a cycle. */
const MAX_REF_DEPTH = 8;

function schemasOf(document: unknown): Json {
  if (!isObject(document) || !isObject(document.components)) return {};
  return isObject(document.components.schemas) ? document.components.schemas : {};
}

/** The schema `schema` names, its references followed. */
function resolveSchema(schema: unknown, schemas: Json): Json | null {
  let current = schema;
  for (let depth = 0; depth < MAX_REF_DEPTH; depth += 1) {
    if (!isObject(current)) return null;
    const ref = text(current.$ref);
    if (ref === "") return current;
    if (!ref.startsWith(REF_PREFIX)) return null;
    current = schemas[ref.slice(REF_PREFIX.length)];
  }
  return null;
}

/**
 * The type of a schema as the contract writes it: the name of a schema it
 * refers to, `string`, `integer[]`, or the alternatives joined by a bar.
 */
export function schemaType(schema: unknown): string {
  if (!isObject(schema)) return "any";
  const ref = text(schema.$ref);
  if (ref !== "") return ref.slice(ref.lastIndexOf("/") + 1);

  for (const [key, joiner] of [
    ["oneOf", " | "],
    ["anyOf", " | "],
    ["allOf", " & "],
  ] as const) {
    const parts = list(schema[key])
      .filter((part) => !(isObject(part) && part.type === "null"))
      .map(schemaType);
    if (parts.length > 0) return [...new Set(parts)].join(joiner);
  }

  const declared = Array.isArray(schema.type) ? schema.type : [schema.type];
  const types = declared.filter((type): type is string => typeof type === "string" && type !== "null");
  const type = types.join(" | ");
  if (type === "array") return `${schemaType(schema.items)}[]`;
  if (type !== "") return type;
  if (isObject(schema.properties)) return "object";
  if (Array.isArray(schema.enum)) return "enum";
  return "any";
}

// ── Reading an operation ───────────────────────────────────────────────────

const PLACES: readonly string[] = ["path", "query", "header", "cookie"];
const JSON_TYPE = "application/json";

function declaredParameters(operation: Json, pathItem: Json): ReferenceParameter[] {
  const out: ReferenceParameter[] = [];
  for (const raw of [...list(pathItem.parameters), ...list(operation.parameters)]) {
    if (!isObject(raw)) continue;
    const where = text(raw.in);
    if (!PLACES.includes(where)) continue;
    out.push({
      name: text(raw.name),
      where: where as ParameterPlace,
      type: schemaType(raw.schema),
      required: raw.required === true,
      description: text(raw.description),
    });
  }
  return out;
}

/** The media type the reader shows of a content map: JSON when offered. */
function shownMediaType(content: unknown): string | null {
  if (!isObject(content)) return null;
  const types = Object.keys(content);
  if (types.length === 0) return null;
  return types.includes(JSON_TYPE) ? JSON_TYPE : types[0];
}

function mediaSchema(content: unknown, mediaType: string): unknown {
  const media = isObject(content) ? content[mediaType] : null;
  return isObject(media) ? media.schema : undefined;
}

function bodyOf(operation: Json): { body: ReferenceBody | null; schema: unknown } {
  const request = operation.requestBody;
  if (!isObject(request)) return { body: null, schema: undefined };
  const mediaType = shownMediaType(request.content);
  if (mediaType === null) return { body: null, schema: undefined };
  const schema = mediaSchema(request.content, mediaType);
  return {
    body: { mediaType, type: schemaType(schema), required: request.required === true },
    schema,
  };
}

/** The fields of a JSON body, each as a parameter that travels in the body. */
function bodyFields(schema: unknown, schemas: Json): ReferenceParameter[] {
  const resolved = resolveSchema(schema, schemas);
  if (!resolved || !isObject(resolved.properties)) return [];
  const required = list(resolved.required);
  return Object.entries(resolved.properties).map(([name, property]) => ({
    name,
    where: "body" as const,
    type: schemaType(property),
    required: required.includes(name),
    description:
      text(isObject(property) ? property.description : "") ||
      text(resolveSchema(property, schemas)?.description),
  }));
}

function responsesOf(operation: Json): ReferenceResponse[] {
  if (!isObject(operation.responses)) return [];
  return Object.entries(operation.responses).map(([status, raw]) => {
    const response = isObject(raw) ? raw : {};
    const mediaType = shownMediaType(response.content);
    const schema = mediaType === null ? undefined : mediaSchema(response.content, mediaType);
    let type: string | null = null;
    if (mediaType !== null) type = schema === undefined ? mediaType : schemaType(schema);
    return { status, description: text(response.description), type };
  });
}

/** Markdown code marks, which the contract writes in its summaries. */
function plain(written: string): string {
  return written.replaceAll("`", "").trim();
}

function readOperation(
  method: HttpMethod,
  path: string,
  operation: Json,
  pathItem: Json,
  schemas: Json,
): ReferenceOperation {
  const { body, schema } = bodyOf(operation);
  const fields = body?.mediaType === JSON_TYPE ? bodyFields(schema, schemas) : [];
  return {
    id: text(operation.operationId) || `${method}-${referenceSlug(path)}`,
    method,
    path,
    summary: plain(text(operation.summary)),
    description: plain(text(operation.description)),
    parameters: [...declaredParameters(operation, pathItem), ...fields],
    body,
    responses: responsesOf(operation),
  };
}

// ── The reference ──────────────────────────────────────────────────────────

/**
 * The reference an OpenAPI document describes: its operations grouped by
 * their first tag, the groups in the order the document declares its tags.
 * `null` when the document declares no operation.
 */
export function buildHttpReference(document: unknown): HttpReference | null {
  if (!isObject(document) || !isObject(document.paths)) return null;
  const schemas = schemasOf(document);
  const info = isObject(document.info) ? document.info : {};

  const groups = new Map<string, ReferenceGroup>();
  const group = (tag: string, description = ""): ReferenceGroup => {
    let known = groups.get(tag);
    if (!known) {
      known = {
        tag,
        route: `${REFERENCE_ROOT}/${referenceSlug(tag) || UNTAGGED}`,
        title: groupTitle(tag),
        description,
        operations: [],
      };
      groups.set(tag, known);
    }
    return known;
  };
  for (const declared of list(document.tags)) {
    if (isObject(declared) && text(declared.name) !== "") {
      group(text(declared.name), text(declared.description));
    }
  }

  for (const [path, pathItem] of Object.entries(document.paths)) {
    if (!isObject(pathItem)) continue;
    for (const method of HTTP_METHODS) {
      const operation = pathItem[method];
      if (!isObject(operation)) continue;
      const tag = text(list(operation.tags)[0]) || UNTAGGED;
      group(tag).operations.push(readOperation(method, path, operation, pathItem, schemas));
    }
  }

  const filled = [...groups.values()].filter((entry) => entry.operations.length > 0);
  if (filled.length === 0) return null;
  const title = text(info.title);
  return {
    title,
    description: text(info.description),
    version: text(info.version),
    overviewRoute: `${REFERENCE_ROOT}/${referenceSlug(title) || "overview"}`,
    groups: filled,
  };
}

/** How many operations the reference holds. */
export function operationCount(reference: HttpReference): number {
  return reference.groups.reduce((sum, entry) => sum + entry.operations.length, 0);
}

/** The page of the reference served at `route`, `null` when it serves none. */
export function referencePageAt(
  reference: HttpReference | null,
  route: string,
): ReferencePage | null {
  if (!reference) return null;
  if (route === reference.overviewRoute) return { kind: "overview", reference };
  const group = reference.groups.find((entry) => entry.route === route);
  return group ? { kind: "group", reference, group } : null;
}

// ── Its place in the documentation ─────────────────────────────────────────

/** The kind the navigation gives a page of the reference. */
export const REFERENCE_NAV_KIND = "http-reference";

/** What the tree calls the reference and its first page. */
export interface ReferenceLabels {
  /** The label of the category. */
  category: string;
  /** The label of the overview. */
  overview: string;
}

/**
 * `manifest` with the reference in it: a category at the end of the tree,
 * the overview then one entry per group, and their titles with the page
 * before and the page after. A manifest that already lists a route of the
 * reference keeps its own entry.
 */
export function withHttpReference(
  manifest: DocumentationManifest,
  reference: HttpReference,
  labels: ReferenceLabels,
): DocumentationManifest {
  const stops = [
    { route: reference.overviewRoute, title: labels.overview },
    ...reference.groups.map((entry) => ({ route: entry.route, title: entry.title })),
  ].filter((stop) => manifest.navigation[stop.route] === undefined);
  if (stops.length === 0) return manifest;

  const navigation: Record<string, DocumentationNavEntry> = { ...manifest.navigation };
  for (const [index, stop] of stops.entries()) {
    navigation[stop.route] = {
      kind: REFERENCE_NAV_KIND,
      title: stop.title,
      previous: index > 0 ? stops[index - 1] : null,
      next: index < stops.length - 1 ? stops[index + 1] : null,
    };
  }
  const category: DocumentationTreeItem = {
    kind: "category",
    label: labels.category,
    route: null,
    items: stops.map((stop) => ({ kind: "doc", route: stop.route, label: stop.title })),
  };
  return { ...manifest, tree: [...manifest.tree, category], navigation };
}

// ── What a page writes ─────────────────────────────────────────────────────

/** The i18n key of the name of each place a parameter travels in. */
export const PLACE_LABELS: Record<ParameterPlace, string> = {
  path: "dev.developer.docs.reference.where.path",
  query: "dev.developer.docs.reference.where.query",
  header: "dev.developer.docs.reference.where.header",
  cookie: "dev.developer.docs.reference.where.cookie",
  body: "dev.developer.docs.reference.where.body",
};

/** The i18n key of what each method does, shown when its badge is hovered. */
export const METHOD_EXPLANATIONS: Record<HttpMethod, string> = {
  get: "dev.developer.docs.reference.method.get",
  post: "dev.developer.docs.reference.method.post",
  put: "dev.developer.docs.reference.method.put",
  patch: "dev.developer.docs.reference.method.patch",
  delete: "dev.developer.docs.reference.method.delete",
  head: "dev.developer.docs.reference.method.other",
  options: "dev.developer.docs.reference.method.other",
  trace: "dev.developer.docs.reference.method.other",
};

/** What names a group in a test id. */
export function groupSlug(group: ReferenceGroup): string {
  return group.route.slice(REFERENCE_ROOT.length + 1);
}
