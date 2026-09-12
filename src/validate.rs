//! Document validation (SPEC §9).
//!
//! Split into errors and warnings deliberately. The format is permissive by
//! design, so most defects are reported without refusing the document; only
//! the conditions SPEC §9.1 enumerates are fatal. Structural parsing already
//! happened by the time this runs — a `Document` exists — so everything here
//! is semantic.

use std::fmt;

use crate::document::{Convention, Document};
use crate::geometry::Geometry;

/// A condition that makes a document unusable (SPEC §9.1).
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    /// A `convention` value outside the fixed vocabulary (SPEC §7.3).
    /// Fatal rather than a warning: silently accepting an unsupported
    /// origin would flip an interpretation vertically.
    UnsupportedConvention {
        field: &'static str,
        value: String,
        permitted: &'static str,
    },
    /// A feature entry without `type: "Feature"`.
    NotAFeature { index: usize, found: String },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationError::UnsupportedConvention {
                field,
                value,
                permitted,
            } => write!(
                f,
                "convention.{field} is '{value}', but v0.x permits only '{permitted}'"
            ),
            ValidationError::NotAFeature { index, found } => write!(
                f,
                "features[{index}] has type '{found}', expected 'Feature'"
            ),
        }
    }
}

impl std::error::Error for ValidationError {}

/// A defect that does not make the document unusable (SPEC §9.1, §9.2).
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationWarning {
    MissingProperties {
        index: usize,
    },
    MissingLabel {
        index: usize,
    },
    MissingId {
        index: usize,
    },
    DuplicateId {
        index: usize,
        id: String,
    },
    /// A coordinate outside the shape declared in `source` (SPEC §6.2).
    OutOfBounds {
        index: usize,
        axis: &'static str,
        value: f64,
        limit: u64,
    },
    /// An anchor mapping that cannot be used for re-anchoring (SPEC §7.5).
    UnusableAnchor {
        axis: &'static str,
        name: String,
        reason: String,
    },
    /// The document carries no anchor axes, so it is only meaningful against
    /// the exact revision it was authored on (SPEC §7.1).
    NotReanchorable,
    /// A geometry whose type this implementation does not model. It
    /// round-trips, but nothing can be computed from it (SPEC §4.3).
    UnknownGeometryType {
        index: usize,
        type_name: String,
    },
}

impl fmt::Display for ValidationWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationWarning::MissingProperties { index } => {
                write!(f, "features[{index}] has no 'properties'")
            }
            ValidationWarning::MissingLabel { index } => write!(
                f,
                "features[{index}] has no 'properties.label' (the layer name)"
            ),
            ValidationWarning::MissingId { index } => write!(
                f,
                "features[{index}] has no 'properties.id'; array position is not stable identity"
            ),
            ValidationWarning::DuplicateId { index, id } => write!(
                f,
                "features[{index}] repeats 'properties.id' = '{id}', which must be unique"
            ),
            ValidationWarning::OutOfBounds {
                index,
                axis,
                value,
                limit,
            } => write!(
                f,
                "features[{index}] has {axis} = {value}, outside the declared range [0, {limit})"
            ),
            ValidationWarning::UnusableAnchor { axis, name, reason } => write!(
                f,
                "the '{name}' anchor on axis {axis} cannot be used for re-anchoring: {reason}"
            ),
            ValidationWarning::NotReanchorable => write!(
                f,
                "no anchor axes are defined, so this document is only valid against \
                 the exact revision it was authored on"
            ),
            ValidationWarning::UnknownGeometryType { index, type_name } => write!(
                f,
                "features[{index}] has unmodelled geometry type '{type_name}'; \
                 it will round-trip but cannot be interpreted"
            ),
        }
    }
}

/// The outcome of validating a document.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
}

impl Report {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

const PERMITTED_ORIGIN: &str = "upper-left";
const PERMITTED_INDEXING: &str = "zero-based";
const PERMITTED_PIXEL_REFERENCE: &str = "center";

/// Validate a document against SPEC §9.
pub fn validate(document: &Document) -> Report {
    let mut report = Report::default();

    if let Some(convention) = document
        .coordinates
        .as_ref()
        .and_then(|c| c.convention.as_ref())
    {
        check_convention(convention, &mut report);
    }

    check_anchors(document, &mut report);
    check_features(document, &mut report);

    report
}

fn check_convention(convention: &Convention, report: &mut Report) {
    let checks: [(&'static str, &Option<String>, &'static str); 3] = [
        ("origin", &convention.origin, PERMITTED_ORIGIN),
        ("indexing", &convention.indexing, PERMITTED_INDEXING),
        (
            "pixel_reference",
            &convention.pixel_reference,
            PERMITTED_PIXEL_REFERENCE,
        ),
    ];
    for (field, value, permitted) in checks {
        if let Some(value) = value {
            if value != permitted {
                report.errors.push(ValidationError::UnsupportedConvention {
                    field,
                    value: value.clone(),
                    permitted,
                });
            }
        }
    }
}

fn check_anchors(document: &Document, report: &mut Report) {
    let Some(axes) = document.coordinates.as_ref().and_then(|c| c.axes.as_ref()) else {
        report.warnings.push(ValidationWarning::NotReanchorable);
        return;
    };

    let mut any = false;
    for (label, axis) in [("x", &axes.x), ("y", &axes.y)] {
        let Some(axis) = axis else { continue };
        for anchor in axis.anchors() {
            any = true;
            if let Err(error) = anchor.mapping() {
                report.warnings.push(ValidationWarning::UnusableAnchor {
                    axis: if label == "x" { "x" } else { "y" },
                    name: anchor.name.clone(),
                    reason: error.to_string(),
                });
            }
        }
    }
    if !any {
        report.warnings.push(ValidationWarning::NotReanchorable);
    }
}

fn check_features(document: &Document, report: &mut Report) {
    let (n_traces, n_samples) = match &document.source {
        Some(source) => (source.n_traces, source.n_samples),
        None => (None, None),
    };
    let mut seen_ids: Vec<&str> = Vec::new();

    for (index, feature) in document.features.iter().enumerate() {
        if feature.type_ != "Feature" {
            report.errors.push(ValidationError::NotAFeature {
                index,
                found: feature.type_.clone(),
            });
        }

        match &feature.properties {
            None => report
                .warnings
                .push(ValidationWarning::MissingProperties { index }),
            Some(_) => {
                if feature.label().is_none() {
                    report
                        .warnings
                        .push(ValidationWarning::MissingLabel { index });
                }
                match feature.id() {
                    None => report.warnings.push(ValidationWarning::MissingId { index }),
                    Some(id) => {
                        if seen_ids.contains(&id) {
                            report.warnings.push(ValidationWarning::DuplicateId {
                                index,
                                id: id.to_string(),
                            });
                        } else {
                            seen_ids.push(id);
                        }
                    }
                }
            }
        }

        if let Geometry::Unknown(_) = &feature.geometry {
            report
                .warnings
                .push(ValidationWarning::UnknownGeometryType {
                    index,
                    type_name: feature.geometry.type_name().to_string(),
                });
        }

        for position in feature.geometry.positions() {
            for (axis, value, limit) in [
                ("x", position.x(), n_traces),
                ("y", position.y(), n_samples),
            ] {
                let (Some(value), Some(limit)) = (value, limit) else {
                    continue;
                };
                if value < 0.0 || value >= limit as f64 {
                    report.warnings.push(ValidationWarning::OutOfBounds {
                        index,
                        axis,
                        value,
                        limit,
                    });
                }
            }
        }
    }
}
