// The session's typed pending-edit queue (Svelte 5 runes store).
//
// One ordered queue of *composite* operations per session. A composite is one
// undo unit — a ruler rename is a single scalar-ish edit; a brush stroke or a
// create-entity scaffold (later sprints) bundle many TypedEdits under one label.
// Undo/redo operate on whole composites. `serialize()` flattens the applied
// composites, in order, into the wire payload for `save_project`.
//
// The queue is also the source of truth for *pending* state: panels, map
// repaint and hit-testing must read base + mod + pending. The projection API
// (`pendingScalar`, `pendingLocOverride`, `pendingRulerName`, and the generic
// `findLast`) answers "what would this value be after the pending edits?".
// Reads inside a `$derived`/`$effect` track the queue and recompute on change;
// `version` is an explicit counter for consumers that prefer to depend on it.

/** One typed, serializable edit. Mirrors the backend `edits::TypedEdit`. */
export type TypedEdit =
  | { kind: "setScalar"; file: string; path: string[]; value: string; quoted: boolean }
  | { kind: "setBlock"; file: string; path: string[]; value: string }
  | {
      kind: "removeStatement";
      file: string;
      blockPath: string[];
      key: string;
      value?: string | null;
    }
  | { kind: "insertStatement"; file: string; blockPath: string[]; statement: string }
  // Insert a new top-level `Y.M.D = { ... }` block in date order (Sprint 12.3
  // edit-at-date). Mirrors backend `TypedEdit::InsertDatedBlock`.
  | { kind: "insertDatedBlock"; file: string; date: string; statement: string }
  // Override an `NDefines.<namespace>.<key>` define (Sprint 12.1/12.4 dates,
  // generalized to any namespace by Sprint 28's Defines editor). `namespace`
  // defaults to `NGame` when omitted. Mirrors backend `TypedEdit::SetDefine`.
  | { kind: "setDefine"; key: string; value: string; namespace?: string }
  | { kind: "addId"; file: string; listPath: string[]; id: string }
  | { kind: "removeId"; file: string; listPath: string[]; id: string }
  | {
      kind: "listMove";
      fromFile: string;
      fromPath: string[];
      toFile: string;
      toPath: string[];
      id: string;
    }
  | { kind: "appendText"; file: string; text: string }
  | { kind: "createFile"; file: string; text: string }
  // Line-surgical rewrite of a semicolon CSV (map/adjacencies.csv, Sprint 25).
  // `rows` is the FULL desired row list; the backend re-emits unchanged origin
  // rows byte-for-byte. Mirrors backend `TypedEdit::CsvRewrite`.
  | { kind: "csvRewrite"; file: string; rows: import("$lib/adjnet").AdjRowInput[] }
  // Delete a project file (Sprint 13.2 war deletion; project-only). Mirrors
  // backend `TypedEdit::DeleteFile`.
  | { kind: "deleteFile"; file: string }
  | { kind: "locOverride"; key: string; value: string }
  // Remove a toolkit-owned loc override key (S2.1 country deletion; project-only).
  // Mirrors backend `TypedEdit::LocRemove`.
  | { kind: "locRemove"; key: string }
  | { kind: "binaryAsset"; file: string; bytes: number[] }
  // Rewrite map/provinces.bmp by replaying color-space pixel ops against the
  // copy-on-write base bitmap (Province Colors add/expand/dissolve). The frontend
  // ships semantic ops, never the bitmap. Mirrors backend `TypedEdit::ProvinceBmp`.
  | { kind: "provinceBmp"; file: string; ops: BmpOp[] }
  | { kind: "renameRuler"; tag: string; name: string };

/** One color-space province-bitmap op. Mirrors backend `province_edit::BmpOp`. */
export type BmpOp =
  // Set every listed pixel (top-down flat index `y*width + x`) to `color`.
  | { op: "paint"; pixels: number[]; color: [number, number, number] }
  // Reassign every pixel of `from` among `into`, each to the nearest target.
  | { op: "dissolve"; from: [number, number, number]; into: [number, number, number][] };

/** One undo unit: a human label plus the edits it applies, in order. */
export interface Composite {
  label: string;
  edits: TypedEdit[];
  /**
   * Optional coalescing key. When the just-pushed composite carries the same
   * key as the current top-of-undo composite, it *replaces* it rather than
   * stacking — so a continuous gesture (dragging a color slider, which emits an
   * onchange per pixel) collapses into a single undo unit.
   */
  coalesceKey?: string;
  /**
   * Sprint 12.3: the selected date ("Y.M.D") this composite was made at, when it
   * edits date-aware history (province/country). Undefined for date-agnostic
   * edits (static map/common files, create-entity scaffolds). The pending-edit
   * folds apply a composite to the view only when its date ≤ the selected view
   * date, so an edit made at 1444 disappears when viewed at 1300 (it still saves).
   */
  date?: string;
}

