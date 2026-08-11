/**
 * How much of an area the canvas draws at the current zoom.
 *
 * One threshold gave two states — everything, or a bare block — and fitting a
 * repository-sized map lands just above it. The reader then gets summaries,
 * member chains and a relation count on every line, all rendered too small to
 * read: the map at its most cluttered rather than its most useful. A middle
 * tier keeps the shape of each area without the detail that needs a closer
 * look anyway.
 *
 * Every zoom-dependent decision belongs in the table below. A component that
 * needs a new one adds a field here instead of testing the scale itself, so
 * the tiers cannot drift apart from one another.
 */

export type MapDetail = "overview" | "outline" | "full";

/** Under this the map is a set of labelled blocks and their connections. */
const OUTLINE_SCALE = 0.4;
/** At this size every member is legible, so nothing has to be held back. */
const FULL_SCALE = 0.9;

interface MapDetailPolicy {
  /** The area's one-line responsibility. */
  summary: boolean;
  /** Nested areas. Whether their bodies are drawn follows `members`. */
  subareas: boolean;
  /** Member chains, hidden-member counts, and trace receipts. */
  members: boolean;
}

export const MAP_DETAIL_POLICY: Record<MapDetail, MapDetailPolicy> = {
  overview: { summary: false, subareas: false, members: false },
  outline: { summary: true, subareas: true, members: false },
  full: { summary: true, subareas: true, members: true },
};

export function detailForScale(scale: number): MapDetail {
  if (scale >= FULL_SCALE) return "full";
  if (scale >= OUTLINE_SCALE) return "outline";
  return "overview";
}
