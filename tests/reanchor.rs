//! Re-anchoring behaviour (SPEC §8).
//!
//! The scenario throughout: an interpretation is drawn on revision A, the
//! radargram is reprocessed into revision B, and the picks must land on the
//! same physical reflector in B. Every test states what B did to A.

use gprinterp::{reanchor, AnchorAxis, Document, Geometry, ReanchorError, RevisionAxes};

/// An `original_trace` anchor mapping index `0..=n-1` onto
/// `first..=first+n-1`, i.e. a revision whose first trace was originally
/// trace `first`. This is what subsetting produces.
fn original_trace(first: f64, n: f64) -> AnchorAxis {
    AnchorAxis {
        name: "original_trace".into(),
        unit: Some("index".into()),
        type_: Some("tiepoints".into()),
        points: Some(vec![
            gprinterp::Tiepoint {
                trace: 0.0,
                x: first,
            },
            gprinterp::Tiepoint {
                trace: n - 1.0,
                x: first + n - 1.0,
            },
        ]),
        ..Default::default()
    }
}

fn twtt(t0: f64, dt: f64) -> AnchorAxis {
    AnchorAxis {
        name: "twtt".into(),
        unit: Some("ns".into()),
        type_: Some("regular".into()),
        t0: Some(t0),
        dt: Some(dt),
        ..Default::default()
    }
}

/// The `y` anchor a revision has after antenna-separation correction: the
/// same shape as `twtt`, denoting a different physical quantity.
fn twtt_normal_incidence(t0: f64, dt: f64) -> AnchorAxis {
    AnchorAxis {
        name: "twtt_normal_incidence".into(),
        unit: Some("ns".into()),
        type_: Some("regular".into()),
        t0: Some(t0),
        dt: Some(dt),
        ..Default::default()
    }
}

fn trace_time(first: f64, n: f64, per_trace: f64) -> AnchorAxis {
    AnchorAxis {
        name: "trace_time".into(),
        unit: Some("s".into()),
        type_: Some("tiepoints".into()),
        points: Some(vec![
            gprinterp::Tiepoint {
                trace: 0.0,
                x: first,
            },
            gprinterp::Tiepoint {
                trace: n - 1.0,
                x: first + (n - 1.0) * per_trace,
            },
        ]),
        ..Default::default()
    }
}

/// A one-line interpretation on a revision described by `x`/`y` anchors.
fn document(x: Vec<AnchorAxis>, y: Vec<AnchorAxis>, coords: &[[f64; 2]]) -> Document {
    let geometry = Geometry::LineString(
        coords
            .iter()
            .map(|c| gprinterp::Position(c.to_vec()))
            .collect(),
    );
    let json = serde_json::json!({
        "key": "line_07_003",
        "coordinates": {
            "space": "index2d",
            "axes": {
                "x": { "primary": {"name": "trace_index", "unit": "index"}, "anchor": x },
                "y": { "primary": {"name": "sample_index", "unit": "index"}, "anchor": y }
            }
        },
        "features": [{
            "type": "Feature",
            "geometry": geometry,
            "properties": {"id": "f-0001", "label": "bed"}
        }]
    });
    serde_json::from_value(json).unwrap()
}

fn line_of(document: &Document) -> Vec<(f64, f64)> {
    match &document.features[0].geometry {
        Geometry::LineString(ps) => ps
            .iter()
            .map(|p| (p.x().unwrap(), p.y().unwrap()))
            .collect(),
        other => panic!("expected a LineString, got {}", other.type_name()),
    }
}

