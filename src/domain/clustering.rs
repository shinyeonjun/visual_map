//! 결정적 응집형 클러스터링(Deterministic Agglomerative Clustering).
//!
//! 평균 연결(average linkage)을 사용하고, 동점일 때 ID 기반 안정
//! tie-breaking으로 결과를 결정적으로 유지한다. 목표 클러스터 수에 맞춰
//! 임계값 기반 병합 후 필요하면 강제 병합을 수행한다.

use super::feature_graph::SimilarityMatrix;

const DEFAULT_MERGE_THRESHOLD: f64 = 0.08;

/// 하나의 클러스터는 Capability 인덱스 집합이다.
#[derive(Debug, Clone)]
pub(super) struct Cluster {
    pub id: usize,
    pub members: Vec<usize>,
}

/// Hard constraint로 병합을 금지할 Feature 쌍 정보다.
pub(super) struct MergeConstraints {
    pub forbidden_pairs: Vec<(usize, usize)>,
}

impl MergeConstraints {
    pub fn is_forbidden(&self, a: usize, b: usize) -> bool {
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        self.forbidden_pairs
            .iter()
            .any(|&(x, y)| x == lo && y == hi)
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ClusterOptions {
    pub merge_threshold: f64,
    pub target_count: usize,
    pub max_count: usize,
}

impl Default for ClusterOptions {
    fn default() -> Self {
        Self {
            merge_threshold: DEFAULT_MERGE_THRESHOLD,
            target_count: 12,
            max_count: 20,
        }
    }
}

/// 응집형 클러스터링을 실행한다.
pub(super) fn cluster(
    matrix: &SimilarityMatrix,
    constraints: &MergeConstraints,
    options: ClusterOptions,
) -> Vec<Cluster> {
    let n = matrix.size();
    if n == 0 {
        return Vec::new();
    }

    let mut clusters: Vec<Option<Cluster>> = (0..n)
        .map(|i| {
            Some(Cluster {
                id: i,
                members: vec![i],
            })
        })
        .collect();
    let mut active: Vec<usize> = (0..n).collect();

    let mut cluster_sim = vec![vec![0.0_f64; n]; n];
    for i in 0..n {
        for j in (i + 1)..n {
            let sim = matrix.get(i, j).combined;
            cluster_sim[i][j] = sim;
            cluster_sim[j][i] = sim;
        }
    }

    loop {
        let Some((a, b, sim)) = best_merge_pair(&active, &cluster_sim, constraints) else {
            break;
        };
        if active.len() <= options.target_count.max(1) {
            break;
        }
        if sim < options.merge_threshold && active.len() <= options.max_count {
            break;
        }
        if !can_merge(a, b, &clusters, matrix, constraints) {
            cluster_sim[a][b] = -1.0;
            cluster_sim[b][a] = -1.0;
            continue;
        }
        merge_clusters(a, b, &mut clusters, &mut active, &mut cluster_sim, matrix);
    }

    while active.len() > options.max_count {
        let Some((a, b, _)) = best_merge_pair_force(
            &active,
            &cluster_sim,
            constraints,
            &clusters,
            matrix,
        ) else {
            break;
        };
        merge_clusters(a, b, &mut clusters, &mut active, &mut cluster_sim, matrix);
    }

    clusters.into_iter().flatten().collect()
}

fn best_merge_pair(
    active: &[usize],
    cluster_sim: &[Vec<f64>],
    constraints: &MergeConstraints,
) -> Option<(usize, usize, f64)> {
    let mut best_sim = -1.0_f64;
    let mut best_pair: Option<(usize, usize)> = None;
    for (idx_a, &a) in active.iter().enumerate() {
        for &b in active.iter().skip(idx_a + 1) {
            if constraints.is_forbidden(a, b) {
                continue;
            }
            let sim = cluster_sim[a][b];
            if sim < 0.0 {
                continue;
            }
            if sim > best_sim || (sim == best_sim && best_pair.map_or(true, |(pa, pb)| (a, b) < (pa, pb)))
            {
                best_sim = sim;
                best_pair = Some((a, b));
            }
        }
    }
    best_pair.map(|(a, b)| (a, b, best_sim))
}

fn best_merge_pair_force(
    active: &[usize],
    cluster_sim: &[Vec<f64>],
    constraints: &MergeConstraints,
    clusters: &[Option<Cluster>],
    matrix: &SimilarityMatrix,
) -> Option<(usize, usize, f64)> {
    let mut best_sim = -1.0_f64;
    let mut best_pair: Option<(usize, usize)> = None;
    for (idx_a, &a) in active.iter().enumerate() {
        for &b in active.iter().skip(idx_a + 1) {
            if constraints.is_forbidden(a, b) {
                continue;
            }
            if !can_merge(a, b, clusters, matrix, constraints) {
                continue;
            }
            let sim = cluster_sim[a][b];
            if sim > best_sim || (sim == best_sim && best_pair.map_or(true, |(pa, pb)| (a, b) < (pa, pb)))
            {
                best_sim = sim;
                best_pair = Some((a, b));
            }
        }
    }
    best_pair.map(|(a, b)| (a, b, best_sim))
}

fn merge_clusters(
    a: usize,
    b: usize,
    clusters: &mut [Option<Cluster>],
    active: &mut Vec<usize>,
    cluster_sim: &mut [Vec<f64>],
    matrix: &SimilarityMatrix,
) {
    let members_a = clusters[a].as_ref().unwrap().members.clone();
    let members_b = clusters[b].as_ref().unwrap().members.clone();
    let mut merged_members = members_a;
    merged_members.extend(members_b);
    merged_members.sort_unstable();

    let merged_id = a.min(b);
    clusters[a] = None;
    clusters[b] = None;
    clusters[merged_id] = Some(Cluster {
        id: merged_id,
        members: merged_members,
    });

    active.retain(|&x| x != a && x != b);
    active.push(merged_id);
    active.sort_unstable();

    for &other in active.iter() {
        if other == merged_id {
            continue;
        }
        let merged = clusters[merged_id].as_ref().unwrap();
        let other_cluster = clusters[other].as_ref().unwrap();
        let avg = average_linkage_from_matrix(&merged.members, &other_cluster.members, matrix);
        cluster_sim[merged_id][other] = avg;
        cluster_sim[other][merged_id] = avg;
    }
}

fn can_merge(
    a: usize,
    b: usize,
    clusters: &[Option<Cluster>],
    matrix: &SimilarityMatrix,
    constraints: &MergeConstraints,
) -> bool {
    let cluster_a = clusters[a].as_ref().unwrap();
    let cluster_b = clusters[b].as_ref().unwrap();

    for &member_a in &cluster_a.members {
        for &member_b in &cluster_b.members {
            if constraints.is_forbidden(member_a, member_b) {
                return false;
            }
        }
    }

    let mut has_structural = false;
    for &member_a in &cluster_a.members {
        for &member_b in &cluster_b.members {
            let sim = matrix.get(member_a, member_b);
            if sim.http_match > 0.0 || sim.call > 0.0 || sim.flow > 0.0 {
                has_structural = true;
                break;
            }
        }
        if has_structural {
            break;
        }
    }
    has_structural
}

fn average_linkage_from_matrix(
    members_a: &[usize],
    members_b: &[usize],
    matrix: &SimilarityMatrix,
) -> f64 {
    if members_a.is_empty() || members_b.is_empty() {
        return 0.0;
    }
    let mut total = 0.0;
    let mut count = 0;
    for &a in members_a {
        for &b in members_b {
            total += matrix.get(a, b).combined;
            count += 1;
        }
    }
    if count == 0 {
        return 0.0;
    }
    total / count as f64
}

pub(super) fn target_cluster_count(feature_count: usize, min_count: usize, max_count: usize) -> usize {
    if feature_count == 0 {
        return 0;
    }
    let sqrt = (feature_count as f64).sqrt().round() as usize;
    sqrt.clamp(min_count, max_count).min(feature_count)
}

#[cfg(test)]
mod tests {
    use super::{cluster, ClusterOptions, MergeConstraints};
    use crate::domain::feature_graph::SimilarityMatrix;

    #[test]
    fn 클러스터가_하한보다_적어도_억지로_합치지_않는다() {
        let matrix = SimilarityMatrix::uniform(3, 0.2);
        let constraints = MergeConstraints {
            forbidden_pairs: vec![(0, 1), (0, 2), (1, 2)],
        };
        let clusters = cluster(
            &matrix,
            &constraints,
            ClusterOptions {
                merge_threshold: 1.0,
                target_count: 3,
                max_count: 20,
            },
        );
        assert_eq!(clusters.len(), 3);
    }

    #[test]
    fn force_merge도_병합_금지와_구조_가드를_지킨다() {
        let matrix = SimilarityMatrix::uniform(4, 0.95);
        let constraints = MergeConstraints {
            forbidden_pairs: vec![(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)],
        };
        let clusters = cluster(
            &matrix,
            &constraints,
            ClusterOptions {
                merge_threshold: 0.0,
                target_count: 1,
                max_count: 1,
            },
        );
        assert_eq!(clusters.len(), 4);
    }
}
