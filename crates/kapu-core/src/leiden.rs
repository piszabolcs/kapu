//! Leiden community detection (Traag, Waltman & van Eck 2019), FR-12 and D43.
//!
//! Modularity with resolution `γ = 1`. One iteration: fast local moving,
//! refinement inside each community, aggregation of the refined partition
//! (the unrefined partition is the starting point on the aggregate graph).
//! Iterations stop when local moving changes nothing, or after 10.
//!
//! Determinism (NFR-02): the only randomness is the order in which nodes are
//! first visited, from a xoshiro256** generator seeded with 42. The
//! refinement takes the best non-negative move (the θ → 0 limit of the paper,
//! D92), so it needs no random choice. Ties go to the current community, then
//! to the smallest community index. Sums run in a fixed order in f64.

use std::collections::{BTreeMap, VecDeque};

/// Seed of the node visiting order (FR-12/1).
pub const SEED: u64 = 42;
/// At most this many aggregation levels per run (D43).
pub const MAX_ITERATIONS: usize = 10;
/// At most this many full optimisation runs, each from the previous partition (D92, D110).
pub const OUTER_RUNS: usize = 10;
/// Independent optimisations with seeds 42, 43, ...; the best modularity wins (D110).
pub const RESTARTS: u64 = 8;
const GAMMA: f64 = 1.0;

/// xoshiro256** seeded through splitmix64; platform independent.
struct Rng {
    s: [u64; 4],
}

impl Rng {
    fn new(seed: u64) -> Self {
        let mut x = seed;
        let mut next = move || {
            x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        Self {
            s: [next(), next(), next(), next()],
        }
    }

    fn next_u64(&mut self) -> u64 {
        let [s0, s1, s2, s3] = &mut self.s;
        let result = s1.wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = *s1 << 17;
        *s2 ^= *s0;
        *s3 ^= *s1;
        *s1 ^= *s2;
        *s0 ^= *s3;
        *s2 ^= t;
        *s3 = s3.rotate_left(45);
        result
    }

    /// Fisher–Yates shuffle.
    fn shuffle(&mut self, v: &mut [usize]) {
        for i in (1..v.len()).rev() {
            let j = usize::try_from(self.next_u64() % (i as u64 + 1)).unwrap_or(0);
            v.swap(i, j);
        }
    }
}

/// An undirected weighted graph with adjacency lists; a self-loop of weight
/// `w` contributes `2w` to the node's degree.
#[derive(Debug, Clone)]
struct Graph {
    adj: Vec<Vec<(usize, f64)>>,
    self_loops: Vec<f64>,
    degree: Vec<f64>,
    /// Sum of all degrees (= 2m).
    total: f64,
}

impl Graph {
    fn new(n: usize, edges: &BTreeMap<(usize, usize), f64>) -> Self {
        let mut adj = vec![Vec::new(); n];
        let mut self_loops = vec![0.0; n];
        for (&(a, b), &w) in edges {
            if a == b {
                if let Some(s) = self_loops.get_mut(a) {
                    *s += w;
                }
            } else {
                if let Some(l) = adj.get_mut(a) {
                    l.push((b, w));
                }
                if let Some(l) = adj.get_mut(b) {
                    l.push((a, w));
                }
            }
        }
        let degree: Vec<f64> = adj
            .iter()
            .zip(&self_loops)
            .map(|(l, s)| l.iter().map(|(_, w)| w).sum::<f64>() + 2.0 * s)
            .collect();
        let total = degree.iter().sum();
        Self {
            adj,
            self_loops,
            degree,
            total,
        }
    }

    fn len(&self) -> usize {
        self.adj.len()
    }

    fn deg(&self, v: usize) -> f64 {
        self.degree.get(v).copied().unwrap_or(0.0)
    }