/// Compare re-anchored coordinates with a tolerance.
///
/// Re-anchoring evaluates a piecewise-linear mapping and inverts another, so
/// a coordinate that is mathematically an integer arrives as (for example)
/// 10.000000000000028. The crate deliberately does not snap near-integers:
/// picks are legitimately sub-pixel, so rounding would corrupt real data to
/// make test assertions prettier. 1e-9 of a sample is many orders of
/// magnitude below anything the format represents.
fn assert_line_close(document: &Document, expected: &[(f64, f64)]) {
    let actual = line_of(document);
    assert_eq!(
        actual.len(),
        expected.len(),
        "vertex count differs: {actual:?} vs {expected:?}"
    );
    for (i, (got, want)) in actual.iter().zip(expected).enumerate() {
        assert!(
            (got.0 - want.0).abs() < 1e-9 && (got.1 - want.1).abs() < 1e-9,
            "vertex {i}: got {got:?}, expected {want:?}"
        );
    }
}

#[test]
fn identical_revisions_leave_coordinates_untouched() {
    let doc = document(
        vec![original_trace(0.0, 1200.0)],
        vec![twtt(-12.0, 0.4)],
        &[[10.0, 200.0], [500.0, 220.0]],
    );
    let target = RevisionAxes {
        x: vec![original_trace(0.0, 1200.0)],
        y: vec![twtt(-12.0, 0.4)],
    };

    let outcome = reanchor(&doc, &target).unwrap();
    assert_line_close(&outcome.document, &[(10.0, 200.0), (500.0, 220.0)]);
    assert!(outcome.dropped.is_empty());
}

#[test]
fn a_subset_revision_shifts_picks_by_the_subset_offset() {
    // A was the full 1200-trace line; B subsets it to traces 200..1200, so
    // original trace 210 is index 10 in B. A pick at index 210 in A must
    // land on index 10 in B, not stay at 210.
    let doc = document(
        vec![original_trace(0.0, 1200.0)],
        vec![twtt(0.0, 0.4)],
        &[[210.0, 100.0], [700.0, 120.0]],
    );
    let target = RevisionAxes {
        x: vec![original_trace(200.0, 1000.0)],
        y: vec![twtt(0.0, 0.4)],
    };

    let outcome = reanchor(&doc, &target).unwrap();
    assert_line_close(&outcome.document, &[(10.0, 100.0), (500.0, 120.0)]);
    assert_eq!(outcome.x_anchor, "original_trace");
}

#[test]
fn a_different_time_zero_shifts_picks_vertically() {
    // B crops 30 more samples from the top, so its t0 is 12 ns later. A pick
    // at sample 100 in A (twtt 40 ns) is sample 70 in B.
    let doc = document(
        vec![original_trace(0.0, 1200.0)],
        vec![twtt(0.0, 0.4)],
        &[[10.0, 100.0]],
    );
    let target = RevisionAxes {
        x: vec![original_trace(0.0, 1200.0)],
        y: vec![twtt(12.0, 0.4)],
    };

    let outcome = reanchor(&doc, &target).unwrap();
    assert_line_close(&outcome.document, &[(10.0, 70.0)]);
    assert!(
        outcome.y_is_approximate,
        "SPEC §8.3 forbids presenting a re-anchored y as exact"
    );
}

/// A document holding one feature of the given geometry.
fn document_with(geometry: serde_json::Value) -> Document {
    let mut doc = document(
        vec![original_trace(0.0, 1200.0)],
        vec![twtt(0.0, 0.4)],
        &[[0.0, 0.0], [1.0, 0.0]],
    );
    doc.features[0].geometry = serde_json::from_value(geometry).unwrap();
    doc
}

/// B keeps only original traces 500..=1199: index 0 in B is original 500.
fn subset_target() -> RevisionAxes {
    RevisionAxes {
        x: vec![original_trace(500.0, 700.0)],
        y: vec![twtt(0.0, 0.4)],
    }
}

