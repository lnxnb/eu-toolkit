// Barrel for the Sprint 14 script-block editor + sprite picker + overlay shell.
// Consumers (Sprints 15–17: Decisions / Events / Missions) import from
// `$lib/components/script`.

export { default as ScriptTreeEditor } from "./ScriptTreeEditor.svelte";
export { default as ScriptNode } from "./ScriptNode.svelte";
export { default as ScriptAdder } from "./ScriptAdder.svelte";
export { default as SpritePicker } from "./SpritePicker.svelte";
export { default as OverlaySurface } from "./OverlaySurface.svelte";
export { default as ScriptedOverlay } from "./ScriptedOverlay.svelte";

export {
  parentPath,
  isBlockLeaf,
  isAddressable,
  stripOuterBraces,
  defaultForArg,
  setScalarEdit,
  setBlockLeafEdit,
  setWholeBlockEdit,
  insertConditionEdit,
  insertGroupEdit,
  removeNodeEdit,
  groupBadge,
  GROUP_COMBINATORS,
} from "./scriptEdits";
export type { GroupBadge } from "./scriptEdits";

export type {
  ArgKind,
  TypedValue,
  TreeNode,
  ScriptBlock,
  KnownKey,
  ScriptValidation,
  Registry,
} from "./scriptTypes";
