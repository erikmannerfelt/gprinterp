//! The OpenAPI schemas (`utoipa` feature) must accept what the serde accepts
//! and reject what it rejects. `Geometry` and `Position` are written by hand,
//! so nothing else holds them to the implementation.

#![cfg(feature = "utoipa")]

use gprinterp::Document;
use serde_json::{json, Value};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(components(schemas(Document)))]
struct Spec;

/// A validator for `Document`, resolving the references between schemas
/// within the generated components.
fn validator() -> jsonschema::Validator {
    let spec = serde_json::to_value(Spec::openapi()).unwrap();
    let root = json!({
        "$ref": "#/components/schemas/Document",
        "components": spec["components"],
    });
    jsonschema::draft202012::new(&root).unwrap()
}

fn example(name: &str) -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/");
    serde_json::from_str(&std::fs::read_to_string(format!("{path}{name}")).unwrap()).unwrap()
}

fn assert_valid(validator: &jsonschema::Validator, document: &Value) {
    let errors: Vec<String> = validator
        .iter_errors(document)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
}

/// One LineString feature with `geometry` as given.
fn with_geometry(geometry: Value) -> Value {
    json!({
        "key": "line",
        "features": [{"type": "Feature", "geometry": geometry, "properties": {}}]
    })
}

#[test]
fn the_examples_match_the_schema_as_written_and_as_rewritten() {
    let validator = validator();
    for name in ["minimal.json", "recommended.json"] {
        let written = example(name);
        assert_valid(&validator, &written);
        let rewritten: Value =
            serde_json::to_value(serde_json::from_value::<Document>(written).unwrap()).unwrap();
        assert_valid(&validator, &rewritten);
    }
}

#[test]
fn every_geometry_type_matches_the_schema() {
    let validator = validator();
    let geometries = [
        json!({"type": "Point", "coordinates": [1.0, 2.0]}),
        json!({"type": "MultiPoint", "coordinates": [[1.0, 2.0], [3.0, 4.0]]}),
        json!({"type": "LineString", "coordinates": [[1.0, 2.0], [3.0, 4.0]]}),
        json!({"type": "MultiLineString", "coordinates": [[[1.0, 2.0], [3.0, 4.0]]]}),
        json!({"type": "Polygon", "coordinates": [[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [0.0, 0.0]]]}),
        json!({"type": "MultiPolygon", "coordinates": [[[[0.0, 0.0], [1.0, 0.0], [0.0, 0.0]]]]}),
        json!({"type": "GeometryCollection", "geometries": [{"type": "Point", "coordinates": [1.0, 2.0]}]}),
    ];
    for geometry in geometries {
        let document = with_geometry(geometry);
        serde_json::from_value::<Document>(document.clone()).unwrap();
        assert_valid(&validator, &document);
    }
}

#[test]
fn an_unknown_geometry_and_unknown_fields_are_kept_by_both() {
    // SPEC §3.3 and §4.3: what this version does not model survives.
    let validator = validator();
    let mut document = with_geometry(json!({"type": "Circle", "centre": [1.0, 2.0]}));
    document["producer_note"] = json!("not modelled");
    document["features"][0]["geometry"]["radius"] = json!(3.0);
    serde_json::from_value::<Document>(document.clone()).unwrap();
    assert_valid(&validator, &document);
}

#[test]
fn a_malformed_known_geometry_is_not_taken_for_an_unknown_one() {
    // A known type with bad coordinates must fail the schema rather than
    // pass as an "unknown" geometry. The schema is the stricter of the two
    // here: a one-element position parses, but SPEC §7.1 needs `[x, y]`.
    let validator = validator();
    for geometry in [
        json!({"type": "LineString", "coordinates": "not coordinates"}),
        json!({"type": "LineString"}),
        json!({"type": "Point", "coordinates": [1.0]}),
    ] {
        let document = with_geometry(geometry);
        assert!(!validator.is_valid(&document), "{document}");
    }
    // The serde checks the type, not the length of a position.
    assert!(serde_json::from_value::<Document>(with_geometry(
        json!({"type": "LineString", "coordinates": "not coordinates"})
    ))
    .is_err());
}

#[test]
fn a_document_without_its_required_fields_is_refused() {
    let validator = validator();
    assert!(!validator.is_valid(&json!({"features": []})));
    assert!(!validator.is_valid(&json!({"key": "line"})));
}