#[test]
fn a_line_crossing_the_edge_is_cut_where_it_crosses_not_dropped() {
    // The pick starts at original trace 210, which B does not contain, and
    // ends at 700, which it does. The part inside B is the line as drawn:
    // it is cut at B's first trace, at the sample the line had reached
    // there, rather than dropped whole or clamped.
    let doc = document(
        vec![original_trace(0.0, 1200.0)],
        vec![twtt(0.0, 0.4)],
        &[[210.0, 100.0], [700.0, 120.0]],
    );

    let outcome = reanchor(&doc, &subset_target()).unwrap();
    assert!(outcome.dropped.is_empty());
    let crossing = 100.0 + 20.0 * (500.0 - 210.0) / (700.0 - 210.0);
    assert_line_close(&outcome.document, &[(0.0, crossing), (200.0, 120.0)]);
    assert_eq!(outcome.clipped.len(), 1);
    let clipped = &outcome.clipped[0];
    assert_eq!(clipped.id.as_deref(), Some("f-0001"));
    assert_eq!(clipped.outside, 1);
    assert!((clipped.beyond_x - 290.0).abs() < 1e-9);
    assert_eq!(clipped.beyond_y, 0.0);
}

#[test]
fn a_line_that_ends_just_past_the_edge_keeps_every_vertex_inside() {
    // The common case: a line drawn to the edge of a 1200-trace radargram
    // whose last vertex landed two traces past it. Every vertex inside is
    // kept exactly, and the overshoot is measured, not hidden.
    let doc = document(
        vec![original_trace(0.0, 1200.0)],
        vec![twtt(0.0, 0.4)],
        &[[-2.0, 50.0], [600.0, 60.0], [1201.0, 70.0]],
    );
    let target = RevisionAxes {
        x: vec![original_trace(0.0, 1200.0)],
        y: vec![twtt(0.0, 0.4)],
    };

    let outcome = reanchor(&doc, &target).unwrap();
    let line = line_of(&outcome.document);
    assert_eq!(line.len(), 3);
    assert!((line[0].0 - 0.0).abs() < 1e-9);
    assert!((line[1].0 - 600.0).abs() < 1e-9 && (line[1].1 - 60.0).abs() < 1e-9);
    assert!((line[2].0 - 1199.0).abs() < 1e-9);
    assert_eq!(outcome.clipped[0].outside, 2);
    assert!((outcome.clipped[0].beyond_x - 2.0).abs() < 1e-9);
}

#[test]
fn a_line_entirely_outside_the_target_is_dropped() {
    let doc = document(
        vec![original_trace(0.0, 1200.0)],
        vec![twtt(0.0, 0.4)],
        &[[210.0, 100.0], [400.0, 120.0]],
    );

    let outcome = reanchor(&doc, &subset_target()).unwrap();
    assert!(outcome.document.features.is_empty());
    assert!(outcome.clipped.is_empty());
    assert_eq!(outcome.dropped.len(), 1);
    assert_eq!(outcome.dropped[0].id.as_deref(), Some("f-0001"));
    assert_eq!(outcome.dropped[0].label.as_deref(), Some("bed"));
}

#[test]
fn a_line_that_leaves_and_comes_back_is_dropped_rather_than_split() {
    // Inside, outside, inside again on y: two pieces, which one
    // `LineString` cannot hold.
    let doc = document(
        vec![original_trace(0.0, 1200.0)],
        vec![twtt(0.0, 0.4)],
        &[[600.0, 0.0], [700.0, 0.0], [800.0, 0.0]],
    );
    let mut target = subset_target();
    // A y axis bounded to samples 0..=99 that the middle vertex leaves.
    target.y = vec![AnchorAxis {
        name: "twtt".into(),
        unit: Some("ns".into()),
        type_: Some("tiepoints".into()),
        points: Some(vec![
            gprinterp::Tiepoint { trace: 0.0, x: 0.0 },
            gprinterp::Tiepoint {
                trace: 99.0,
                x: 39.6,
            },
        ]),
        ..Default::default()
    }];
    let mut doc = doc;
    doc.features[0].geometry = Geometry::LineString(vec![
        gprinterp::Position(vec![600.0, 50.0]),
        gprinterp::Position(vec![700.0, 150.0]),
        gprinterp::Position(vec![800.0, 50.0]),
    ]);

    let outcome = reanchor(&doc, &target).unwrap();
    assert!(outcome.document.features.is_empty());
    assert_eq!(outcome.dropped.len(), 1);
}

