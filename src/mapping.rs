//! Anchor axis mappings (SPEC §7.5) and their inverses.
//!
//! Re-anchoring evaluates a mapping forwards on the revision an
//! interpretation was authored against, then inverts it on the target
//! revision (SPEC §8.1). Both directions are therefore first-class, and
//! strict monotonicity is a correctness requirement rather than a stylistic
//! one: a non-monotone mapping has no single inverse, so a consumer would
//! silently pick one of several possible answers.

use std::fmt;

/// A tiepoint in a [`Mapping::Tiepoints`] mapping.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Tiepoint {
    /// Index along the primary (index) axis.
    pub trace: f64,
    /// Value in the anchor axis' unit.
    pub x: f64,
}

/// Why a mapping cannot be used for computation.
#[derive(Debug, Clone, PartialEq)]
pub enum MappingError {
    /// The `type` field was absent.
    MissingType,
    /// The `type` is not one of the types defined in SPEC §7.5. Preserved on
    /// read, but unusable for re-anchoring (SPEC §11).
    UnknownType(String),
    /// A field the mapping type requires was absent.
    MissingField {
        mapping_type: &'static str,
        field: &'static str,
    },
    /// A constraint from SPEC §7.5 was violated.
    NotInvertible(String),
}

impl fmt::Display for MappingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MappingError::MissingType => write!(f, "mapping has no 'type'"),
            MappingError::UnknownType(t) => write!(f, "unknown mapping type '{t}'"),
            MappingError::MissingField {
                mapping_type,
                field,
            } => write!(f, "{mapping_type} mapping has no '{field}'"),
            MappingError::NotInvertible(why) => {
                write!(f, "mapping is not usable for re-anchoring: {why}")
            }
        }
    }
}

impl std::error::Error for MappingError {}

/// A validated, invertible index-to-value mapping.
///
/// Constructed via [`Mapping::from_parts`], which is where every SPEC §7.5
/// constraint is enforced, so holding a `Mapping` means the constraints hold.
#[derive(Debug, Clone, PartialEq)]
pub enum Mapping {
    /// `value = t0 + index * dt` (SPEC §7.5.1).
    Regular { t0: f64, dt: f64 },
    /// Piecewise-linear through explicit tiepoints (SPEC §7.5.2).
    Tiepoints { points: Vec<Tiepoint> },
    /// One value per index (SPEC §7.5.3).
    Explicit { values: Vec<f64> },
}

impl Mapping {
    /// Validate raw fields into a usable mapping.
    pub fn from_parts(
        mapping_type: Option<&str>,
        t0: Option<f64>,
        dt: Option<f64>,
        points: Option<&[Tiepoint]>,
        values: Option<&[f64]>,
    ) -> Result<Mapping, MappingError> {
        match mapping_type.ok_or(MappingError::MissingType)? {
            "regular" => {
                let t0 = t0.ok_or(MappingError::MissingField {
                    mapping_type: "regular",
                    field: "t0",
                })?;
                let dt = dt.ok_or(MappingError::MissingField {
                    mapping_type: "regular",
                    field: "dt",
                })?;
                // `is_finite` first, so a NaN `dt` is rejected here rather
                // than slipping past a comparison that is false for NaN.
                if !dt.is_finite() || dt <= 0.0 {
                    return Err(MappingError::NotInvertible(format!(
                        "dt must be finite and strictly greater than zero, got {dt}"
                    )));
                }
                if !t0.is_finite() {
                    return Err(MappingError::NotInvertible(format!(
                        "t0 must be finite, got {t0}"
                    )));
                }
                Ok(Mapping::Regular { t0, dt })
            }
            "tiepoints" => {
                let points = points.ok_or(MappingError::MissingField {
                    mapping_type: "tiepoints",
                    field: "points",
                })?;
                if points.len() < 2 {
                    return Err(MappingError::NotInvertible(format!(
                        "tiepoints needs at least 2 points to define a mapping, got {}",
                        points.len()
                    )));
                }
                for (i, w) in points.windows(2).enumerate() {
                    if !increases(w[0].trace, w[1].trace) {
                        return Err(MappingError::NotInvertible(format!(
                            "tiepoints must be strictly increasing in 'trace', \
                             but point {} ({}) does not exceed point {i} ({})",
                            i + 1,
                            w[1].trace,
                            w[0].trace
                        )));
                    }
                    if !increases(w[0].x, w[1].x) {
                        return Err(MappingError::NotInvertible(format!(
                            "tiepoints must be strictly increasing in 'x', \
                             but point {} ({}) does not exceed point {i} ({})",
                            i + 1,
                            w[1].x,
                            w[0].x
                        )));
                    }
                }
                Ok(Mapping::Tiepoints {
                    points: points.to_vec(),
                })
            }
            "explicit" => {
                let values = values.ok_or(MappingError::MissingField {
                    mapping_type: "explicit",
                    field: "values",
                })?;
                if values.len() < 2 {
                    return Err(MappingError::NotInvertible(format!(
                        "explicit needs at least 2 values to define a mapping, got {}",
                        values.len()
                    )));
                }
                for (i, w) in values.windows(2).enumerate() {
                    if !increases(w[0], w[1]) {
                        return Err(MappingError::NotInvertible(format!(
                            "explicit values must be strictly increasing, but \
                             value {} ({}) does not exceed value {i} ({})",
                            i + 1,
                            w[1],
                            w[0]
                        )));
                    }
                }
                Ok(Mapping::Explicit {
                    values: values.to_vec(),
                })
            }
            other => Err(MappingError::UnknownType(other.to_string())),
        }
    }

