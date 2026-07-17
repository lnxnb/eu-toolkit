// Pure edit-emission logic for the script-block tree editor (Sprint 14.2).
//
// The ScriptTreeEditor component is PURE with respect to the pending-edit queue:
// props in (a parsed ScriptBlock + the known-key registry), edit-batches out via
// its `onedit(edits, label)` callback — exactly the IdeaBlockEditor own-block-
// commit precedent. The HOST owns the queue (wraps each batch in a composite and
// re-parses the block). Keeping the tree-node → TypedEdit mapping here (not buried
// in the component) means it can be reasoned about and tested as plain functions.
//
// All paths are byte-surgical and `#n`-qualified exactly as the backend emits
// them, so the four mappings below compose with mod_writer without re-parsing:
//   • scalar leaf value change  → setScalar   { path }
//   • block-valued leaf change  → setBlock    { path }  (value = inner tokens)
//   • add a condition / group   → insertStatement { blockPath }
//   • delete a node             → removeStatement { blockPath, key, value? }
//   • whole-block raw edit       → setBlock    { path: rootPath } (value = inner)

import type { TypedEdit } from "$lib/edits.svelte";
import type { ArgKind, KnownKey, TreeNode } from "./scriptTypes";

/** A group's visual badge: a short label + a Windows-classic chip color. */
export interface GroupBadge {
  label: string;
  color: string;
}

/** The parent block path of a node (drop the node's own final segment). Used as
 *  the `blockPath` for insert/remove edits addressing a sibling of the node. */
export function parentPath(node: TreeNode): string[] {
  return node.path.slice(0, -1);
}

/** True when a leaf carries a `{ … }` block argument (edited via setBlock, not
 *  setScalar). */
export function isBlockLeaf(node: TreeNode): boolean {
  return node.nodeType === "leaf" && node.value?.kind === "block";
}

/** True when the node is path-addressable (has a non-empty path + a key). Bare
 *  list elements / anonymous `{ }` blocks are not — they are shown read-only and
 *  edited only through the whole-block raw editor. */
export function isAddressable(node: TreeNode): boolean {
  return node.path.length > 0 && node.key != null;
}

/** Strips the outer `{ … }` from a braces-inclusive block slice, returning the
 *  inner tokens. `set_block_value` re-wraps the value in `{ }`, so a block edit
 *  must supply the INNER content (else we'd double the braces). Defensive: if the
 *  text isn't brace-wrapped it is returned trimmed as-is. */
export function stripOuterBraces(raw: string): string {
  const t = raw.trim();
  const open = t.indexOf("{");
  const close = t.lastIndexOf("}");
  if (open === 0 && close === t.length - 1 && close > open) {
    return t.slice(open + 1, close).trim();
  }
  return t;
}

/** Whether a string value needs quoting to survive as a single token. */
function needsQuote(value: string): boolean {
  return /\s/.test(value) || value === "";
}

/** A sensible default value literal for a freshly-added condition of `argKind`.
 *  The user edits it immediately after; it only has to parse. */
export function defaultForArg(argKind: ArgKind | undefined): string {
  switch (argKind) {
    case "bool":
      return "yes";
    case "number":
    case "comparison":
      return "0";
    case "tag":
      return "ROOT";
    case "block":
      return "{ }";
    case "string":
    default:
      return "yes";
  }
}

// --- Edit builders ---------------------------------------------------------

/** setScalar for a scalar leaf's value change. `argKind` decides quoting. */
export function setScalarEdit(
  file: string,
  node: TreeNode,
  value: string,
  argKind: ArgKind | undefined,
): TypedEdit {
  const quoted = argKind === "string" && needsQuote(value);
  return { kind: "setScalar", file, path: node.path, value, quoted };
}

/** setBlock for a block-valued leaf's change. `blockText` is braces-inclusive
 *  (what the user edits); the outer braces are stripped for the wire value. */
export function setBlockLeafEdit(file: string, node: TreeNode, blockText: string): TypedEdit {
  return { kind: "setBlock", file, path: node.path, value: stripOuterBraces(blockText) };
}