#[test]
fn a_point_outside_the_target_is_dropped_not_clamped() {
    // Clamping it to B's first trace would put the pick somewhere it was
    // never made.
    let doc = document_with(serde_json::json!({"type": "Point", "coordinates": [210.0, 100.0]}));
    let outcome = reanchor(&doc, &subset_target()).unwrap();
    assert!(outcome.document.features.is_empty());
    assert_eq!(outcome.dropped.len(), 1);
}

#[test]
fn a_multipoint_loses_only_the_points_outside() {
    let doc = document_with(serde_json::json!({
        "type": "MultiPoint",
        "coordinates": [[210.0, 100.0], [600.0, 100.0], [700.0, 110.0]]
    }));
    let outcome = reanchor(&doc, &subset_target()).unwrap();
    match &outcome.document.features[0].geometry {
        Geometry::MultiPoint(ps) => assert_eq!(ps.len(), 2),
        other => panic!("expected a MultiPoint, got {}", other.type_name()),
    }
    assert_eq!(outcome.clipped[0].outside, 1);
}

#[test]
fn a_multilinestring_keeps_every_piece_inside() {
    // One part crosses the edge and one is outside entirely: the first is
    // cut, the second goes, and the feature stays.
    let doc = document_with(serde_json::json!({
        "type": "MultiLineString",
        "coordinates": [
            [[400.0, 100.0], [600.0, 100.0]],
            [[100.0, 100.0], [200.0, 100.0]]
        ]
    }));
    let outcome = reanchor(&doc, &subset_target()).unwrap();
    match &outcome.document.features[0].geometry {
        Geometry::MultiLineString(parts) => {
            assert_eq!(parts.len(), 1);
            assert!((parts[0][0].x().unwrap() - 0.0).abs() < 1e-9);
            assert!((parts[0][1].x().unwrap() - 100.0).abs() < 1e-9);
        }
        other => panic!("expected a MultiLineString, got {}", other.type_name()),
    }
    assert_eq!(outcome.clipped[0].outside, 3);
}

#[test]
fn a_line_inside_the_target_is_not_reported_as_clipped() {
    let doc = document(
        vec![original_trace(0.0, 1200.0)],
        vec![twtt(0.0, 0.4)],
        &[[600.0, 100.0], [700.0, 120.0]],
    );
    let outcome = reanchor(&doc, &subset_target()).unwrap();
    assert!(outcome.clipped.is_empty());
    assert_line_close(&outcome.document, &[(100.0, 100.0), (200.0, 120.0)]);
}

#[test]
fn original_trace_is_preferred_when_both_x_anchors_are_available() {
    // SPEC §8.2 preference order. Both are present and usable on both sides.
    let doc = document(
        vec![original_trace(0.0, 1200.0), trace_time(1000.0, 1200.0, 0.1)],
        vec![twtt(0.0, 0.4)],
        &[[10.0, 100.0]],
    );
    let target = RevisionAxes {
        x: vec![original_trace(0.0, 1200.0), trace_time(1000.0, 1200.0, 0.1)],
        y: vec![twtt(0.0, 0.4)],
    };

    assert_eq!(reanchor(&doc, &target).unwrap().x_anchor, "original_trace");
}

