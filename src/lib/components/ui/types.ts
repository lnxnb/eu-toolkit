// Shared types for the EU Toolkit UI kit (Phase 0.6).
// These are the data contracts every kit component speaks; pickers/editors are
// "the same component with data props" per SPRINT.md 0.6.

/** An 8-bit RGB triple (0-255 per channel), as the game stores colors. */
export interface RGB {
  r: number;
  g: number;
  b: number;
}

/** One selectable entry for {@link SearchDropdown} — tag/culture/religion pickers
 *  are this shape with localized labels + flags/swatches. */
export interface DropdownItem {
  /** Stable identity (tag, culture key, religion key, …). */
  key: string;
  /** Human-facing label (localized name). */
  label: string;
  /** Optional color swatch: any CSS color string, e.g. "#aa3311" or "rgb(1,2,3)". */
  swatch?: string;
  /** Optional leading icon (a flag/religion-icon image URL). */
  icon?: string;
}

/** One row in {@link MultiSelectModal} (dynasty modal 1.3 prototype). */
export interface MultiSelectItem {
  key: string;
  label: string;
  /** Optional trailing badge, e.g. dynasty usage count. */
  badge?: string | number;
}

/** The kinds of typed values a modifier can carry (ModifierEditor). */
export type ModifierKind = "percent" | "flat" | "boolean";

/** A known modifier key the editor offers via its searchable dropdown. */
export interface KnownModifier {
  key: string;
  label: string;
  kind: ModifierKind;
}

/** One key/value modifier row (the game-format value is always a string). */
export interface ModifierRow {
  key: string;
  value: string;
}

/** The scaffold payload from `prepare_{religion,culture}_group_scaffold` (S2.3/S2.4). */
export interface GroupScaffold {
  /** Slugified, collision-free new group key. */
  group_key: string;
  /** Display name (== requested name) — used for the group's loc override. */
  group_name: string;
  /** New group block text (authored at column 0), no members yet. */
  block: string;
  /** Game-relative file the sibling group lives in (suggested target). */
  source_file: string;
}

/** Sentinel dropdown key that opens the "+ New group" flow (S2.3/S2.4). */
export const NEW_GROUP_KEY = "__new_group__";

/** Result of the {@link NewGroupModal} form. */
export interface NewGroupResult {
  /** Display name for the new group. */
  name: string;
  /** Sibling group key whose defaults/pools are copied. */
  sibling: string;
  /** Chosen graphical culture (culture groups only; required there). */
  graphicalCulture?: string;
}

/** A tool button in {@link BottomToolbar}. */
export interface ToolButton {
  id: string;
  label: string;
  /** Emoji/text glyph or an image URL for the button face. */
  icon?: string;
  /** Hover tooltip; falls back to `label`. */
  tooltip?: string;
}
