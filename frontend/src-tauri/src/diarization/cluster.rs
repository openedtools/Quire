//! Average-linkage (UPGMA) agglomerative clustering of voice embeddings, plus an estimate of
//! how many speakers there are.
//!
//! Embeddings are L2-normalised, so cosine distance is `1 - dot`. The merge order and heights
//! are the same as SciPy's `linkage(method="average", metric="cosine")`, which the tests check.

/// One merge in the tree. Ids follow SciPy: `0..n` are the input items, `n + i` is merge `i`.
#[derive(Debug, Clone, PartialEq)]
pub struct Merge {
    pub a: usize,
    pub b: usize,
    pub distance: f32,
    pub size: usize,
}

#[derive(Debug, Clone)]
pub struct Dendrogram {
    n: usize,
    merges: Vec<Merge>,
}

/// Cosine distance matrix (row-major `n x n`) for L2-normalised embeddings.
pub fn cosine_distances(embeddings: &[Vec<f32>]) -> Vec<f32> {
    let n = embeddings.len();
    let mut dist = vec![0.0f32; n * n];
    for i in 0..n {
        for j in (i + 1)..n {
            let dot: f32 = embeddings[i].iter().zip(&embeddings[j]).map(|(a, b)| a * b).sum();
            // f32::max ignores NaN, which would turn an unusable embedding into "identical to everything"
            let d = if dot.is_finite() { (1.0 - dot).max(0.0) } else { 2.0 };
            dist[i * n + j] = d;
            dist[j * n + i] = d;
        }
    }
    dist
}

/// Cosine similarity of two L2-normalised embeddings.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Scale a vector to unit length (left unchanged if it is all zeros).
pub fn normalized(mut v: Vec<f32>) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.iter_mut().for_each(|x| *x /= norm);
    }
    v
}

/// Cluster `n` items from their pairwise distances (row-major, symmetric).
///
/// Cost is O(n^3) in the worst case, fine for the few hundred speech segments this is used on.
pub fn average_linkage(distances: &[f32], n: usize) -> Dendrogram {
    assert_eq!(distances.len(), n * n, "distance matrix must be n x n");
    let mut d: Vec<f64> = distances.iter().map(|&x| x as f64).collect();
    let mut ids: Vec<usize> = (0..n).collect();
    let mut sizes = vec![1usize; n];
    let mut active = vec![true; n];
    let mut merges = Vec::with_capacity(n.saturating_sub(1));

    for step in 0..n.saturating_sub(1) {
        // Closest pair of live clusters (ties resolved by lowest indices, so results are repeatable)
        let (mut bi, mut bj, mut best) = (0, 0, f64::INFINITY);
        for i in 0..n {
            if !active[i] {
                continue;
            }
            for j in (i + 1)..n {
                if active[j] && d[i * n + j] < best {
                    (bi, bj, best) = (i, j, d[i * n + j]);
                }
            }
        }

        merges.push(Merge { a: ids[bi], b: ids[bj], distance: best as f32, size: sizes[bi] + sizes[bj] });

        // Lance-Williams update for average linkage: size-weighted mean of the two old distances
        let (si, sj) = (sizes[bi] as f64, sizes[bj] as f64);
        for k in 0..n {
            if active[k] && k != bi && k != bj {
                let merged = (si * d[bi * n + k] + sj * d[bj * n + k]) / (si + sj);
                d[bi * n + k] = merged;
                d[k * n + bi] = merged;
            }
        }
        active[bj] = false;
        sizes[bi] += sizes[bj];
        ids[bi] = n + step;
    }
    Dendrogram { n, merges }
}