    fn neighbours(&self, v: usize) -> &[(usize, f64)] {
        self.adj.get(v).map_or(&[], Vec::as_slice)
    }
}

/// Edge weight from `v` to each community, in community order.
fn links_to_communities(g: &Graph, v: usize, community: &[usize]) -> BTreeMap<usize, f64> {
    let mut out: BTreeMap<usize, f64> = BTreeMap::new();
    for &(u, w) in g.neighbours(v) {
        if let Some(&c) = community.get(u) {
            *out.entry(c).or_insert(0.0) += w;
        }
    }
    out
}

/// Phase 1: queue-based local moving. Returns whether any node moved.
fn local_moving(g: &Graph, community: &mut [usize], rng: &mut Rng) -> bool {
    let n = g.len();
    let mut totals = vec![0.0f64; n];
    let mut sizes = vec![0usize; n];
    for (v, &c) in community.iter().enumerate() {
        if let Some(t) = totals.get_mut(c) {
            *t += g.deg(v);
        }
        if let Some(s) = sizes.get_mut(c) {
            *s += 1;
        }
    }
    // empty community labels, smallest on top
    let mut empty: Vec<usize> = (0..n).rev().filter(|&c| sizes.get(c) == Some(&0)).collect();
    let mut order: Vec<usize> = (0..n).collect();
    rng.shuffle(&mut order);
    let mut queue: VecDeque<usize> = order.into_iter().collect();
    let mut queued = vec![true; n];
    let mut moved = false;
    while let Some(v) = queue.pop_front() {
        if let Some(q) = queued.get_mut(v) {
            *q = false;
        }
        let Some(&current) = community.get(v) else {
            continue;
        };
        let kv = g.deg(v);
        if let Some(t) = totals.get_mut(current) {
            *t -= kv;
        }
        if let Some(s) = sizes.get_mut(current) {
            *s -= 1;
        }
        let links = links_to_communities(g, v, community);
        let gain = |c: usize| {
            links.get(&c).copied().unwrap_or(0.0)
                - GAMMA * kv * totals.get(c).copied().unwrap_or(0.0) / g.total
        };
        let mut best = current;
        let mut best_gain = gain(current);
        for &c in links.keys() {
            let gc = gain(c);
            if gc > best_gain {
                best = c;
                best_gain = gc;
            }
        }
        // alone is worth 0: if every option is negative, take an empty community
        if best_gain < 0.0
            && let Some(&e) = empty.last()
        {
            best = e;
        }
        if let Some(t) = totals.get_mut(best) {
            *t += kv;
        }
        if let Some(s) = sizes.get_mut(best) {
            *s += 1;
        }
        if empty.last() == Some(&best) {
            empty.pop();
        }
        if sizes.get(current) == Some(&0) && current != best {
            empty.push(current);
        }
        if best != current {
            if let Some(c) = community.get_mut(v) {
                *c = best;
            }
            moved = true;
            for &(u, _) in g.neighbours(v) {
                if community.get(u) != Some(&best) && queued.get(u) == Some(&false) {
                    if let Some(q) = queued.get_mut(u) {
                        *q = true;
                    }
                    queue.push_back(u);
                }
            }
        }
    }
    moved
}

/// Phase 2: refinement. Every node starts alone; inside each community of
/// `community`, a node that is still a singleton and well connected to its
/// community joins the refined sub-community with the best non-negative
/// gain among the well-connected ones (D92). Nodes are taken in index order.
fn refine(g: &Graph, community: &[usize]) -> Vec<usize> {
    let n = g.len();
    let mut refined: Vec<usize> = (0..n).collect();
    let mut members: Vec<Vec<usize>> = (0..n).map(|v| vec![v]).collect();
    let mut ref_total: Vec<f64> = (0..n).map(|v| g.deg(v)).collect();
    let mut comm_total: BTreeMap<usize, f64> = BTreeMap::new();
    for (v, &c) in community.iter().enumerate() {
        *comm_total.entry(c).or_insert(0.0) += g.deg(v);
    }
    // weight from each refined sub-community to the rest of its community
    let mut external: Vec<f64> = (0..n)
        .map(|v| {
            let c = community.get(v);
            g.neighbours(v)
                .iter()
                .filter(|(u, _)| community.get(*u) == c && *u != v)
                .map(|(_, w)| w)
                .sum()
        })
        .collect();
    let mut singleton = vec![true; n];
    for v in 0..n {
        let Some(&c) = community.get(v) else { continue };
        let kc = comm_total.get(&c).copied().unwrap_or(0.0);
        let kv = g.deg(v);
        let own = refined.get(v).copied().unwrap_or(v);
        if !singleton.get(v).copied().unwrap_or(false)
            || external.get(own).copied().unwrap_or(0.0) < GAMMA * kv * (kc - kv) / g.total
        {
            continue;
        }
        let mut links: BTreeMap<usize, f64> = BTreeMap::new();
        for &(u, w) in g.neighbours(v) {
            if community.get(u) == Some(&c)
                && let Some(&r) = refined.get(u).filter(|&&r| r != own)
            {
                *links.entry(r).or_insert(0.0) += w;
            }
        }
        let mut best = own;
        let mut best_gain = 0.0f64;
        for (&r, &w) in &links {
            let kr = ref_total.get(r).copied().unwrap_or(0.0);
            if external.get(r).copied().unwrap_or(0.0) < GAMMA * kr * (kc - kr) / g.total {
                continue;
            }
            let gain = w - GAMMA * kv * kr / g.total;
            if gain > best_gain {
                best = r;
                best_gain = gain;
            }
        }
        if best == own {
            continue;
        }
        let w_to_best = links.get(&best).copied().unwrap_or(0.0);
        let ext_v = external.get(own).copied().unwrap_or(0.0);
        if let Some(e) = external.get_mut(best) {
            *e += ext_v - 2.0 * w_to_best;
        }
        if let Some(t) = ref_total.get_mut(best) {
            *t += kv;
        }
        if let Some(r) = refined.get_mut(v) {
            *r = best;
        }
        let moved_members = members.get_mut(own).map(std::mem::take).unwrap_or_default();
        if let Some(list) = members.get_mut(best) {
            list.extend(moved_members);
            for &u in list.iter() {
                if let Some(s) = singleton.get_mut(u) {
                    *s = false;
                }
            }
        }
    }
    refined
}

// Renumbers labels to 0.. in order of first appearance.
fn renumber(labels: &[usize]) -> Vec<usize> {
    let mut map: BTreeMap<usize, usize> = BTreeMap::new();
    let mut next = 0;
    labels
        .iter()
        .map(|&l| {
            *map.entry(l).or_insert_with(|| {
                next += 1;
                next - 1
            })
        })
        .collect()
}

/// Communities of a graph with `n` nodes. `edges` are undirected; parallel
/// edges must already be summed, keys with `a <= b`. Returns a community
/// index per node, numbered 0.. in order of first appearance.
///
/// [`RESTARTS`] independent optimisations run with the seeds 42, 43, ...;
/// in each, the whole multi-level optimisation repeats from the previous
/// partition until it no longer changes (at most [`OUTER_RUNS`] times). The
/// partition with the highest modularity wins; on a tie the earlier seed
/// (D110). Everything is deterministic.
#[must_use]
pub fn leiden(n: usize, edges: &BTreeMap<(usize, usize), f64>) -> Vec<usize> {
    let mut best: Option<(f64, Vec<usize>)> = None;
    for restart in 0..RESTARTS {
        let mut rng = Rng::new(SEED.wrapping_add(restart));
        let mut partition: Vec<usize> = (0..n).collect();
        for _ in 0..OUTER_RUNS {
            let next = leiden_run(n, edges, partition.clone(), &mut rng);
            if next == partition {
                break;
            }
            partition = next;
        }
        let partition = renumber(&partition);
        let q = modularity(n, edges, &partition);
        if best.as_ref().is_none_or(|(best_q, _)| q > *best_q) {
            best = Some((q, partition));
        }
    }
    best.map_or_else(|| (0..n).collect(), |(_, p)| p)
}

/// One multi-level Leiden optimisation starting from `initial`.
fn leiden_run(
    n: usize,
    edges: &BTreeMap<(usize, usize), f64>,
    initial: Vec<usize>,
    rng: &mut Rng,
) -> Vec<usize> {
    let mut graph = Graph::new(n, edges);
    // node of the original graph -> node of the current aggregate graph
    let mut node_of: Vec<usize> = (0..n).collect();
    let mut community: Vec<usize> = initial;
    for _ in 0..MAX_ITERATIONS {
        if graph.total == 0.0 {
            break;
        }
        let moved = local_moving(&graph, &mut community, rng);
        let k = renumber(&community)
            .iter()
            .copied()
            .max()
            .map_or(0, |m| m + 1);
        if !moved || k == graph.len() {
            break;
        }
        let refined = renumber(&refine(&graph, &community));
        let m = refined.iter().copied().max().map_or(0, |x| x + 1);
        // aggregate graph on the refined partition
        let mut agg: BTreeMap<(usize, usize), f64> = BTreeMap::new();
        for v in 0..graph.len() {
            let rv = refined.get(v).copied().unwrap_or(0);
            if let Some(s) = graph.self_loops.get(v).filter(|s| **s != 0.0) {
                *agg.entry((rv, rv)).or_insert(0.0) += s;
            }
            for &(u, w) in graph.neighbours(v) {
                let ru = refined.get(u).copied().unwrap_or(0);
                if v < u {
                    let key = if rv <= ru { (rv, ru) } else { (ru, rv) };
                    *agg.entry(key).or_insert(0.0) += w;
                }
            }
        }
        // the unrefined partition, lifted to the aggregate nodes
        let mut agg_community = vec![0usize; m];
        for v in 0..graph.len() {
            if let (Some(&r), Some(&c)) = (refined.get(v), community.get(v))
                && let Some(slot) = agg_community.get_mut(r)
            {
                *slot = c;
            }
        }
        for slot in &mut node_of {
            *slot = refined.get(*slot).copied().unwrap_or(0);
        }
        graph = Graph::new(m, &agg);
        community = renumber(&agg_community);
    }
    renumber(
        &node_of
            .iter()
            .map(|&a| community.get(a).copied().unwrap_or(0))
            .collect::<Vec<_>>(),
    )
}

/// Modularity (γ = 1) of a partition, for measurement (M4/M8).
#[must_use]
pub fn modularity(n: usize, edges: &BTreeMap<(usize, usize), f64>, community: &[usize]) -> f64 {
    let g = Graph::new(n, edges);
    if g.total == 0.0 {
        return 0.0;
    }
    let mut inner: BTreeMap<usize, f64> = BTreeMap::new();
    let mut totals: BTreeMap<usize, f64> = BTreeMap::new();
    for v in 0..n {
        let c = community.get(v).copied().unwrap_or(v);
        *totals.entry(c).or_insert(0.0) += g.deg(v);
        *inner.entry(c).or_insert(0.0) += 2.0 * g.self_loops.get(v).copied().unwrap_or(0.0);
        for &(u, w) in g.neighbours(v) {
            if community.get(u).copied().unwrap_or(u) == c {
                *inner.entry(c).or_insert(0.0) += w;
            }
        }
    }
    totals
        .iter()
        .map(|(c, k)| inner.get(c).copied().unwrap_or(0.0) / g.total - (k / g.total).powi(2))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edges(list: &[(usize, usize, f64)]) -> BTreeMap<(usize, usize), f64> {
        list.iter()
            .map(|&(a, b, w)| ((a.min(b), a.max(b)), w))
            .collect()
    }

    #[test]
    fn two_cliques_joined_by_one_edge() {
        let mut e = Vec::new();
        for a in 0..4 {
            for b in a + 1..4 {
                e.push((a, b, 1.0));
                e.push((a + 4, b + 4, 1.0));
            }
        }
        e.push((3, 4, 1.0));
        let g = edges(&e);
        let c = leiden(8, &g);
        assert_eq!(c[..4], [c[0]; 4]);
        assert_eq!(c[4..], [c[4]; 4]);
        assert_ne!(c[0], c[4]);
        assert!(modularity(8, &g, &c) > 0.4);
    }

    #[test]
    fn isolated_nodes_stay_alone_and_empty_graph_is_trivial() {
        let g = edges(&[(0, 1, 1.0)]);
        let c = leiden(3, &g);
        assert_eq!(c[0], c[1]);
        assert_ne!(c[0], c[2]);
        assert_eq!(leiden(3, &BTreeMap::new()), [0, 1, 2]);
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn deterministic() {
        let mut e = Vec::new();
        for i in 0..60usize {
            e.push((i, (i * 7 + 3) % 60, 1.0 + (i % 3) as f64));
            e.push((i, (i + 1) % 60, 0.5));
        }
        let g = edges(&e);
        assert_eq!(leiden(60, &g), leiden(60, &g));
    }

    #[test]
    fn karate_club_matches_leidenalg_within_d110_tolerance() {
        // Zachary's karate club; Python leidenalg 0.12 (seed 42) reaches 0.4198
        let list: [(usize, usize); 78] = [
            (0, 1),
            (0, 2),
            (0, 3),
            (0, 4),
            (0, 5),
            (0, 6),
            (0, 7),
            (0, 8),
            (0, 10),
            (0, 11),
            (0, 12),
            (0, 13),
            (0, 17),
            (0, 19),
            (0, 21),
            (0, 31),
            (1, 2),
            (1, 3),
            (1, 7),
            (1, 13),
            (1, 17),
            (1, 19),
            (1, 21),
            (1, 30),
            (2, 3),
            (2, 7),
            (2, 27),
            (2, 28),
            (2, 32),
            (2, 9),
            (2, 8),
            (2, 13),
            (3, 7),
            (3, 12),
            (3, 13),
            (4, 6),
            (4, 10),
            (5, 6),
            (5, 10),
            (5, 16),
            (6, 16),
            (8, 30),
            (8, 32),
            (8, 33),
            (9, 33),
            (13, 33),
            (14, 32),
            (14, 33),
            (15, 32),
            (15, 33),
            (18, 32),
            (18, 33),
            (19, 33),
            (20, 32),
            (20, 33),
            (22, 32),
            (22, 33),
            (23, 25),
            (23, 27),
            (23, 32),
            (23, 33),
            (23, 29),
            (24, 25),
            (24, 27),
            (24, 31),
            (25, 31),
            (26, 29),
            (26, 33),
            (27, 33),
            (28, 31),
            (28, 33),
            (29, 32),
            (29, 33),
            (30, 32),
            (30, 33),
            (31, 32),
            (31, 33),
            (32, 33),
        ];
        let g = edges(&list.iter().map(|&(a, b)| (a, b, 1.0)).collect::<Vec<_>>());
        let c = leiden(34, &g);
        let q = modularity(34, &g, &c);
        // D110: never more than 0.005 below leidenalg
        assert!(q >= 0.4198 - 0.005, "modularity {q}");
    }
}
