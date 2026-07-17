// S2.8 — pending-aware trade-graph validation (client-side).
//
// Pure, framework-free graph checks over the EFFECTIVE trade network (the fold
// of base + the typed edit queue produced by `tradenet.ts` `foldNetwork`). The
// backend `validation.rs` `trade_nodes` domain runs the same checks over the
// SAVED files; porting the graph-derived ones here lets the validation strip
// flag cycles / unreachable ends / orphans *while editing* — the moment a route
// reshape or node create/delete is queued — instead of only after save. Undo is
// automatic: MapView recomputes these from `foldNetwork(base, queue.serialize())`
// on `queue.version`, so reverting the offending edit re-clears the flag.
//
// Every check here is derivable from the folded `TradeNetwork` shape alone (the
// frontend's complete mirror of the backend `node_graph`), so the trade_nodes
// domain has NO non-graph checks left on the backend to keep separate: the strip
// is simply these results on the effective network. With an empty queue the
// effective network equals the saved network, so the output matches the backend
// domain (modulo localisation of node names, which the fold already carries in
// `node.name`).
//
// Messages/severities/jump targets mirror `validation.rs::check_trade_nodes` so
// the strip reads identically whether an issue is pending-derived or on disk.

import type { TradeNetwork, TradeNode } from "$lib/tradenet";
import type { ValidationIssue } from "$lib/components/ValidationStrip.svelte";

/** Display name for a node key, resolving through the network (falls back to the
 *  key when unknown — matches the backend's `loc.resolve`, which returns the key
 *  for un-localised entries and for freshly-created nodes). */
function resolveName(network: TradeNetwork, key: string): string {
  const n = network.nodes.find((x) => x.key === key);
  return n?.name || key;
}

/**
 * First steering cycle (back-edge) along outgoing routes, as `{ from, to }`
 * node keys, or null when the graph is a DAG. Iterative DFS with white/gray/
 * black colouring — identical to the backend's cycle pass, deep-chain safe.
 * Exported for direct logic testing.
 */
export function findCycle(network: TradeNetwork): { from: string; to: string } | null {
  const nodes = network.nodes;
  const index = new Map<string, number>();
  nodes.forEach((n, i) => index.set(n.key, i));

  const enum_white = 0,
    enum_gray = 1,
    enum_black = 2;
  const color = new Array<number>(nodes.length).fill(enum_white);

  for (let start = 0; start < nodes.length; start++) {
    if (color[start] !== enum_white) continue;
    // stack of (nodeIndex, nextEdgeCursor)
    const stack: Array<[number, number]> = [[start, 0]];
    color[start] = enum_gray;
    while (stack.length > 0) {
      const top = stack[stack.length - 1];
      const [node, edge] = top;
      const routes = nodes[node].outgoing;
      if (edge < routes.length) {
        top[1] = edge + 1;
        const target = routes[edge].target;
        const t = index.get(target);
        if (t === undefined) continue;
        if (color[t] === enum_white) {
          color[t] = enum_gray;
          stack.push([t, 0]);
        } else if (color[t] === enum_gray) {
          // Back-edge to a node on the current DFS path: a cycle.
          return { from: nodes[node].key, to: target };
        }
      } else {
        color[node] = enum_black;
        stack.pop();
      }
    }
  }
  return null;
}

/**
 * Keys of the non-`end` nodes that cannot reach any `end` node by following
 * outgoing routes (a sink with no drain / an isolated loop). Memoised DFS that
 * treats a node revisited on the current path as no-end-this-way, exactly like
 * the backend `reaches_end`. Exported for direct logic testing.
 */
export function unreachableToEnd(network: TradeNetwork): string[] {
  const nodes = network.nodes;
  const index = new Map<string, number>();
  nodes.forEach((n, i) => index.set(n.key, i));

  // 0 = unknown, 1 = reaches end, 2 = does not.
  const reach = new Array<number>(nodes.length).fill(0);

  function reachesEnd(i: number, visiting: Set<number>): boolean {
    if (nodes[i].end) return true;
    if (reach[i] !== 0) return reach[i] === 1;
    if (visiting.has(i)) return false; // on the current DFS path → no end this way
    visiting.add(i);
    let ok = false;
    for (const r of nodes[i].outgoing) {
      const t = index.get(r.target);
      if (t !== undefined && reachesEnd(t, visiting)) {
        ok = true;
        break;
      }
    }
    visiting.delete(i);
    reach[i] = ok ? 1 : 2;
    return ok;
  }

  const out: string[] = [];
  for (let i = 0; i < nodes.length; i++) {
    if (!nodes[i].end && !reachesEnd(i, new Set<number>())) out.push(nodes[i].key);
  }
  return out;
}

function node(severity: ValidationIssue["severity"], message: string, key: string): ValidationIssue {
  return { severity, message, jump: { kind: "node", id: key } };
}

/**
 * Full trade-graph report over the effective (folded) network — the client-side
 * mirror of `validation.rs::check_trade_nodes`. Issue order and text match the
 * backend (the strip re-sorts by severity, so exact order is cosmetic):
 *   per node, in file order — zero members (warn), location-not-a-member (err),
 *   non-end dead-end (err), end-with-outgoing (warn), then per route: unknown
 *   target (err) / empty path (warn); then one cycle (err) and any nodes that
 *   cannot reach an end (err).
 */
export function validateTradeGraph(network: TradeNetwork): ValidationIssue[] {
  const issues: ValidationIssue[] = [];
  const keys = new Set(network.nodes.map((n) => n.key));
  const nm = (n: TradeNode) => n.name || n.key;

  for (const n of network.nodes) {
    // Zero members (warning).
    if (n.members.length === 0) {
      issues.push(node("warning", `Trade node "${nm(n)}" has no members`, n.key));
    }
    // location must be a member of its own node (error).
    if (n.location != null && !n.members.includes(n.location)) {
      issues.push(
        node(
          "error",
          `Trade node "${nm(n)}" collection province ${n.location} is not one of its members`,
          n.key,
        ),
      );
    }
    // Non-end node needs at least one outgoing route (error).
    if (!n.end && n.outgoing.length === 0) {
      issues.push(
        node("error", `Trade node "${nm(n)}" is not an end node but has no outgoing routes`, n.key),
      );
    }
    // End node with outgoing routes (warning).
    if (n.end && n.outgoing.length > 0) {
      issues.push(
        node("warning", `End trade node "${nm(n)}" has ${n.outgoing.length} outgoing route(s)`, n.key),
      );
    }
    for (const r of n.outgoing) {
      // Route endpoint must exist (error).
      if (!keys.has(r.target)) {
        issues.push(node("error", `Trade node "${nm(n)}" has a route to unknown node "${r.target}"`, n.key));
        continue;
      }
      // Empty path (warning) — the only reliable "disconnected" signal (route-
      // corridor seas legitimately belong to no node, so path membership can't
      // be relied on; matches the backend's reasoning).
      if (r.path.length === 0) {
        issues.push(
          node("warning", `Route "${nm(n)}" → "${resolveName(network, r.target)}" has an empty path`, n.key),
        );
      }
    }
  }

  // Cycle detection (error, reported once).
  const cyc = findCycle(network);
  if (cyc) {
    issues.push(
      node(
        "error",
        `Trade route cycle detected involving "${resolveName(network, cyc.from)}" → "${resolveName(network, cyc.to)}"`,
        cyc.from,
      ),
    );
  }

  // Reachability: every non-end node must reach an end node (error).
  for (const key of unreachableToEnd(network)) {
    issues.push(node("error", `Trade node "${resolveName(network, key)}" cannot reach any end node`, key));
  }

  return issues;
}