#[test]
fn trace_time_carries_the_mapping_when_original_trace_is_gone() {
    // B resampled onto a distance grid, so it can no longer describe an
    // original trace, but time interpolates through resampling. B's 600
    // traces span the same 120 s window, so a pick 1/4 of the way along in
    // A lands 1/4 of the way along in B.
    let doc = document(
        vec![original_trace(0.0, 1200.0), trace_time(1000.0, 1200.0, 0.1)],
        vec![twtt(0.0, 0.4)],
        &[[300.0, 100.0]],
    );
    let target = RevisionAxes {
        x: vec![trace_time(1000.0, 600.0, 0.2)],
        y: vec![twtt(0.0, 0.4)],
    };

    let outcome = reanchor(&doc, &target).unwrap();
    assert_eq!(outcome.x_anchor, "trace_time");
    assert_line_close(&outcome.document, &[(150.0, 100.0)]);
}

#[test]
fn no_shared_anchor_is_an_error_rather_than_a_silent_index_fallback() {
    // This is the failure SPEC §8.1 exists to prevent: A only knows original
    // traces, B only knows time, so nothing connects them. Reusing the raw
    // index here would produce plausible-looking, wrong picks.
    let doc = document(
        vec![original_trace(0.0, 1200.0)],
        vec![twtt(0.0, 0.4)],
        &[[210.0, 100.0]],
    );
    let target = RevisionAxes {
        x: vec![trace_time(1000.0, 600.0, 0.2)],
        y: vec![twtt(0.0, 0.4)],
    };

    match reanchor(&doc, &target) {
        Err(ReanchorError::NoSharedAnchor { axis, .. }) => assert_eq!(axis, "x"),
        other => panic!("expected NoSharedAnchor, got {other:?}"),
    }
}

#[test]
fn a_document_without_axes_cannot_be_reanchored() {
    let doc: Document = serde_json::from_value(serde_json::json!({
        "key": "line_07_003",
        "features": [{
            "type": "Feature",
            "geometry": {"type": "LineString", "coordinates": [[10.0, 20.0]]}
        }]
    }))
    .unwrap();

    assert_eq!(
        reanchor(&doc, &RevisionAxes::default()),
        Err(ReanchorError::NoCoordinates)
    );
}

#[test]
fn a_synthetic_anchor_is_refused_across_revisions() {
    // SPEC §8.4: a fabricated trace_time is just the index wearing a
    // costume. Trusting it across revisions is the exact mistake the flag
    // exists to prevent.
    let mut synthetic = trace_time(0.0, 1200.0, 1.0);
    synthetic.synthetic = true;

    let doc = document(
        vec![synthetic.clone()],
        vec![twtt(0.0, 0.4)],
        &[[10.0, 100.0]],
    );
    let target = RevisionAxes {
        x: vec![trace_time(0.0, 1200.0, 1.0)],
        y: vec![twtt(0.0, 0.4)],
    };

    match reanchor(&doc, &target) {
        Err(ReanchorError::SyntheticOnly { axis, name }) => {
            assert_eq!(axis, "x");
            assert_eq!(name, "trace_time");
        }
        other => panic!("expected SyntheticOnly, got {other:?}"),
    }
}

#[test]
fn an_unusable_mapping_is_skipped_in_favour_of_a_usable_one() {
    // A non-monotone original_trace cannot be inverted, so selection must
    // fall through to trace_time rather than failing outright.
    let mut broken = original_trace(0.0, 1200.0);
    broken.points = Some(vec![
        gprinterp::Tiepoint {
            trace: 0.0,
            x: 100.0,
        },
        gprinterp::Tiepoint {
            trace: 1199.0,
            x: 50.0,
        },
    ]);

    let doc = document(
        vec![broken, trace_time(1000.0, 1200.0, 0.1)],
        vec![twtt(0.0, 0.4)],
        &[[300.0, 100.0]],
    );
    let target = RevisionAxes {
        x: vec![trace_time(1000.0, 1200.0, 0.1)],
        y: vec![twtt(0.0, 0.4)],
    };

    assert_eq!(reanchor(&doc, &target).unwrap().x_anchor, "trace_time");
}

