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

#[test]
fn picks_outside_the_target_revision_are_dropped_not_clamped() {
    // B keeps only traces 500..1200. The pick starts at original trace 210,
    // which B does not contain. Clamping it to B's first trace would put the
    // interpretation somewhere it was never drawn.
    let doc = document(
        vec![original_trace(0.0, 1200.0)],
        vec![twtt(0.0, 0.4)],
        &[[210.0, 100.0], [700.0, 120.0]],
    );
    let target = RevisionAxes {
        x: vec![original_trace(500.0, 700.0)],
        y: vec![twtt(0.0, 0.4)],
    };

    let outcome = reanchor(&doc, &target).unwrap();
    assert!(outcome.document.features.is_empty());
    assert_eq!(outcome.dropped.len(), 1);
    assert_eq!(outcome.dropped[0].id.as_deref(), Some("f-0001"));
    assert_eq!(outcome.dropped[0].label.as_deref(), Some("bed"));
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
