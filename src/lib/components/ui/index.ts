// Barrel export for the EU Toolkit shared UI kit (Phase 0.6).
// Import components + types from a single path: `$lib/components/ui`.

export { default as SidePanel } from "./SidePanel.svelte";
export { default as ListSection } from "./ListSection.svelte";
export { default as SearchDropdown } from "./SearchDropdown.svelte";
export { default as MultiSelectModal } from "./MultiSelectModal.svelte";
export { default as ModifierEditor } from "./ModifierEditor.svelte";
export { default as ColorPicker } from "./ColorPicker.svelte";
export { default as DatePicker } from "./DatePicker.svelte";
export { default as SliderGroup } from "./SliderGroup.svelte";
export { default as BottomToolbar } from "./BottomToolbar.svelte";
export { default as PromptBanner } from "./PromptBanner.svelte";
export { default as InlineNamePrompt } from "./InlineNamePrompt.svelte";
export { default as NewGroupModal } from "./NewGroupModal.svelte";
export { default as IconImportButton } from "./IconImportButton.svelte";

export { redistribute, roundToTotal } from "./sliderMath";
export { createEntityFlow } from "./createEntityFlow";
export type {
  EntityFlow,
  EntityFlowState,
  EntityFlowPhase,
  EntityFlowConfig,
  Point,
} from "./createEntityFlow";

export { NEW_GROUP_KEY } from "./types";
export type {
  RGB,
  DropdownItem,
  MultiSelectItem,
  ModifierKind,
  KnownModifier,
  ModifierRow,
  ToolButton,
  GroupScaffold,
  NewGroupResult,
} from "./types";
