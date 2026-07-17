// Shared prop/callback types for the Trade Node panel + route editor (Sprint 8).

import type { RouteRef } from "$lib/tradenet";

export type { RouteRef };

/** Actions the node panel/route editor ask MapView to perform (map-side or
 *  selection changes). Structural file edits are pushed to the queue directly. */
export interface NodePanelActions {
  onclose: () => void;
  /** Select another node (incoming jump, validation jump, delete cleanup). */
  onselectnode: (key: string) => void;
  /** Select a route for the on-map editor (Edit button), or clear with null. */
  onselectroute: (ref: RouteRef | null) => void;
  /** Arm the Set Location tool (click a member province). */
  onsetlocation: () => void;
  /** Arm the Add Route tool (click the target node's marker). */
  onaddroute: () => void;
  /** The selected node was deleted; MapView should clear selection. */
  ondeleted: () => void;
}