impl Dendrogram {
    pub fn len(&self) -> usize {
        self.n
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    /// Merge distances, ascending (average linkage never goes backwards).
    pub fn heights(&self) -> Vec<f32> {
        self.merges.iter().map(|m| m.distance).collect()
    }

    pub fn merges(&self) -> &[Merge] {
        &self.merges
    }

    /// Cut the tree into `k` clusters. Labels are `0..k`, numbered by first appearance.
    pub fn labels_for_k(&self, k: usize) -> Vec<usize> {
        let n = self.n;
        if n == 0 {
            return Vec::new();
        }
        let k = k.clamp(1, n);

        // members[id] = leaves under cluster `id`, for the first n-k merges
        let mut members: Vec<Vec<usize>> = (0..n).map(|i| vec![i]).collect();
        let mut leaf_cluster: Vec<usize> = (0..n).collect();
        for (step, merge) in self.merges.iter().take(n - k).enumerate() {
            let mut joined = std::mem::take(&mut members[merge.a]);
            joined.extend(std::mem::take(&mut members[merge.b]));
            for &leaf in &joined {
                leaf_cluster[leaf] = n + step;
            }
            members.push(joined);
        }

        let mut next = 0;
        let mut relabel = std::collections::HashMap::new();
        leaf_cluster
            .into_iter()
            .map(|c| *relabel.entry(c).or_insert_with(|| { next += 1; next - 1 }))
            .collect()
    }
}

/// Below this top-of-tree distance everything sounds like one voice (cosine similarity > 0.55).
/// Measured: single-speaker recordings top out at about 0.3, real multi-speaker ones start at 0.65.
pub const SINGLE_SPEAKER_TOP_DISTANCE: f32 = 0.45;

/// Guess how many speakers there are from the shape of the tree: cut where the merge distance
/// jumps the most. If even the final merge is small, everything is one voice.
///
/// Under-counting is the usual failure (two similar voices merged), which is why the UI also
/// lets people say how many speakers there were.
pub fn estimate_num_speakers(tree: &Dendrogram, max_speakers: usize) -> usize {
    let n = tree.len();
    let h = tree.heights();
    if n < 3 {
        // Too little to see a gap: two items are two speakers unless they sound alike
        return match n {
            2 if h[0] >= SINGLE_SPEAKER_TOP_DISTANCE => 2,
            _ => 1,
        };
    }
    if h[n - 2] < SINGLE_SPEAKER_TOP_DISTANCE {
        return 1;
    }
    let (mut best_k, mut best_gap) = (1, 0.0f32);
    for k in 2..=max_speakers.min(n) {
        // cutting into k clusters sits between merge n-k-1 and merge n-k
        let gap = h[n - k] - if n >= k + 1 { h[n - k - 1] } else { 0.0 };
        if gap > best_gap {
            (best_k, best_gap) = (k, gap);
        }
    }
    best_k
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic PRNG so tests need no extra dependency.
    struct Rng(u64);
    impl Rng {
        fn next_f32(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            ((self.0 >> 11) as f64 / (1u64 << 53) as f64) as f32 * 2.0 - 1.0
        }
    }

    /// `per` noisy unit vectors around each of `k` prototypes (spread controls overlap).
    fn blobs(k: usize, per: usize, spread: f32, seed: u64) -> (Vec<Vec<f32>>, Vec<usize>) {
        let mut rng = Rng(seed);
        let dim = 16;
        // Prototypes along separate axes, so the groups are clearly distinct by construction
        let protos: Vec<Vec<f32>> = (0..k).map(|v| (0..dim).map(|d| if d == v { 1.0 } else { 0.0 }).collect()).collect();
        let mut items = Vec::new();
        let mut truth = Vec::new();
        for i in 0..per * k {
            let c = i % k;
            items.push(normalized(protos[c].iter().map(|x| x + spread * rng.next_f32()).collect()));
            truth.push(c);
        }
        (items, truth)
    }

    /// Do two labelings describe the same partition (ignoring label names)?
    fn same_partition(a: &[usize], b: &[usize]) -> bool {
        a.len() == b.len()
            && (0..a.len()).all(|i| (0..a.len()).all(|j| (a[i] == a[j]) == (b[i] == b[j])))
    }

    #[test]
    fn matches_scipy_average_linkage() {
        // Reference values produced with SciPy on these exact 8 vectors:
        //   Z = linkage(E, "average", "cosine"); fcluster(Z, k, "maxclust") for k = 2, 3, 4
        let raw: [[f32; 3]; 8] = [
            [0.98, 0.10, 0.05], [0.95, 0.20, 0.00], [0.10, 0.99, 0.05], [0.05, 0.92, 0.30],
            [0.02, 0.10, 0.99], [0.10, 0.00, 0.95], [0.60, 0.60, 0.20], [0.97, 0.12, 0.10],
        ];
        let items: Vec<Vec<f32>> = raw.iter().map(|v| normalized(v.to_vec())).collect();
        let tree = average_linkage(&cosine_distances(&items), items.len());

        let expected_heights = [0.001_540, 0.007_803, 0.008_626, 0.035_817, 0.210_869, 0.642_031, 0.811_912];
        let got = tree.heights();
        assert_eq!(got.len(), expected_heights.len());
        for (g, want) in got.iter().zip(expected_heights) {
            assert!((g - want).abs() < 1e-5, "heights {got:?} vs {expected_heights:?}");
        }
        assert_eq!(tree.labels_for_k(2), [0, 0, 0, 0, 1, 1, 0, 0]);
        assert_eq!(tree.labels_for_k(3), [0, 0, 1, 1, 2, 2, 0, 0]);
        assert_eq!(tree.labels_for_k(4), [0, 0, 1, 1, 2, 2, 3, 0]);
    }

    #[test]
    fn heights_never_decrease_and_k_cuts_are_nested() {
        let (items, _) = blobs(4, 10, 0.45, 7);
        let tree = average_linkage(&cosine_distances(&items), items.len());
        let h = tree.heights();
        assert!(h.windows(2).all(|w| w[0] <= w[1] + 1e-6), "average linkage is monotonic");
        for k in 1..=items.len() {
            assert_eq!(tree.labels_for_k(k).iter().max().unwrap() + 1, k);
        }
    }

    #[test]
    fn recovers_well_separated_groups_and_estimates_their_number() {
        for k in 2..=5 {
            let (items, truth) = blobs(k, 12, 0.18, 100 + k as u64);
            let tree = average_linkage(&cosine_distances(&items), items.len());
            assert_eq!(estimate_num_speakers(&tree, 8), k, "estimated number of speakers");
            assert!(same_partition(&tree.labels_for_k(k), &truth), "k={k} partition");
        }
    }

    #[test]
    fn one_voice_is_reported_as_one_speaker() {
        let (items, _) = blobs(1, 25, 0.25, 9);
        let tree = average_linkage(&cosine_distances(&items), items.len());
        assert_eq!(estimate_num_speakers(&tree, 8), 1);
    }

    #[test]
    fn unusable_embeddings_are_far_from_everything_not_identical() {
        let items = vec![vec![1.0, 0.0], vec![f32::NAN, 0.0], vec![0.0, 1.0]];
        let d = cosine_distances(&items);
        assert_eq!(d[1], 2.0, "NaN is maximally distant");
        assert_eq!(d[3], 2.0);
        assert!(d.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn tiny_inputs_do_not_panic() {
        assert!(average_linkage(&[], 0).labels_for_k(3).is_empty());
        let one = average_linkage(&[0.0], 1);
        assert_eq!(one.labels_for_k(2), vec![0]);
        assert_eq!(estimate_num_speakers(&one, 8), 1);
        let two = average_linkage(&[0.0, 0.7, 0.7, 0.0], 2);
        assert_eq!(two.labels_for_k(2), vec![0, 1]);
        assert_eq!(two.labels_for_k(1), vec![0, 0]);
        assert_eq!(estimate_num_speakers(&two, 8), 2);
    }
}
