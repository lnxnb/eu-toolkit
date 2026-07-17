// createEntityFlow — a tiny, UI-agnostic state machine wrapping the shared
// "create entity" sequence used by sprints 4, 5.4, 6.4, 8.5, 10:
//
//   idle --arm()-------> awaiting-click
//   awaiting-click --mapClicked(target, pos)--> naming
//   naming --submitName(name)-----------------> done   (scaffoldArgs ready)
//   (any active) --cancel()-------------------> cancelled
//   --reset()---------------------------------> idle
//
// It holds no DOM and no framework primitives (no runes), so it is trivially
// testable and can be driven by any UI. Svelte consumers subscribe() and mirror
// `state` into a $state variable (see src/routes/kit/+page.svelte for the pattern).
// The no-map-click variant (trade goods 7.4) simply calls arm() then submitName()
// after feeding a synthetic target via mapClicked with a null position.

export type EntityFlowPhase =
  | "idle"
  | "awaiting-click"
  | "naming"
  | "done"
  | "cancelled";

export interface Point {
  x: number;
  y: number;
}

export interface EntityFlowState<TTarget, TArgs> {
  phase: EntityFlowPhase;
  /** The armed tool id (e.g. "create-country"), or null when idle/finished. */
  tool: string | null;
  /** What the map click resolved to (e.g. a province id). */
  target: TTarget | null;
  /** Screen position of the click, for anchoring the inline name prompt. */
  position: Point | null;
  /** The name entered in the prompt. */
  name: string | null;
  /** The scaffold arguments produced on completion (phase === "done"). */
  scaffoldArgs: TArgs | null;
}

export interface EntityFlowConfig<TTarget, TArgs> {
  /** Tool id this flow arms (surfaces in BottomToolbar). */
  tool: string;
  /** Default/prefill name for the inline prompt given the clicked target. */
  defaultName?: (target: TTarget) => string;
  /** Build the scaffold args handed to the backend once a name is confirmed. */
  buildArgs: (target: TTarget, name: string, position: Point | null) => TArgs;
  /** Called once the flow reaches `done`, with the built scaffold args. */
  onDone?: (args: TArgs, state: EntityFlowState<TTarget, TArgs>) => void;
  /** Called when the flow is cancelled from any active phase. */
  onCancel?: () => void;
}

export interface EntityFlow<TTarget, TArgs> {
  /** Current immutable state snapshot. */
  readonly state: EntityFlowState<TTarget, TArgs>;
  /** Arm the tool (idle → awaiting-click). No-op if not idle. */
  arm(): void;
  /** Register a map click (awaiting-click → naming). No-op in other phases. */
  mapClicked(target: TTarget, position?: Point | null): void;
  /** Confirm the name (naming → done). Returns the scaffold args, or null. */
  submitName(name: string): TArgs | null;
  /** Cancel from any active phase (→ cancelled). */
  cancel(): void;
  /** Return to idle, clearing all transient data. */
  reset(): void;
  /** Subscribe to state changes; returns an unsubscribe function. */
  subscribe(listener: (state: EntityFlowState<TTarget, TArgs>) => void): () => void;
}

export function createEntityFlow<TTarget, TArgs>(
  config: EntityFlowConfig<TTarget, TArgs>,
): EntityFlow<TTarget, TArgs> {
  const listeners = new Set<(s: EntityFlowState<TTarget, TArgs>) => void>();

  let state: EntityFlowState<TTarget, TArgs> = idle();

  function idle(): EntityFlowState<TTarget, TArgs> {
    return {
      phase: "idle",
      tool: null,
      target: null,
      position: null,
      name: null,
      scaffoldArgs: null,
    };
  }

  function emit() {
    for (const l of listeners) l(state);
  }

  function set(next: Partial<EntityFlowState<TTarget, TArgs>>) {
    state = { ...state, ...next };
    emit();
  }

  return {
    get state() {
      return state;
    },

    arm() {
      if (state.phase !== "idle") return;
      set({ phase: "awaiting-click", tool: config.tool });
    },

    mapClicked(target, position = null) {
      if (state.phase !== "awaiting-click") return;
      const name = config.defaultName ? config.defaultName(target) : "";
      set({ phase: "naming", target, position, name });
    },

    submitName(name) {
      if (state.phase !== "naming" || state.target === null) return null;
      const args = config.buildArgs(state.target, name, state.position);
      set({ phase: "done", name, scaffoldArgs: args });
      config.onDone?.(args, state);
      return args;
    },

    cancel() {
      if (state.phase === "idle" || state.phase === "done") return;
      set({ phase: "cancelled" });
      config.onCancel?.();
    },

    reset() {
      state = idle();
      emit();
    },

    subscribe(listener) {
      listeners.add(listener);
      listener(state);
      return () => listeners.delete(listener);
    },
  };
}
