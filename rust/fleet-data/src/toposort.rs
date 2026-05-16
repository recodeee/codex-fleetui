//! Kahn-style topological-wave assignment for plan subtasks.
//!
//! Extracted from the byte-identical copies that previously lived in
//! `fleet-plan-tree::waves()` and `fleet-waves::waves()`. Behaviour is
//! preserved verbatim: missing predecessors collapse to level 0, indices
//! inside each wave are sorted ascending, and cycles in `depends_on` are
//! NOT handled (the recursive resolver will overflow the stack on a cycle).
//! Callers must ensure their plan.json is acyclic — the producer side
//! (Colony plan publisher) already guarantees this.

use crate::plan::Subtask;
use std::collections::HashMap;

/// Assign each subtask to a wave such that every `depends_on` predecessor
/// sits in a strictly lower wave. Returns `out[level] = Vec<subtask_index>`
/// with indices sorted ascending inside each wave.
pub fn waves(subtasks: &[Subtask]) -> Vec<Vec<u32>> {
    let mut level: HashMap<u32, u32> = HashMap::new();
    let by_idx: HashMap<u32, &Subtask> =
        subtasks.iter().map(|s| (s.subtask_index, s)).collect();
    for s in subtasks {
        resolve(s.subtask_index, &by_idx, &mut level);
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

fn resolve(idx: u32, by: &HashMap<u32, &Subtask>, memo: &mut HashMap<u32, u32>) -> u32 {
    if let Some(&v) = memo.get(&idx) {
        return v;
    }
    let s = match by.get(&idx) {
        Some(s) => s,
        None => {
            memo.insert(idx, 0);
            return 0;
        }
    };
    let lvl = if s.depends_on.is_empty() {
        0
    } else {
        s.depends_on
            .iter()
            .map(|d| resolve(*d, by, memo))
            .max()
            .unwrap_or(0)
            + 1
    };
    memo.insert(idx, lvl);
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

    // NOTE: cycles in `depends_on` are NOT handled — the recursive resolver
    // overflows the stack. Adding an executable cycle test would crash the
    // test binary, so the invariant is documented at the module level
    // instead. Producers (Colony plan publisher) guarantee acyclic input.
}
