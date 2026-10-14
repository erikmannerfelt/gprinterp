//! Re-anchoring an interpretation onto a different processed revision
//! (SPEC §8).
//!
//! An interpretation stores trace/sample indices, which belong to the exact
//! revision it was drawn on. Re-anchoring evaluates an invariant quantity
//! from those indices on the authoring revision, then inverts it on the
//! target revision. The two hard rules from SPEC §8.1 are enforced here
//! rather than left to callers: no shared anchor axis means refusal, never a
//! silent fall back to the raw index, and out-of-range coordinates are
//! dropped and counted rather than clamped to the edge.
//!
//! A line is many coordinates, so a line that runs past the edge is cut
//! where it crosses it rather than dropped whole: the part inside is the
//! line exactly as drawn, and the part outside is reported in
//! [`ReanchorOutcome::clipped`]. Dropping the whole line would lose every
//! pick inside the revision because one end overshot it, which is common —
//! a line drawn to the edge of a radargram easily ends a trace beyond it.

use std::fmt;

use crate::document::{AnchorAxis, Document, Feature};
use crate::geometry::{Geometry, Position};
use crate::mapping::Mapping;

/// The anchor axes a target revision offers.
///
/// Built by the consumer from the radargram it is about to apply an
/// interpretation to — for Ridal, from the processed NetCDF.
#[derive(Debug, Clone, Default)]
pub struct RevisionAxes {
    pub x: Vec<AnchorAxis>,
    pub y: Vec<AnchorAxis>,
}

impl RevisionAxes {
    fn find(list: &[AnchorAxis], name: &str) -> Option<(AnchorAxis, Mapping)> {
        let anchor = list.iter().find(|a| a.name == name)?;
        let mapping = anchor.mapping().ok()?;
        Some((anchor.clone(), mapping))
    }
}

/// Why an interpretation could not be re-anchored.
#[derive(Debug, Clone, PartialEq)]
pub enum ReanchorError {
    /// The document carries no axis metadata (SPEC §7.1), so there is
    /// nothing to re-anchor through.
    NoCoordinates,
    /// No anchor axis name is usable on both revisions for this axis
    /// (SPEC §8.1).
    NoSharedAnchor {
        axis: &'static str,
        document_offers: Vec<String>,
        revision_offers: Vec<String>,
    },
    /// The only shared axis is synthetic on one side, so it carries no
    /// information across revisions (SPEC §8.4).
    SyntheticOnly { axis: &'static str, name: String },
}

impl fmt::Display for ReanchorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReanchorError::NoCoordinates => write!(
                f,
                "the interpretation declares no coordinate axes, so it is only \
                 valid against the revision it was authored on"
            ),
            ReanchorError::NoSharedAnchor {
                axis,
                document_offers,
                revision_offers,
            } => write!(
                f,
                "no anchor axis for {axis} is available on both sides \
                 (interpretation offers [{}], radargram offers [{}])",
                document_offers.join(", "),
                revision_offers.join(", ")
            ),
            ReanchorError::SyntheticOnly { axis, name } => write!(
                f,
                "the only shared {axis} anchor ('{name}') is synthetic on at least \
                 one side, so it carries no information across revisions"
            ),
        }
    }
}

impl std::error::Error for ReanchorError {}

/// Preference order for `x` anchors (SPEC §8.2).
///
/// `original_trace` first because it is an exact integer identity rather
/// than an interpolated physical quantity. `trace_time` remains defined
/// where `original_trace` stops being meaningful — after resampling or
/// stacking, where output traces are interpolated combinations of inputs.
pub const X_ANCHOR_PREFERENCE: [&str; 2] = ["original_trace", "trace_time"];

/// Preference order for `y` anchors (SPEC §8.2).
///
/// The two are different physical quantities, not two names for one:
/// `twtt` is what a separated source and receiver recorded, and
/// `twtt_normal_incidence` is what a coincident pair would have. A
/// revision that has had antenna-separation correction applied sits on the
/// second; one that has not sits on the first.
///
/// `twtt_normal_incidence` comes first because where both are present it
/// is the grid the data actually occupies, while `twtt` on a corrected
/// revision is derived back out of it. Where only one is shared the order
/// does not arise — and when neither is, §8.1's refusal is exactly right,
/// because interpolating between the two would be silently wrong rather
/// than approximate.
///
/// `recording_time` is last, and is not a travel time at all: it is the
/// original recording's clock, whose origin is wherever the instrument
/// triggered. It relates two revisions of one recording exactly — including
/// the common case where one has had a time-zero correction and the other
/// has not, which share no travel-time axis and would otherwise be
/// unrelatable — and it relates nothing else. Preferring it over `twtt`
/// would relate two revisions by a coincidence of cropping rather than by
/// physics. See SPEC §8.5.
pub const Y_ANCHOR_PREFERENCE: [&str; 3] = ["twtt_normal_incidence", "twtt", "recording_time"];

