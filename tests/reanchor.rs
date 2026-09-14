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
fn a_shared_travel_time_axis_is_preferred_over_the_recording_clock() {
    // `recording_time` relates revisions by a coincidence of cropping.
    // Where both sides know their travel time, that is the real physics
    // and must win -- otherwise two revisions cropped identically but
    // corrected differently would look identical.
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
        // Same crop, different time zero: the two disagree about travel
        // time by 20 ns and agree about the clock exactly.
        y: vec![axis("twtt", 20.0, dt), axis("recording_time", 50.0, dt)],
    };

    let out = gprinterp::reanchor(&doc, &target).expect("relatable");
    assert_eq!(out.y_anchor, "twtt", "physics over cropping");
    let y = out.document.features[0].geometry.positions()[0].y().unwrap();
    assert!((y - 80.0).abs() < 1e-9, "carried through twtt: {y}");
}
