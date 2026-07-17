// Wire types for the Sprint 14 script-block editor, mirroring the backend
// `script_tree.rs` serde shapes (camelCase over IPC). Kept in a plain .ts module
// so the edit-emission logic (scriptEdits.ts) is unit-testable without a
// component or a running backend.

/** How a known trigger/effect key's argument is entered — mirrors backend `ArgKind`. */
export type ArgKind = "bool" | "number" | "tag" | "string" | "block" | "comparison";

/** A leaf's typed value (mirrors backend `TypedValue`). `kind` is one of
 *  bool | number | tag | scope | string | block; `text` is the value as written
 *  (quotes stripped for `string`, braces-inclusive for a `block` value). */
export interface TypedValue {
  kind: "bool" | "number" | "tag" | "scope" | "string" | "block";
  text: string;
}

/** One node of the typed script tree (mirrors backend `TreeNode`). */
export interface TreeNode {
  /** `group` | `leaf`. */
  nodeType: "group" | "leaf";
  /** Statement key; `null` for a bare list element or an anonymous `{ … }`. */
  key: string | null;
  /** Byte-surgical path from the addressed root block to this node. Block
   *  ancestors carry `#n` occurrence suffixes. Empty for non-addressable
   *  bare/anonymous elements. */
  path: string[];
  /** Group classification (empty for leaves): and/or/not/nand/nor/scope/
   *  quantifier/hidden/tooltip/control/limit/calc/anonymous. */
  groupKind: string;
  /** Typed value (leaves only; `null` for groups). */
  value: TypedValue | null;
  /** Child nodes (groups only). */
  children: TreeNode[];
  /** Raw statement text (raw/tree toggle + preserve-unknown editing). */
  raw: string;
}

/** The typed tree of one script block plus its raw slice (mirrors backend
 *  `ScriptBlock`). */
export interface ScriptBlock {
  /** Direct children of the addressed block. */
  nodes: TreeNode[];
  /** Braces-inclusive raw text of the addressed block. */
  raw: string;
  /** The block's braces-inclusive byte span `[start, end]` in the source file. */
  span: [number, number];
}

/** One curated known trigger/effect key (mirrors backend `KnownKey`). */
export interface KnownKey {
  key: string;
  argKind: ArgKind;
  displayName: string;
}

/** Result of `validate_script_text` (mirrors backend `ScriptValidation`). */
export interface ScriptValidation {
  valid: boolean;
  error?: string | null;
}

/** Which registry a tree editor draws its known keys from. */
export type Registry = "triggers" | "effects";