function samePath(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((v, i) => v === b[i]);
}

/** Key of a `key = value` statement string, or "" when malformed. */
function statementKey(statement: string): string {
  const eq = statement.indexOf("=");
  return eq < 0 ? "" : statement.slice(0, eq).trim();
}

/** Value of a `key = value` statement string, or "" when malformed. */
function statementValue(statement: string): string {
  const eq = statement.indexOf("=");
  return eq < 0 ? "" : statement.slice(eq + 1).trim();
}

export class EditQueue {
  // Applied composites, oldest first. Redo holds composites that were undone.
  #undo = $state<Composite[]>([]);
  #redo = $state<Composite[]>([]);
  // Bumped on every mutation so `undo -> redo` (which keeps the total count
  // constant) still notifies version-based consumers.
  #rev = $state(0);

  /** Monotonic change counter; read it in a `$derived` to recompute on change. */
  get version(): number {
    return this.#rev;
  }

  /** True when there are unsaved (queued) edits. */
  get dirty(): boolean {
    return this.#undo.length > 0;
  }

  get canUndo(): boolean {
    return this.#undo.length > 0;
  }

  get canRedo(): boolean {
    return this.#redo.length > 0;
  }

  /** Label of the composite that Undo would revert, for menu affordances. */
  get undoLabel(): string | null {
    return this.#undo.at(-1)?.label ?? null;
  }

  /** Label of the composite that Redo would re-apply. */
  get redoLabel(): string | null {
    return this.#redo.at(-1)?.label ?? null;
  }

  /** Appends a composite; pushing a new edit truncates the redo stack. When the
   *  composite carries a `coalesceKey` matching the current top-of-undo, it
   *  replaces that composite instead of stacking (one undo unit per gesture). */
  push(composite: Composite): void {
    const last = this.#undo.at(-1);
    if (
      composite.coalesceKey !== undefined &&
      last &&
      last.coalesceKey === composite.coalesceKey
    ) {
      this.#undo[this.#undo.length - 1] = composite;
    } else {
      this.#undo.push(composite);
    }
    this.#redo = [];
    this.#rev++;
  }

  undo(): void {
    const c = this.#undo.pop();
    if (!c) return;
    this.#redo.push(c);
    this.#rev++;
  }

  redo(): void {
    const c = this.#redo.pop();
    if (!c) return;
    this.#undo.push(c);
    this.#rev++;
  }

  /**
   * Drops every applied composite matching `pred` (and clears the redo stack).
   * Returns how many were removed. Used to delete a pending-created entity by
   * removing its whole create composite from the queue (S2.1 country deletion) —
   * each composite is a self-contained undo unit, so dropping one is safe.
   */
  removeWhere(pred: (c: Composite) => boolean): number {
    const before = this.#undo.length;
    this.#undo = this.#undo.filter((c) => !pred(c));
    const removed = before - this.#undo.length;
    if (removed > 0) {
      this.#redo = [];
      this.#rev++;
    }
    return removed;
  }

  /**
   * The applied composites, oldest first — for the Edits panel (Sprint 30.1) to
   * render one row per composite. Returns a shallow copy so callers can't mutate
   * the queue; reading it inside a `$derived`/`$effect` tracks queue changes.
   */
  get composites(): readonly Composite[] {
    return this.#undo.slice();
  }