/** setBlock replacing the WHOLE edited block (the raw/tree toggle commit).
 *  `rootPath` addresses `key = { … }`; `blockText` is the edited braces-inclusive
 *  slice, whose outer braces are stripped for the wire value. */
export function setWholeBlockEdit(
  file: string,
  rootPath: string[],
  blockText: string,
): TypedEdit {
  return { kind: "setBlock", file, path: rootPath, value: stripOuterBraces(blockText) };
}

/** insertStatement adding one condition/effect into the block at `blockPath`. */
export function insertConditionEdit(
  file: string,
  blockPath: string[],
  key: string,
  argKind: ArgKind | undefined,
): TypedEdit {
  return {
    kind: "insertStatement",
    file,
    blockPath,
    statement: `${key} = ${defaultForArg(argKind)}`,
  };
}

/** insertStatement adding an empty logical group (`AND`/`OR`/`NOT`/…) into the
 *  block at `blockPath`. */
export function insertGroupEdit(
  file: string,
  blockPath: string[],
  combinator: string,
): TypedEdit {
  return {
    kind: "insertStatement",
    file,
    blockPath,
    statement: `${combinator} = { }`,
  };
}

/** removeStatement deleting `node` from its parent block.
 *
 *  The removed `key` is the node's OWN final path segment, not the bare key — for
 *  a block node (a logical group / block-valued leaf) that segment is already
 *  occurrence-qualified (`OR#1`), so deleting the 2nd of two same-key sibling
 *  groups is addressable (14B gap fix; backend `remove_statement` splits the `#n`
 *  suffix). A scalar leaf's segment is the bare key (scalar leaves carry no
 *  occurrence in their path); to disambiguate repeated scalar keys we additionally
 *  pass an unambiguous ASCII value as a filter (e.g. two `add_prestige`).
 *  Strings/blocks omit the value filter (quote/encoding risk), falling back to
 *  first-match like the dynasty-delete path. */
export function removeNodeEdit(file: string, node: TreeNode): TypedEdit {
  const key = node.path.length > 0 ? node.path[node.path.length - 1] : (node.key ?? "");
  let value: string | null = null;
  if (
    node.nodeType === "leaf" &&
    node.value &&
    (node.value.kind === "number" ||
      node.value.kind === "bool" ||
      node.value.kind === "tag" ||
      node.value.kind === "scope")
  ) {
    value = node.value.text;
  }
  return { kind: "removeStatement", file, blockPath: parentPath(node), key, value };
}

// --- Presentation ----------------------------------------------------------

/** The Windows-classic chip for a group node: a short label + a color. Scope /
 *  quantifier groups show their own key (a tag, a province id, `any_owned_…`). */
export function groupBadge(node: TreeNode): GroupBadge {
  const key = node.key ?? "";
  switch (node.groupKind) {
    case "and":
      return { label: "AND", color: "#4a6da7" };
    case "or":
      return { label: "OR", color: "#b8863b" };
    case "not":
      return { label: "NOT", color: "#a13636" };
    case "nand":
      return { label: "NAND", color: "#a13636" };
    case "nor":
      return { label: "NOR", color: "#a13636" };
    case "hidden":
      return { label: "HIDDEN", color: "#5a6470" };
    case "tooltip":
      return { label: "TOOLTIP", color: "#5a6470" };
    case "control":
      return { label: key.toUpperCase() || "IF", color: "#7a6a3f" };
    case "limit":
      return { label: "LIMIT", color: "#7a6a3f" };
    case "calc":
      return { label: "CALC", color: "#6d5aa1" };
    case "quantifier":
      return { label: key, color: "#6d5aa1" };
    case "scope":
      return { label: key, color: "#3f8a6d" };
    case "anonymous":
    default:
      return { label: "{ }", color: "#5a6470" };
  }
}

/** The combinators offered by the "＋ Group" affordance. */
export const GROUP_COMBINATORS = ["AND", "OR", "NOT", "NAND", "NOR"] as const;