/// An antenna-separation-corrected revision does not share a `y` anchor
/// with an uncorrected one, and the refusal is what keeps the picks honest.
///
/// Both axes are linear in sample index and both hold plausible
/// nanosecond values, so nothing in the *numbers* distinguishes them. Had
/// the corrected revision kept the name `twtt`, this would re-anchor
/// happily and be wrong by the antenna geometry — silently, since the
/// output would look entirely reasonable. The distinct name is the whole
/// mechanism.
#[test]
fn a_corrected_revision_shares_no_y_anchor_with_an_uncorrected_one() {
    let drawn_on_uncorrected = document(
        vec![trace_time(1000.0, 100.0, 0.1)],
        vec![twtt(0.0, 0.4)],
        &[[10.0, 20.0], [50.0, 30.0]],
    );
    let corrected = RevisionAxes {
        x: vec![trace_time(1000.0, 100.0, 0.1)],
        y: vec![twtt_normal_incidence(0.0, 0.4)],
    };

    match reanchor::reanchor(&drawn_on_uncorrected, &corrected) {
        Err(ReanchorError::NoSharedAnchor { axis, .. }) => assert_eq!(axis, "y"),
        other => panic!("expected the y axes to be incompatible, got {other:?}"),
    }

    // And the mirror: picks drawn on the corrected revision cannot be
    // carried back onto the uncorrected one either.
    let drawn_on_corrected = document(
        vec![trace_time(1000.0, 100.0, 0.1)],
        vec![twtt_normal_incidence(0.0, 0.4)],
        &[[10.0, 20.0]],
    );
    let uncorrected = RevisionAxes {
        x: vec![trace_time(1000.0, 100.0, 0.1)],
        y: vec![twtt(0.0, 0.4)],
    };
    assert!(matches!(
        reanchor::reanchor(&drawn_on_corrected, &uncorrected),
        Err(ReanchorError::NoSharedAnchor { axis: "y", .. })
    ));
}

/// Two corrected revisions re-anchor through `twtt_normal_incidence`
/// normally, so the new name costs nothing where it is shared.
#[test]
fn two_corrected_revisions_reanchor_through_normal_incidence() {
    // B cropped 10 leading samples, which `t0` records.
    let drawn_on_a = document(
        vec![trace_time(1000.0, 100.0, 0.1)],
        vec![twtt_normal_incidence(0.0, 0.4)],
        &[[10.0, 20.0], [50.0, 30.0]],
    );
    let b = RevisionAxes {
        x: vec![trace_time(1000.0, 100.0, 0.1)],
        y: vec![twtt_normal_incidence(4.0, 0.4)],
    };

    let outcome = reanchor::reanchor(&drawn_on_a, &b).expect("shared anchor on both axes");
    assert_eq!(outcome.y_anchor, "twtt_normal_incidence");
    assert_line_close(&outcome.document, &[(10.0, 10.0), (50.0, 20.0)]);
}

/// A corrected revision that also emits `twtt` (SPEC §8.3) regains a shared
/// anchor with an uncorrected one.
///
/// This is the case the SHOULD in §8.3 exists for: the producer knows the
/// antenna separation and the velocity it corrected with, so it can say
/// where each corrected sample sits in recorded-time space, and an
/// otherwise unresolvable pair becomes ordinary.
#[test]
fn a_corrected_revision_that_also_emits_twtt_can_be_reanchored() {
    let drawn_on_uncorrected = document(
        vec![trace_time(1000.0, 100.0, 0.1)],
        vec![twtt(0.0, 0.4)],
        &[[10.0, 20.0]],
    );
    let corrected_but_honest = RevisionAxes {
        x: vec![trace_time(1000.0, 100.0, 0.1)],
        // Its own grid, plus where those samples fall in recorded time.
        y: vec![twtt_normal_incidence(0.0, 0.4), twtt(0.0, 0.5)],
    };

    let outcome =
        reanchor::reanchor(&drawn_on_uncorrected, &corrected_but_honest).expect("twtt is shared");
    assert_eq!(
        outcome.y_anchor, "twtt",
        "the only anchor both sides offer is the recorded one"
    );
    // 20 samples x 0.4 ns = 8 ns, which is sample 16 at 0.5 ns.
    assert_line_close(&outcome.document, &[(10.0, 16.0)]);
}

