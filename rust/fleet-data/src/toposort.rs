//! Kahn-style topological-wave assignment for plan subtasks.
//!
//! Extracted from the byte-identical copies that previously lived in
//! `fleet-plan-tree::waves()` and `fleet-waves::waves()`. Behaviour is
//! preserved verbatim for acyclic input: missing predecessors collapse to
//! level 0 and indices inside each wave are sorted ascending.
//!
//! Cycles in `depends_on` (from hand-edited, partially-written, or
//! schema-bug plan.json files) are broken at the back-edge: when the
//! resolver re-enters a node it is already visiting, that edge contributes
//! level 0 instead of recursing. This avoids the stack overflow that
//! previously crashed `fleet-plan-tree` and `fleet-waves` on malformed
//! input. The producer side (Colony plan publisher) still guarantees
//! acyclic graphs in practice; this is dashboard-side defence in depth.

use crate::plan::Subtask;
use std::collections::{HashMap, HashSet};

/// Assign each subtask to a wave such that every `depends_on` predecessor
/// sits in a strictly lower wave. Returns `out[level] = Vec<subtask_index>`
/// with indices sorted ascending inside each wave.
pub fn waves(subtasks: &[Subtask]) -> Vec<Vec<u32>> {
    let mut level: HashMap<u32, u32> = HashMap::new();
    let by_idx: HashMap<u32, &Subtask> =
        subtasks.iter().map(|s| (s.subtask_index, s)).collect();
    for s in subtasks {
        let mut visiting: HashSet<u32> = HashSet::new();
        resolve(s.subtask_index, &by_idx, &mut level, &mut visiting);
    }
    let max = level.values().copied().max().unwrap_or(0);
    let mut out: Vec<Vec<u32>> = (0..=max).map(|_| Vec::new()).collect();
    let mut idxs: Vec<u32> = level.keys().copied().collect();
    idxs.sort();
    for i in idxs {
        out[level[&i] as usize].push(i);
    }
    out
}

fn resolve(
    idx: u32,
    by: &HashMap<u32, &Subtask>,
    memo: &mut HashMap<u32, u32>,
    visiting: &mut HashSet<u32>,
) -> u32 {
    if let Some(&v) = memo.get(&idx) {
        return v;
    }
    // Cycle guard: if we are already resolving this node further up the
    // recursion stack, treat the back-edge as contributing level 0 instead
    // of recursing into ourselves. The outer call will memoise a real
    // level once the rest of the dependencies resolve.
    if !visiting.insert(idx) {
        return 0;
    }
    let s = match by.get(&idx) {
        Some(s) => s,
        None => {
            memo.insert(idx, 0);
            visiting.remove(&idx);
            return 0;
        }
    };
    let lvl = if s.depends_on.is_empty() {
        0
    } else {
        s.depends_on
            .iter()
            .map(|d| resolve(*d, by, memo, visiting))
            .max()
            .unwrap_or(0)
            + 1
    };
    memo.insert(idx, lvl);
    visiting.remove(&idx);
    lvl
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::Subtask;

    fn st(idx: u32, deps: Vec<u32>) -> Subtask {
        Subtask {
            subtask_index: idx,
            title: format!("t{idx}"),
            description: String::new(),
            file_scope: Vec::new(),
            depends_on: deps,
            capability_hint: None,
            spec_row_id: None,
            status: "available".into(),
            claimed_by_session_id: None,
            claimed_by_agent: None,
            completed_summary: None,
        }
    }

    #[test]
    fn empty_graph_yields_single_empty_wave() {
        // Preserved historical behaviour: with no nodes, `max = 0` so the
        // output is a single empty wave (not an empty Vec).
        let out = waves(&[]);
        assert_eq!(out, vec![Vec::<u32>::new()]);
    }

    #[test]
    fn single_node_lands_in_wave_zero() {
        let out = waves(&[st(0, vec![])]);
        assert_eq!(out, vec![vec![0]]);
    }

    #[test]
    fn linear_chain_one_node_per_wave() {
        let plan = vec![st(0, vec![]), st(1, vec![0]), st(2, vec![1]), st(3, vec![2])];
        let out = waves(&plan);
        assert_eq!(out, vec![vec![0], vec![1], vec![2], vec![3]]);
    }

    #[test]
    fn two_parallel_chains_share_waves() {
        // 0 -> 1 -> 2
        // 10 -> 11 -> 12
        let plan = vec![
            st(0, vec![]),
            st(1, vec![0]),
            st(2, vec![1]),
            st(10, vec![]),
            st(11, vec![10]),
            st(12, vec![11]),
        ];
        let out = waves(&plan);
        assert_eq!(out, vec![vec![0, 10], vec![1, 11], vec![2, 12]]);
    }

    #[test]
    fn node_with_multiple_deps_takes_max_plus_one() {
        // 0, 1 are roots; 2 depends on a long chain via 1; 3 depends on both.
        let plan = vec![
            st(0, vec![]),
            st(1, vec![]),
            st(2, vec![1]),
            st(3, vec![0, 2]),
        ];
        let out = waves(&plan);
        assert_eq!(out, vec![vec![0, 1], vec![2], vec![3]]);
    }

    #[test]
    fn missing_dependency_treated_as_level_zero() {
        // Subtask 1 depends on a non-existent 99 — preserved behaviour
        // memoises the unknown index at level 0, so it is emitted in wave
        // 0 alongside any real roots and 1 lands at level 1.
        let plan = vec![st(1, vec![99])];
        let out = waves(&plan);
        assert_eq!(out, vec![vec![99], vec![1]]);
    }

    #[test]
    fn cycle_broken_to_level_zero() {
        // A -> B -> A cycle. Before the fix this overflowed the stack and
        // crashed fleet-plan-tree / fleet-waves. After the fix the
        // back-edge is broken so both nodes resolve to a sane finite
        // level instead of recursing forever. Exact levels depend on
        // which node the outer loop visits first (one node will see the
        // broken back-edge contribute 0 and land in wave 1; the other
        // sees that as its dep and lands in wave 2). The important
        // invariants are: (a) no panic / overflow, (b) both nodes
        // present, (c) wave count stays small and bounded by the node
        // count.
        let plan = vec![st(0, vec![1]), st(1, vec![0])];
        let out = waves(&plan);
        let flat: Vec<u32> = out.iter().flatten().copied().collect();
        assert!(flat.contains(&0), "node 0 missing from waves: {out:?}");
        assert!(flat.contains(&1), "node 1 missing from waves: {out:?}");
        assert!(
            out.len() <= plan.len() + 1,
            "cycle produced {} waves for {} nodes: {out:?}",
            out.len(),
            plan.len()
        );
    }

    #[test]
    fn self_cycle_broken() {
        // A -> A self-loop. The visiting guard fires on re-entry so the
        // depends_on iteration sees a 0 from the broken back-edge and
        // the node lands in wave 1 (max(0) + 1) without recursing
        // forever. The critical assertion is no overflow + node 0 is
        // still emitted exactly once.
        let plan = vec![st(0, vec![0])];
        let out = waves(&plan);
        let flat: Vec<u32> = out.iter().flatten().copied().collect();
        assert_eq!(flat, vec![0], "self-cycle should still emit node 0 exactly once: {out:?}");
        assert!(out.len() <= 2, "self-cycle produced {} waves: {out:?}", out.len());
    }
}
