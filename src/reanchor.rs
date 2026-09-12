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

use std::fmt;

use crate::document::{AnchorAxis, Document, Feature};
use crate::geometry::Position;
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
pub const Y_ANCHOR_PREFERENCE: [&str; 1] = ["twtt"];

/// What re-anchoring did.
#[derive(Debug, Clone, PartialEq)]
pub struct ReanchorOutcome {
    pub document: Document,
    /// The anchor axis name actually used for each axis.
    pub x_anchor: String,
    pub y_anchor: String,
    /// Features dropped because a coordinate fell outside the target
    /// revision — typically picked in a region the new revision subsets away
    /// (SPEC §8.1).
    pub dropped: Vec<DroppedFeature>,
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

    let x_bounds = x_to.index_range();
    let y_bounds = y_to.index_range();

    let mut out = document.clone();
    let mut kept: Vec<Feature> = Vec::new();
    let mut dropped: Vec<DroppedFeature> = Vec::new();

    for (index, feature) in document.features.iter().enumerate() {
        let mut map_position = |position: &Position| -> Option<Position> {
            let (x, y) = (position.x()?, position.y()?);
            let new_x = x_to.invert(x_from.eval(x));
            let new_y = y_to.invert(y_from.eval(y));
            if !new_x.is_finite() || !new_y.is_finite() {
                return None;
            }
            if let Some((lo, hi)) = x_bounds {
                if new_x < lo || new_x > hi {
                    return None;
                }
            }
            if let Some((lo, hi)) = y_bounds {
                if new_y < lo || new_y > hi {
                    return None;
                }
            }
            Some(position.with_xy(new_x, new_y))
        };

        match feature.geometry.try_map_positions(&mut map_position) {
            Some(geometry) => {
                let mut moved = feature.clone();
                moved.geometry = geometry;
                kept.push(moved);
            }
            None => dropped.push(DroppedFeature {
                index,
                id: feature.id().map(str::to_string),
                label: feature.label().map(str::to_string),
            }),
        }
    }

    out.features = kept;
    Ok(ReanchorOutcome {
        document: out,
        x_anchor: x_name,
        y_anchor: y_name,
        dropped,
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