/// What re-anchoring did.
#[derive(Debug, Clone, PartialEq)]
pub struct ReanchorOutcome {
    pub document: Document,
    /// The anchor axis name actually used for each axis.
    pub x_anchor: String,
    pub y_anchor: String,
    /// Features dropped because a coordinate fell outside the target
    /// revision — typically picked in a region the new revision subsets away
    /// (SPEC §8.1). A line is dropped only when none of it is inside, or
    /// when it leaves and re-enters, since a `LineString` cannot hold two
    /// pieces; one that crosses the edge is in [`Self::clipped`] instead.
    pub dropped: Vec<DroppedFeature>,
    /// Features kept with part of them cut away at the edge of the target
    /// revision: lines cut where they cross it, multi-points without the
    /// points outside it.
    pub clipped: Vec<ClippedFeature>,
    /// `true` when `y` was re-anchored across differing revisions, which
    /// SPEC §8.3 requires consumers to warn about and never present as
    /// exact.
    pub y_is_approximate: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DroppedFeature {
    pub index: usize,
    pub id: Option<String>,
    pub label: Option<String>,
}

/// A feature kept in part, and how much of it lay outside the target.
#[derive(Debug, Clone, PartialEq)]
pub struct ClippedFeature {
    /// Its position in the input document's `features`.
    pub index: usize,
    pub id: Option<String>,
    pub label: Option<String>,
    /// How many of its stored coordinates fell outside the target revision.
    pub outside: usize,
    /// How far past the edge the farthest of those reached, in the target's
    /// index units: traces for `x`, samples for `y`. Zero along an axis the
    /// feature did not leave by.
    ///
    /// The distance is what tells an overshoot apart from a loss. A line
    /// drawn to the edge that ends half a trace beyond it lost nothing
    /// anybody picked; one that ran a hundred traces into a region the new
    /// revision subsets away did.
    pub beyond_x: f64,
    pub beyond_y: f64,
}

/// The target revision's extent in index units. `None` on a side means
/// unbounded: a regular mapping is defined everywhere (SPEC §7.5.1).
#[derive(Debug, Clone, Copy)]
struct Bounds {
    x: Option<(f64, f64)>,
    y: Option<(f64, f64)>,
}

impl Bounds {
    fn past(range: Option<(f64, f64)>, v: f64) -> f64 {
        match range {
            Some((lo, _)) if v < lo => lo - v,
            Some((_, hi)) if v > hi => v - hi,
            _ => 0.0,
        }
    }

    /// How far `p` lies past each edge, `(0, 0)` when inside.
    fn beyond(&self, p: &Position) -> (f64, f64) {
        (
            Self::past(self.x, p.x().unwrap_or(0.0)),
            Self::past(self.y, p.y().unwrap_or(0.0)),
        )
    }

    fn contains(&self, p: &Position) -> bool {
        self.beyond(p) == (0.0, 0.0)
    }