/// Where both revisions offer both `y` anchors, `twtt_normal_incidence`
/// wins (SPEC §8.2) — and the two anchors are rigged to disagree, so
/// reversing `Y_ANCHOR_PREFERENCE` fails this test rather than passing it
/// by coincidence.
///
/// Two revisions corrected with different velocities land on the same
/// normal-incidence grid while relating it to recorded time differently,
/// which is what makes the two routes give different answers here.
#[test]
fn normal_incidence_is_preferred_where_both_anchors_are_shared() {
    let drawn_on_a = document(
        vec![trace_time(1000.0, 100.0, 0.1)],
        vec![twtt_normal_incidence(0.0, 0.4), twtt(0.0, 0.5)],
        &[[10.0, 20.0]],
    );
    let b = RevisionAxes {
        x: vec![trace_time(1000.0, 100.0, 0.1)],
        y: vec![twtt_normal_incidence(0.0, 0.4), twtt(0.0, 0.25)],
    };

    let outcome = reanchor::reanchor(&drawn_on_a, &b).expect("both axes are shared");
    assert_eq!(outcome.y_anchor, "twtt_normal_incidence");
    // Through normal incidence: 20 x 0.4 ns = 8 ns, which is sample 20 of
    // B's identical grid. Through twtt it would have been 10 ns and
    // sample 40, so the assertion below distinguishes the two routes.
    assert_line_close(&outcome.document, &[(10.0, 20.0)]);
}

/// Build a document on one `y` anchor and re-anchor it onto another.
fn document_on(y_anchors: serde_json::Value, points: &[[f64; 2]]) -> gprinterp::Document {
    serde_json::from_value(serde_json::json!({
        "key": "line-01",
        "coordinates": {"space": "index", "axes": {
            "x": {"anchor": [{"name": "trace_time", "unit": "s",
                              "type": "regular", "t0": 0.0, "dt": 1.0}]},
            "y": y_anchors,
        }},
        "features": [{
            "type": "Feature",
            "geometry": {"type": "LineString", "coordinates": points},
            "properties": {"id": "f-0001", "label": "bed"}
        }]
    }))
    .unwrap()
}

fn axis(name: &str, t0: f64, dt: f64) -> gprinterp::AnchorAxis {
    serde_json::from_value(serde_json::json!({
        "name": name, "unit": "ns", "type": "regular", "t0": t0, "dt": dt
    }))
    .unwrap()
}

#[test]
fn a_corrected_and_an_uncorrected_revision_relate_through_the_recording_clock() {
    // The case §8.5 exists for, from real data: one revision has had a
    // time-zero correction and the other has not, so they share no
    // travel-time axis at all. Refusing would be correct and useless --
    // both know exactly where their first sample sits on the original
    // recording's clock, so they are perfectly relatable through it.
    //
    // Corrected: 50.7572 ns cropped from the front, time zero located
    // there, so its `twtt` starts at 0 and its `recording_time` at 50.7572.
    // Uncorrected: nothing cropped, time zero unknown, `recording_time`
    // starts at 0 and there is no `twtt`.
    let dt = 1.586162;
    let corrected = document_on(
        serde_json::json!({"anchor": [
            {"name": "twtt", "unit": "ns", "type": "regular", "t0": 0.0, "dt": dt},
            {"name": "recording_time", "unit": "ns", "type": "regular", "t0": 50.7572, "dt": dt},
        ]}),
        &[[10.0, 700.0], [40.0, 900.0]],
    );
    let uncorrected = gprinterp::RevisionAxes {
        x: vec![serde_json::from_value(serde_json::json!({
            "name": "trace_time", "unit": "s", "type": "regular", "t0": 0.0, "dt": 1.0
        }))
        .unwrap()],
        y: vec![axis("recording_time", 0.0, dt)],
    };

    let out = gprinterp::reanchor(&corrected, &uncorrected).expect("relatable");
    assert_eq!(out.y_anchor, "recording_time");
    assert!(out.dropped.is_empty());

    // 50.7572 / 1.586162 = 32 samples, so every pick moves down by 32.
    // The tolerance is 1e-4 rather than 1e-9 because the crop and the
    // sample interval here are the file's own values rounded to six
    // figures, which puts the quotient at 32.0000063 -- a property of the
    // literals in this test, not of the arithmetic being checked.
    let positions = out.document.features[0].geometry.positions();
    assert!(
        (positions[0].y().unwrap() - 732.0).abs() < 1e-4,
        "{:?}",
        positions[0].y()
    );
    assert!((positions[1].y().unwrap() - 932.0).abs() < 1e-4);
    // And x is untouched, because nothing about the traces changed.
    assert!((positions[0].x().unwrap() - 10.0).abs() < 1e-9);
}