    /// Index -> anchor value.
    ///
    /// Outside the defined index range the first and last segments are
    /// extrapolated (SPEC §7.5.2). Callers that must distinguish
    /// extrapolation from interpolation check [`Mapping::index_range`].
    pub fn eval(&self, index: f64) -> f64 {
        match self {
            Mapping::Regular { t0, dt } => t0 + index * dt,
            Mapping::Tiepoints { points } => {
                let xs: Vec<f64> = points.iter().map(|p| p.trace).collect();
                let ys: Vec<f64> = points.iter().map(|p| p.x).collect();
                interpolate(&xs, &ys, index)
            }
            Mapping::Explicit { values } => {
                let xs: Vec<f64> = (0..values.len()).map(|i| i as f64).collect();
                interpolate(&xs, values, index)
            }
        }
    }

    /// Anchor value -> index. The inverse of [`Mapping::eval`].
    pub fn invert(&self, value: f64) -> f64 {
        match self {
            // dt > 0 is guaranteed by `from_parts`, so this cannot divide by
            // zero.
            Mapping::Regular { t0, dt } => (value - t0) / dt,
            Mapping::Tiepoints { points } => {
                let xs: Vec<f64> = points.iter().map(|p| p.x).collect();
                let ys: Vec<f64> = points.iter().map(|p| p.trace).collect();
                interpolate(&xs, &ys, value)
            }
            Mapping::Explicit { values } => {
                let ys: Vec<f64> = (0..values.len()).map(|i| i as f64).collect();
                interpolate(values, &ys, value)
            }
        }
    }

    /// The index range over which the mapping interpolates rather than
    /// extrapolates. `None` for [`Mapping::Regular`], which is defined
    /// everywhere.
    pub fn index_range(&self) -> Option<(f64, f64)> {
        match self {
            Mapping::Regular { .. } => None,
            Mapping::Tiepoints { points } => {
                Some((points[0].trace, points[points.len() - 1].trace))
            }
            Mapping::Explicit { values } => Some((0.0, (values.len() - 1) as f64)),
        }
    }

    /// The anchor-value range over which [`Mapping::invert`] interpolates
    /// rather than extrapolating.
    pub fn value_range(&self) -> Option<(f64, f64)> {
        match self {
            Mapping::Regular { .. } => None,
            Mapping::Tiepoints { points } => Some((points[0].x, points[points.len() - 1].x)),
            Mapping::Explicit { values } => Some((values[0], values[values.len() - 1])),
        }
    }
}

/// `true` when `b` is strictly greater than `a`.
///
/// Named rather than written inline because the callers all need the
/// *negation* ("this pair is not strictly increasing"), and `!(b > a)` is
/// both hard to read and easy to mistake for `b <= a` -- which it is not,
/// since NaN makes every comparison false. A NaN on either side yields
/// `false` here, so it is correctly reported as breaking monotonicity
/// instead of passing the check.
fn increases(a: f64, b: f64) -> bool {
    b > a
}

