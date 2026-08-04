use std::collections::{HashMap, HashSet, VecDeque};

use super::model::{
    ChangeIntent, Evidence, ImpactReviewBoard, ImpactReviewItem, ImpactReviewLane, InventoryItem,
    InventorySnapshot, SnapshotLink, SourceLocation, VisualEdge,
};
use super::projection_support::{assign_review_ranks, confidence_rank, safe_evidence, safe_text};

const DIRECT_REVIEW_LIMIT: usize = 12;
const CANDIDATE_REVIEW_LIMIT: usize = 10;
const UNKNOWN_REVIEW_LIMIT: usize = 8;
const CHECK_REVIEW_LIMIT: usize = 10;
type ImpactLinkIndex<'a> = HashMap<&'a str, Vec<&'a SnapshotLink>>;

pub(super) fn impact_review_board(
    snapshot: &InventorySnapshot,
    table: &InventoryItem,
    column: Option<&InventoryItem>,
    candidate_edges: &[VisualEdge],
    change_intent: Option<&ChangeIntent>,
) -> ImpactReviewBoard {
    let item_by_id = snapshot
        .items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let (links_by_from, links_by_to) = impact_link_indexes(snapshot);
    let mut direct = impact_direct_items(
        snapshot,
        table,
        column,
        &item_by_id,
        &links_by_from,
        &links_by_to,
    );
    direct.sort_by(|left, right| {
        direct_review_rank(&left.kind)
            .cmp(&direct_review_rank(&right.kind))
            .then_with(|| left.title.cmp(&right.title))
            .then_with(|| left.id.cmp(&right.id))
    });
    assign_review_ranks(&mut direct);

    let mut candidates = impact_candidate_items(candidate_edges, &item_by_id);
    candidates.sort_by(|left, right| {
        confidence_rank(left.confidence.as_deref().unwrap_or(""))
            .cmp(&confidence_rank(right.confidence.as_deref().unwrap_or("")))
            .then_with(|| {
                candidate_review_rank(&left.kind).cmp(&candidate_review_rank(&right.kind))
            })
            .then_with(|| left.title.cmp(&right.title))
            .then_with(|| left.id.cmp(&right.id))
    });
    assign_review_ranks(&mut candidates);

    let mut unknowns = impact_unknown_items(
        snapshot,
        table,
        column,
        &direct,
        &candidates,
        &item_by_id,
        &links_by_from,
    );
    assign_review_ranks(&mut unknowns);
    let mut checks =
        impact_check_items(snapshot, table, column, &direct, &candidates, change_intent);
    checks.sort_by(|left, right| {
        check_review_rank(&left.kind)
            .cmp(&check_review_rank(&right.kind))
            .then_with(|| left.title.cmp(&right.title))
    });
    assign_review_ranks(&mut checks);

    let subject = match column {
        Some(column) if table.id != column.id => format!("{}.{}", table.name, column.name),
        Some(column) => column.name.clone(),
        None => table.name.clone(),
    };
    let scope = if column.is_some() { "column" } else { "table" }.to_string();
    let lanes = vec![
        review_lane("direct", direct, DIRECT_REVIEW_LIMIT),
        review_lane("candidates", candidates, CANDIDATE_REVIEW_LIMIT),
        review_lane("unknowns", unknowns, UNKNOWN_REVIEW_LIMIT),
        review_lane("checks", checks, CHECK_REVIEW_LIMIT),
    ];
    let markdown_summary = impact_markdown_summary(&subject, change_intent, &lanes);

    ImpactReviewBoard {
        subject,
        scope,
        change_intent: change_intent.cloned(),
        lanes,
        markdown_summary,
    }
}

include!("impact_review/lanes.rs");
include!("impact_review/checks.rs");
include!("impact_review/format.rs");
include!("impact_review/relations.rs");
