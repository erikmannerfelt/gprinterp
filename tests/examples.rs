//! The examples in `examples/` are the spec's worked cases, so they double
//! as this crate's regression corpus: if the implementation and the document
//! ever disagree, one of them is wrong and the test says so.

use gprinterp::{Document, Geometry, ValidationWarning};

fn load(name: &str) -> Document {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/");
    let text = std::fs::read_to_string(format!("{path}{name}"))
        .unwrap_or_else(|e| panic!("could not read example {name}: {e}"));
    Document::from_json(&text).unwrap_or_else(|e| panic!("could not parse example {name}: {e}"))
}

#[test]
fn minimal_example_parses_and_is_valid() {
    let document = load("minimal.json");
    assert_eq!(document.key, "line_07_003");
    assert_eq!(document.features.len(), 2);

    let report = gprinterp::validate(&document);
    assert!(report.is_valid(), "unexpected errors: {:?}", report.errors);

    // It declares no axes, so it cannot be re-anchored -- the spec calls
    // this valid but revision-bound (SPEC §7.1).
    assert!(report
        .warnings
        .contains(&ValidationWarning::NotReanchorable));
}

#[test]
fn minimal_example_carries_both_a_line_and_a_point() {
    // SPEC §4.3: all geometry types are storable even though the initial
    // Ridal consumer only interprets lines.
    let document = load("minimal.json");
    assert!(matches!(
        document.features[0].geometry,
        Geometry::LineString(_)
    ));
    assert!(matches!(document.features[1].geometry, Geometry::Point(_)));
}

#[test]
fn recommended_example_is_valid_and_fully_annotated() {
    let document = load("recommended.json");
    let report = gprinterp::validate(&document);
    assert!(report.is_valid(), "unexpected errors: {:?}", report.errors);
    assert!(
        report.warnings.is_empty(),
        "unexpected warnings: {:?}",
        report.warnings
    );
}

#[test]
fn recommended_example_declares_usable_anchors_in_preference_order() {
    let document = load("recommended.json");
    let axes = document
        .coordinates
        .as_ref()
        .unwrap()
        .axes
        .as_ref()
        .unwrap();

    let x_anchors = axes.x.as_ref().unwrap().anchors();
    let names: Vec<&str> = x_anchors.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, vec!["original_trace", "trace_time"]);
    for anchor in x_anchors {
        anchor
            .mapping()
            .unwrap_or_else(|e| panic!("x anchor '{}' is unusable: {e}", anchor.name));
    }

    let y_anchors = axes.y.as_ref().unwrap().anchors();
    assert_eq!(y_anchors.len(), 1);
    assert_eq!(y_anchors[0].name, "twtt");
    let twtt = y_anchors[0].mapping().unwrap();
    // t0 = -12.0, dt = 0.4: sample 30 is the zero crossing.
    assert_eq!(twtt.eval(0.0), -12.0);
    assert_eq!(twtt.eval(30.0), 0.0);
}

#[test]
fn recommended_example_groups_two_lines_into_one_layer() {
    // SPEC §4.6: one Feature per physically separate line, sharing a label.
    let document = load("recommended.json");
    let layers = document.layers();
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].0, Some("bed"));
    assert_eq!(layers[0].1.len(), 2);
    assert_eq!(layers[0].1[0].id(), Some("f-0001"));
    assert_eq!(layers[0].1[1].id(), Some("f-0002"));
}

#[test]
fn every_example_round_trips_byte_stable_through_a_second_pass() {
    // The first pass may reformat (key order is preserved, but whitespace
    // and canonical form are this crate's). What must not change is the
    // content, so a second pass over our own output has to be a fixed point.
    for name in ["minimal.json", "recommended.json"] {
        let document = load(name);
        let once = document.to_json_pretty().unwrap();
        let twice = Document::from_json(&once)
            .unwrap()
            .to_json_pretty()
            .unwrap();
        assert_eq!(once, twice, "{name} is not a round-trip fixed point");
    }
}