/// Piecewise-linear interpolation of `ys` over strictly increasing `xs`,
/// extrapolating along the terminal segments outside the range.
fn interpolate(xs: &[f64], ys: &[f64], at: f64) -> f64 {
    debug_assert_eq!(xs.len(), ys.len());
    debug_assert!(xs.len() >= 2);

    // `partition_point` is a binary search: the mappings are used per
    // coordinate over interpretations that can carry many thousands of
    // vertices, so a linear scan per lookup would dominate.
    let i = xs.partition_point(|v| *v <= at);
    let (lo, hi) = if i == 0 {
        (0, 1)
    } else if i >= xs.len() {
        (xs.len() - 2, xs.len() - 1)
    } else {
        (i - 1, i)
    };
    let span = xs[hi] - xs[lo];
    let t = (at - xs[lo]) / span;
    ys[lo] + t * (ys[hi] - ys[lo])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiepoints(points: &[(f64, f64)]) -> Mapping {
        let pts: Vec<Tiepoint> = points
            .iter()
            .map(|(trace, x)| Tiepoint {
                trace: *trace,
                x: *x,
            })
            .collect();
        Mapping::from_parts(Some("tiepoints"), None, None, Some(&pts), None).unwrap()
    }

    #[test]
    fn regular_round_trips_through_its_inverse() {
        let m = Mapping::from_parts(Some("regular"), Some(-12.0), Some(0.4), None, None).unwrap();
        assert_eq!(m.eval(0.0), -12.0);
        assert_eq!(m.eval(30.0), 0.0);
        for index in [0.0, 1.5, 30.0, 2047.0] {
            assert!((m.invert(m.eval(index)) - index).abs() < 1e-9);
        }
    }

    #[test]
    fn regular_rejects_a_non_positive_dt() {
        // dt <= 0 would make the axis non-monotone and the inverse either
        // undefined or a division by zero.
        for dt in [0.0, -0.4] {
            let err =
                Mapping::from_parts(Some("regular"), Some(0.0), Some(dt), None, None).unwrap_err();
            assert!(matches!(err, MappingError::NotInvertible(_)), "dt={dt}");
        }
    }

    #[test]
    fn tiepoints_interpolate_and_invert() {
        let m = tiepoints(&[(0.0, 100.0), (100.0, 200.0)]);
        assert_eq!(m.eval(50.0), 150.0);
        assert_eq!(m.invert(150.0), 50.0);
    }

    #[test]
    fn tiepoints_extrapolate_along_the_terminal_segments() {
        // SPEC §7.5.2: outside the tiepoint range, continue the first/last
        // segment rather than clamping. Clamping would silently collapse
        // distinct out-of-range picks onto the same index.
        let m = tiepoints(&[(0.0, 0.0), (10.0, 10.0), (20.0, 30.0)]);
        assert_eq!(m.eval(-5.0), -5.0);
        assert_eq!(m.eval(25.0), 40.0);
    }

    #[test]
    fn tiepoints_reject_non_monotone_input() {
        let pts = [
            Tiepoint { trace: 0.0, x: 0.0 },
            Tiepoint {
                trace: 100.0,
                x: 50.0,
            },
            // Goes backwards in `x`: two indices would map to one value, so
            // the inverse is ambiguous.
            Tiepoint {
                trace: 200.0,
                x: 25.0,
            },
        ];
        let err = Mapping::from_parts(Some("tiepoints"), None, None, Some(&pts), None).unwrap_err();
        assert!(matches!(err, MappingError::NotInvertible(_)));
    }

    #[test]
    fn tiepoints_need_at_least_two_points() {
        let pts = [Tiepoint { trace: 0.0, x: 0.0 }];
        assert!(Mapping::from_parts(Some("tiepoints"), None, None, Some(&pts), None).is_err());
    }

    #[test]
    fn explicit_maps_index_to_value_positionally() {
        let m = Mapping::from_parts(Some("explicit"), None, None, None, Some(&[5.0, 6.0, 9.0]))
            .unwrap();
        assert_eq!(m.eval(0.0), 5.0);
        assert_eq!(m.eval(1.5), 7.5);
        assert_eq!(m.invert(9.0), 2.0);
        assert_eq!(m.index_range(), Some((0.0, 2.0)));
    }

    #[test]
    fn unknown_types_are_reported_rather_than_guessed() {
        let err = Mapping::from_parts(Some("polynomial"), None, None, None, None).unwrap_err();
        assert_eq!(err, MappingError::UnknownType("polynomial".into()));
    }

    #[test]
    fn interpolation_is_exact_at_every_tiepoint() {
        // The binary search must land on the tiepoint itself, not one
        // segment off, or re-anchoring drifts at exactly the positions that
        // should be exact.
        let m = tiepoints(&[(0.0, 0.0), (10.0, 100.0), (11.0, 101.0), (500.0, 900.0)]);
        for (trace, value) in [(0.0, 0.0), (10.0, 100.0), (11.0, 101.0), (500.0, 900.0)] {
            assert_eq!(m.eval(trace), value, "eval at {trace}");
            assert_eq!(m.invert(value), trace, "invert at {value}");
        }
    }
}