    /// The part of the segment `a`–`b` inside, as parameters `0 <= t0 <=
    /// t1 <= 1` along it (Liang–Barsky), or `None` when none of it is.
    fn clip_segment(&self, a: &Position, b: &Position) -> Option<(f64, f64)> {
        let (ax, ay) = (a.x()?, a.y()?);
        let (dx, dy) = (b.x()? - ax, b.y()? - ay);
        let mut t0: f64 = 0.0;
        let mut t1: f64 = 1.0;
        let mut edge = |p: f64, q: f64| -> bool {
            if p == 0.0 {
                // Parallel to this edge: inside it entirely or not at all.
                return q >= 0.0;
            }
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
            t0 <= t1
        };
        if let Some((lo, hi)) = self.x {
            if !edge(-dx, ax - lo) || !edge(dx, hi - ax) {
                return None;
            }
        }
        if let Some((lo, hi)) = self.y {
            if !edge(-dy, ay - lo) || !edge(dy, hi - ay) {
                return None;
            }
        }
        Some((t0, t1))
    }
}

/// The point a fraction `t` of the way from `a` to `b`.
///
/// The endpoints are returned as they are rather than recomputed, so a
/// vertex inside the target keeps its exact value and any elements beyond
/// `x` and `y`.
fn along(a: &Position, b: &Position, t: f64) -> Position {
    if t <= 0.0 {
        return a.clone();
    }
    if t >= 1.0 {
        return b.clone();
    }
    let (ax, ay) = (a.x().unwrap_or(0.0), a.y().unwrap_or(0.0));
    let (bx, by) = (b.x().unwrap_or(0.0), b.y().unwrap_or(0.0));
    a.with_xy(ax + (bx - ax) * t, ay + (by - ay) * t)
}

/// The pieces of a line that lie inside `bounds`, each with at least two
/// vertices.
///
/// The vertices inside are kept exactly; where the line crosses the edge, a
/// vertex is added at the crossing. That vertex is on the line as drawn, so
/// this is not clamping: clamping moves a coordinate to the edge and bends
/// the line to get there, while this keeps the line and stops it.
fn clip_line(line: &[Position], bounds: &Bounds) -> Vec<Vec<Position>> {
    if line.len() < 2 {
        return if line.iter().all(|p| bounds.contains(p)) && !line.is_empty() {
            vec![line.to_vec()]
        } else {
            Vec::new()
        };
    }
    let mut pieces: Vec<Vec<Position>> = Vec::new();
    let mut current: Vec<Position> = Vec::new();
    for pair in line.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        // A segment that only touches the edge at one point adds nothing
        // drawable, and would otherwise split the line in two.
        match bounds.clip_segment(a, b).filter(|(t0, t1)| t1 > t0) {
            Some((t0, t1)) => {
                // Continuing the piece only when this segment starts where
                // the last one ended inside; anything else is a new piece.
                if t0 > 0.0 || current.is_empty() {
                    if current.len() >= 2 {
                        pieces.push(std::mem::take(&mut current));
                    }
                    current.clear();
                    current.push(along(a, b, t0));
                }
                current.push(along(a, b, t1));
                if t1 < 1.0 {
                    pieces.push(std::mem::take(&mut current));
                }
            }
            None => {
                if current.len() >= 2 {
                    pieces.push(std::mem::take(&mut current));
                }
                current.clear();
            }
        }
    }
    if current.len() >= 2 {
        pieces.push(current);
    }
    pieces
}

/// How many of `positions` lie outside `bounds`, and how far the farthest
/// reached past each edge.
fn outside(positions: &[&Position], bounds: &Bounds) -> (usize, f64, f64) {
    positions
        .iter()
        .map(|p| bounds.beyond(p))
        .filter(|&(x, y)| x > 0.0 || y > 0.0)
        .fold((0, 0.0_f64, 0.0_f64), |(n, bx, by), (x, y)| {
            (n + 1, bx.max(x), by.max(y))
        })
}

/// `geometry`, already in the target's index space, cut to `bounds`.
///
/// `None` when nothing of it is inside. Lines and multi-points are cut;
/// a point, a polygon and a collection are all-or-nothing as before, since
/// cutting an area at an edge is a different operation that nothing needs
/// yet.
fn clip(geometry: &Geometry, bounds: &Bounds) -> Option<Geometry> {
    match geometry {
        Geometry::LineString(line) => {
            let mut pieces = clip_line(line, bounds);
            // One piece is the line, cut. Two would need a second feature
            // or a change of geometry type, and a reader that draws
            // `LineString`s would then silently lose it: dropping and
            // reporting it is the honest choice.
            (pieces.len() == 1).then(|| Geometry::LineString(pieces.remove(0)))
        }
        Geometry::MultiLineString(lines) => {
            let pieces: Vec<Vec<Position>> =
                lines.iter().flat_map(|l| clip_line(l, bounds)).collect();
            (!pieces.is_empty()).then_some(Geometry::MultiLineString(pieces))
        }
        Geometry::MultiPoint(points) => {
            let kept: Vec<Position> = points
                .iter()
                .filter(|p| bounds.contains(p))
                .cloned()
                .collect();
            (!kept.is_empty()).then_some(Geometry::MultiPoint(kept))
        }
        other => other
            .positions()
            .iter()
            .all(|p| bounds.contains(p))
            .then(|| other.clone()),
    }
}