#[test]
fn the_recording_clock_is_preferred_over_a_shared_travel_time_axis() {
    // Same crop, time zero located 20 ns apart: at the onset of the direct
    // wave on one revision and at its peak on the other. The data are the
    // same samples on the same clock, so a feature belongs at the same
    // sample; through `twtt` it would move 20 samples off the reflector it
    // was drawn on, because the reflector's travel time changed with time
    // zero and the feature's did not.
    let dt = 1.0;
    let doc = document_on(
        serde_json::json!({"anchor": [
            {"name": "twtt", "unit": "ns", "type": "regular", "t0": 0.0, "dt": dt},
            {"name": "recording_time", "unit": "ns", "type": "regular", "t0": 50.0, "dt": dt},
        ]}),
        &[[10.0, 100.0]],
    );
    let target = gprinterp::RevisionAxes {
        x: vec![serde_json::from_value(serde_json::json!({
            "name": "trace_time", "unit": "s", "type": "regular", "t0": 0.0, "dt": 1.0
        }))
        .unwrap()],
        y: vec![axis("twtt", -20.0, dt), axis("recording_time", 50.0, dt)],
    };

    let out = gprinterp::reanchor(&doc, &target).expect("relatable");
    assert_eq!(out.y_anchor, "recording_time");
    let y = out.document.features[0].geometry.positions()[0]
        .y()
        .unwrap();
    // Through `twtt` this would be 120.
    assert!((y - 100.0).abs() < 1e-9, "carried through the clock: {y}");
}

#[test]
fn travel_time_is_used_where_either_side_has_no_recording_clock() {
    // A per-trace crop or an antenna-separation correction that cannot be
    // inverted leaves a revision without a regular clock; travel time is
    // then the only shared axis, and the order changes nothing.
    let dt = 1.0;
    let doc = document_on(
        serde_json::json!({"anchor": [
            {"name": "twtt", "unit": "ns", "type": "regular", "t0": 0.0, "dt": dt},
            {"name": "recording_time", "unit": "ns", "type": "regular", "t0": 50.0, "dt": dt},
        ]}),
        &[[10.0, 100.0]],
    );
    let target = gprinterp::RevisionAxes {
        x: vec![serde_json::from_value(serde_json::json!({
            "name": "trace_time", "unit": "s", "type": "regular", "t0": 0.0, "dt": 1.0
        }))
        .unwrap()],
        y: vec![axis("twtt", 20.0, dt)],
    };

    let out = gprinterp::reanchor(&doc, &target).expect("relatable");
    assert_eq!(out.y_anchor, "twtt");
    let y = out.document.features[0].geometry.positions()[0]
        .y()
        .unwrap();
    assert!((y - 80.0).abs() < 1e-9, "carried through twtt: {y}");
}