  /**
   * Rewinds the linear queue to just before `target` (Sprint 30.1 "undo to
   * here"): every composite from the top down to and including `target` is
   * undone (moved to the redo stack, newest-undone on top), so it's fully
   * reversible via redo. No-op if `target` isn't an applied composite. Always
   * safe — it's exactly N sequential `undo()`s.
   */
  undoToBefore(target: Composite): void {
    const index = this.#undo.indexOf(target);
    if (index < 0) return;
    while (this.#undo.length > index) this.undo();
  }

  /**
   * Removes a single applied composite (Sprint 30.1 "revert this edit alone").
   * The caller must have proven independence (`isIndependentlyRevertible`) — this
   * just drops it and clears redo. Returns true if it was present. Reuses
   * `removeWhere` with reference identity, so each composite stays a self-
   * contained undo unit.
   */
  revertComposite(target: Composite): boolean {
    return this.removeWhere((c) => c === target) > 0;
  }

  /** Drops all pending state (used after a successful save). */
  clear(): void {
    this.#undo = [];
    this.#redo = [];
    this.#rev++;
  }

  /** The applied edits flattened in queue order — the Save payload. Always the
   *  FULL queue, regardless of any per-composite date (saving is date-agnostic). */
  serialize(): TypedEdit[] {
    return this.#undo.flatMap((c) => c.edits);
  }

  /**
   * Sprint 12.3 fold gating: the applied edits flattened in queue order, but only
   * from composites `isVisible(composite.date)` accepts. A composite with no date
   * (date-agnostic edit) is passed its `undefined` date; callers keep those
   * always-visible. Used by the pending-edit folds so a dated edit shows in the
   * view only when its date ≤ the selected view date. `serialize()` (the save
   * payload) is unaffected — gating is view-only.
   */
  serializeVisibleAt(isVisible: (compositeDate: string | undefined) => boolean): TypedEdit[] {
    return this.#undo.filter((c) => isVisible(c.date)).flatMap((c) => c.edits);
  }

  // --- Projection API (base + mod + PENDING) -----------------------------

  /**
   * The last edit (across all applied composites, in order) matching `pred`.
   * Later edits win, mirroring how the backend applies the queue sequentially.
   * Generic so future consumers (e.g. "pending owner of province N") build
   * their own lookups without extending this class.
   */
  findLast(pred: (e: TypedEdit) => boolean): TypedEdit | undefined {
    for (let i = this.#undo.length - 1; i >= 0; i--) {
      const edits = this.#undo[i].edits;
      for (let j = edits.length - 1; j >= 0; j--) {
        if (pred(edits[j])) return edits[j];
      }
    }
    return undefined;
  }

  /** Pending value of a scalar at `file`/`path`, or undefined if none queued. */
  pendingScalar(file: string, path: string[]): string | undefined {
    const hit = this.findLast(
      (e) => e.kind === "setScalar" && e.file === file && samePath(e.path, path),
    );
    return hit?.kind === "setScalar" ? hit.value : undefined;
  }

  /** Pending block value at `file`/`path` (setBlock), or undefined if none. */
  pendingBlockValue(file: string, path: string[]): string | undefined {
    const hit = this.findLast(
      (e) => e.kind === "setBlock" && e.file === file && samePath(e.path, path),
    );
    return hit?.kind === "setBlock" ? hit.value : undefined;
  }

  /**
   * Effective pending state of a single top-level field `key` in `file`,
   * folding setScalar / insertStatement / removeStatement (later wins). Returns
   * `undefined` when the queue doesn't touch it (caller falls back to base);
   * `{ value: null }` means a pending removal. Drives the Government/Identity
   * field displays (which mix set-on-present with insert-on-absent).
   */
  pendingField(file: string, key: string): { value: string | null } | undefined {
    const hit = this.findLast((e) => {
      if (e.kind === "setScalar")
        return e.file === file && e.path.length === 1 && e.path[0] === key;
      if (e.kind === "insertStatement")
        return e.file === file && e.blockPath.length === 0 && statementKey(e.statement) === key;
      if (e.kind === "removeStatement")
        return e.file === file && e.blockPath.length === 0 && e.key === key && e.value == null;
      return false;
    });
    if (!hit) return undefined;
    if (hit.kind === "removeStatement") return { value: null };
    if (hit.kind === "setScalar") return { value: hit.value };
    if (hit.kind === "insertStatement") return { value: statementValue(hit.statement) };
    return undefined;
  }

  /**
   * Effective membership of a repeated top-level key (`add_government_reform`,
   * `add_accepted_culture`, `historical_rival`/`historical_friend`, …): the base
   * list plus queued inserts minus queued value-filtered removals, in order.
   */
  pendingList(file: string, key: string, base: string[]): string[] {
    const out = base.slice();
    for (const e of this.serialize()) {
      if (e.kind === "insertStatement" && e.file === file && e.blockPath.length === 0) {
        if (statementKey(e.statement) === key) {
          const v = statementValue(e.statement);
          if (v && !out.includes(v)) out.push(v);
        }
      } else if (
        e.kind === "removeStatement" &&
        e.file === file &&
        e.blockPath.length === 0 &&
        e.key === key &&
        e.value != null
      ) {
        const i = out.indexOf(e.value);
        if (i >= 0) out.splice(i, 1);
      }
    }
    return out;
  }

  /** Pending localized-name override for `key`, or undefined. */
  pendingLocOverride(key: string): string | undefined {
    const hit = this.findLast((e) => e.kind === "locOverride" && e.key === key);
    return hit?.kind === "locOverride" ? hit.value : undefined;
  }

  /** Pending starting-ruler name for `tag`, or undefined. */
  pendingRulerName(tag: string): string | undefined {
    const hit = this.findLast((e) => e.kind === "renameRuler" && e.tag === tag);
    return hit?.kind === "renameRuler" ? hit.name : undefined;
  }
}