/// Apply `document` to a revision described by `target`.
pub fn reanchor(
    document: &Document,
    target: &RevisionAxes,
) -> Result<ReanchorOutcome, ReanchorError> {
    let axes = document
        .coordinates
        .as_ref()
        .and_then(|c| c.axes.as_ref())
        .ok_or(ReanchorError::NoCoordinates)?;

    let (x_name, x_from, x_to, x_synthetic) = select(
        "x",
        axes.x.as_ref().map(|a| a.anchors()).unwrap_or(&[]),
        &target.x,
        &X_ANCHOR_PREFERENCE,
    )?;
    let (y_name, y_from, y_to, y_synthetic) = select(
        "y",
        axes.y.as_ref().map(|a| a.anchors()).unwrap_or(&[]),
        &target.y,
        &Y_ANCHOR_PREFERENCE,
    )?;

    // A synthetic axis on both sides is only meaningful if the same
    // synthesis rule applies to both, which this crate cannot verify
    // (SPEC §8.4). Refusing is the conservative reading.
    if x_synthetic {
        return Err(ReanchorError::SyntheticOnly {
            axis: "x",
            name: x_name,
        });
    }
    if y_synthetic {
        return Err(ReanchorError::SyntheticOnly {
            axis: "y",
            name: y_name,
        });
    }

    let bounds = Bounds {
        x: x_to.index_range(),
        y: y_to.index_range(),
    };

    let mut out = document.clone();
    let mut kept: Vec<Feature> = Vec::new();
    let mut dropped: Vec<DroppedFeature> = Vec::new();
    let mut clipped: Vec<ClippedFeature> = Vec::new();

    for (index, feature) in document.features.iter().enumerate() {
        // Carried first, cut second: whether a coordinate is inside is a
        // question about the target's index space, and the cut has to be
        // made there, where the line is straight between its vertices.
        // Every mapping extrapolates (SPEC §7.5), so a coordinate outside
        // still has a place to be cut from.
        let mut map_position = |position: &Position| -> Option<Position> {
            let (x, y) = (position.x()?, position.y()?);
            let new_x = x_to.invert(x_from.eval(x));
            let new_y = y_to.invert(y_from.eval(y));
            if !new_x.is_finite() || !new_y.is_finite() {
                return None;
            }
            Some(position.with_xy(new_x, new_y))
        };
        let drop = DroppedFeature {
            index,
            id: feature.id().map(str::to_string),
            label: feature.label().map(str::to_string),
        };
        let Some(carried) = feature.geometry.try_map_positions(&mut map_position) else {
            dropped.push(drop);
            continue;
        };
        let Some(geometry) = clip(&carried, &bounds) else {
            dropped.push(drop);
            continue;
        };
        let (n_outside, beyond_x, beyond_y) = outside(&carried.positions(), &bounds);
        if n_outside > 0 {
            clipped.push(ClippedFeature {
                index,
                id: drop.id,
                label: drop.label,
                outside: n_outside,
                beyond_x,
                beyond_y,
            });
        }
        let mut moved = feature.clone();
        moved.geometry = geometry;
        kept.push(moved);
    }

    out.features = kept;
    Ok(ReanchorOutcome {
        document: out,
        x_anchor: x_name,
        y_anchor: y_name,
        dropped,
        clipped,
        y_is_approximate: true,
    })
}

/// Pick the highest-preference anchor axis usable on both sides.
fn select(
    axis: &'static str,
    document_anchors: &[AnchorAxis],
    revision_anchors: &[AnchorAxis],
    preference: &[&str],
) -> Result<(String, Mapping, Mapping, bool), ReanchorError> {
    for name in preference {
        let Some((from_axis, from)) = RevisionAxes::find(document_anchors, name) else {
            continue;
        };
        let Some((to_axis, to)) = RevisionAxes::find(revision_anchors, name) else {
            continue;
        };
        return Ok((
            (*name).to_string(),
            from,
            to,
            from_axis.synthetic || to_axis.synthetic,
        ));
    }
    Err(ReanchorError::NoSharedAnchor {
        axis,
        document_offers: document_anchors.iter().map(|a| a.name.clone()).collect(),
        revision_offers: revision_anchors.iter().map(|a| a.name.clone()).collect(),
    })
}
