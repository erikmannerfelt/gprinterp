//! GeoJSON geometry objects, in the index space the spec defines.
//!
//! Every geometry type in RFC 7946 is representable, plus an
//! [`Geometry::Unknown`] catch-all. That catch-all is not a convenience: SPEC
//! §3.3 and §4.3 require a reader to round-trip what it does not itself
//! understand, and a consumer that quietly dropped an unrecognised geometry
//! would destroy another tool's data on the next write.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

/// One coordinate tuple.
///
/// GeoJSON permits a third element, and the spec neither uses nor forbids
/// one, so the full tuple is preserved even though only `x`/`y` carry
/// meaning in `index2d`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Position(pub Vec<f64>);

impl Position {
    /// Trace index in the default `index2d` space (SPEC §7.1).
    pub fn x(&self) -> Option<f64> {
        self.0.first().copied()
    }

    /// Sample index in the default `index2d` space (SPEC §7.1).
    pub fn y(&self) -> Option<f64> {
        self.0.get(1).copied()
    }

    /// Replace `x`/`y` while preserving any further elements.
    pub fn with_xy(&self, x: f64, y: f64) -> Position {
        let mut out = self.0.clone();
        if out.len() < 2 {
            out.resize(2, 0.0);
        }
        out[0] = x;
        out[1] = y;
        Position(out)
    }
}

/// A GeoJSON geometry.
#[derive(Debug, Clone, PartialEq)]
pub enum Geometry {
    Point(Position),
    MultiPoint(Vec<Position>),
    LineString(Vec<Position>),
    MultiLineString(Vec<Vec<Position>>),
    Polygon(Vec<Vec<Position>>),
    MultiPolygon(Vec<Vec<Vec<Position>>>),
    GeometryCollection(Vec<Geometry>),
    /// A geometry whose `type` this implementation does not model, kept
    /// verbatim so it survives a read/write cycle (SPEC §4.3).
    Unknown(Map<String, Value>),
}

impl Geometry {
    /// The GeoJSON `type` string.
    pub fn type_name(&self) -> &str {
        match self {
            Geometry::Point(_) => "Point",
            Geometry::MultiPoint(_) => "MultiPoint",
            Geometry::LineString(_) => "LineString",
            Geometry::MultiLineString(_) => "MultiLineString",
            Geometry::Polygon(_) => "Polygon",
            Geometry::MultiPolygon(_) => "MultiPolygon",
            Geometry::GeometryCollection(_) => "GeometryCollection",
            Geometry::Unknown(map) => map
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("<untyped>"),
        }
    }

    /// Every position in the geometry, in document order.
    pub fn positions(&self) -> Vec<&Position> {
        let mut out = Vec::new();
        self.collect_positions(&mut out);
        out
    }

    fn collect_positions<'a>(&'a self, out: &mut Vec<&'a Position>) {
        match self {
            Geometry::Point(p) => out.push(p),
            Geometry::MultiPoint(ps) | Geometry::LineString(ps) => out.extend(ps.iter()),
            Geometry::MultiLineString(rings) | Geometry::Polygon(rings) => {
                out.extend(rings.iter().flatten())
            }
            Geometry::MultiPolygon(polys) => out.extend(polys.iter().flatten().flatten()),
            Geometry::GeometryCollection(gs) => {
                for g in gs {
                    g.collect_positions(out);
                }
            }
            Geometry::Unknown(_) => {}
        }
    }

    /// Rebuild the geometry with every position replaced by `f`.
    ///
    /// Used by re-anchoring (SPEC §8), which maps coordinates between
    /// revisions without otherwise disturbing the geometry. `f` returning
    /// `None` for a position propagates as `None` for the whole geometry:
    /// a line with a hole punched in it is not a partial success, and
    /// silently dropping the vertex would change the interpretation's shape.
    pub fn try_map_positions<F>(&self, f: &mut F) -> Option<Geometry>
    where
        F: FnMut(&Position) -> Option<Position>,
    {
        let map_vec = |v: &Vec<Position>, f: &mut F| -> Option<Vec<Position>> {
            v.iter().map(&mut *f).collect()
        };
        Some(match self {
            Geometry::Point(p) => Geometry::Point(f(p)?),
            Geometry::MultiPoint(ps) => Geometry::MultiPoint(map_vec(ps, f)?),
            Geometry::LineString(ps) => Geometry::LineString(map_vec(ps, f)?),
            Geometry::MultiLineString(rings) => Geometry::MultiLineString(
                rings.iter().map(|r| map_vec(r, f)).collect::<Option<_>>()?,
            ),
            Geometry::Polygon(rings) => {
                Geometry::Polygon(rings.iter().map(|r| map_vec(r, f)).collect::<Option<_>>()?)
            }
            Geometry::MultiPolygon(polys) => Geometry::MultiPolygon(
                polys
                    .iter()
                    .map(|poly| poly.iter().map(|r| map_vec(r, f)).collect::<Option<_>>())
                    .collect::<Option<_>>()?,
            ),
            Geometry::GeometryCollection(gs) => Geometry::GeometryCollection(
                gs.iter()
                    .map(|g| g.try_map_positions(f))
                    .collect::<Option<_>>()?,
            ),
            Geometry::Unknown(m) => Geometry::Unknown(m.clone()),
        })
    }
}

impl<'de> Deserialize<'de> for Geometry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Routed through `Value` rather than an internally tagged enum
        // because serde cannot express "unknown variant, keep the original
        // object", which SPEC §4.3 requires.
        let value = Value::deserialize(deserializer)?;
        let Value::Object(map) = value else {
            return Err(D::Error::custom("geometry must be a JSON object"));
        };
        let Some(type_name) = map.get("type").and_then(Value::as_str) else {
            return Ok(Geometry::Unknown(map));
        };

        macro_rules! coords {
            ($t:ty) => {{
                let Some(raw) = map.get("coordinates") else {
                    return Err(D::Error::custom(format!(
                        "{type_name} geometry has no 'coordinates'"
                    )));
                };
                serde_json::from_value::<$t>(raw.clone()).map_err(|e| {
                    D::Error::custom(format!("invalid coordinates for {type_name}: {e}"))
                })?
            }};
        }

        Ok(match type_name {
            "Point" => Geometry::Point(coords!(Position)),
            "MultiPoint" => Geometry::MultiPoint(coords!(Vec<Position>)),
            "LineString" => Geometry::LineString(coords!(Vec<Position>)),
            "MultiLineString" => Geometry::MultiLineString(coords!(Vec<Vec<Position>>)),
            "Polygon" => Geometry::Polygon(coords!(Vec<Vec<Position>>)),
            "MultiPolygon" => Geometry::MultiPolygon(coords!(Vec<Vec<Vec<Position>>>)),
            "GeometryCollection" => {
                let Some(raw) = map.get("geometries") else {
                    return Err(D::Error::custom(
                        "GeometryCollection has no 'geometries'".to_string(),
                    ));
                };
                Geometry::GeometryCollection(
                    serde_json::from_value::<Vec<Geometry>>(raw.clone())
                        .map_err(|e| D::Error::custom(format!("invalid geometries: {e}")))?,
                )
            }
            _ => Geometry::Unknown(map),
        })
    }
}

impl Serialize for Geometry {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Geometry::Unknown(map) = self {
            return map.serialize(serializer);
        }
        let mut out = Map::new();
        out.insert("type".into(), Value::String(self.type_name().into()));
        let body = match self {
            Geometry::Point(p) => serde_json::to_value(p),
            Geometry::MultiPoint(ps) | Geometry::LineString(ps) => serde_json::to_value(ps),
            Geometry::MultiLineString(r) | Geometry::Polygon(r) => serde_json::to_value(r),
            Geometry::MultiPolygon(p) => serde_json::to_value(p),
            Geometry::GeometryCollection(g) => serde_json::to_value(g),
            Geometry::Unknown(_) => unreachable!("handled above"),
        }
        .map_err(serde::ser::Error::custom)?;
        let key = if matches!(self, Geometry::GeometryCollection(_)) {
            "geometries"
        } else {
            "coordinates"
        };
        out.insert(key.into(), body);
        out.serialize(serializer)
    }
}

/// Hand-written schemas, because the serde they describe is hand-written.
#[cfg(feature = "utoipa")]
mod schema {
    use serde_json::json;
    use utoipa::openapi::{schema::Schema, RefOr};
    use utoipa::{PartialSchema, ToSchema};

    use super::{Geometry, Position};

    /// The geometry types modelled here, as `type` spells them.
    const KNOWN: [&str; 7] = [
        "Point",
        "MultiPoint",
        "LineString",
        "MultiLineString",
        "Polygon",
        "MultiPolygon",
        "GeometryCollection",
    ];

    fn schema_from(value: serde_json::Value) -> RefOr<Schema> {
        RefOr::T(serde_json::from_value(value).expect("a hand-written schema is valid"))
    }

    impl PartialSchema for Position {
        fn schema() -> RefOr<Schema> {
            schema_from(json!({
                "type": "array",
                "items": {"type": "number"},
                "minItems": 2,
                "description": "A GeoJSON position: `[x, y]` in index space (SPEC §7.1), \
                                trace index then sample index. Further elements are kept."
            }))
        }
    }

    impl ToSchema for Position {}

    impl PartialSchema for Geometry {
        fn schema() -> RefOr<Schema> {
            let position = json!({"$ref": "#/components/schemas/Position"});
            let array_of = |items: serde_json::Value| json!({"type": "array", "items": items});
            let shape = |name: &str, coordinates: serde_json::Value| {
                json!({
                    "type": "object",
                    "required": ["type", "coordinates"],
                    "properties": {
                        "type": {"type": "string", "enum": [name]},
                        "coordinates": coordinates
                    }
                })
            };
            let line = array_of(position.clone());
            let rings = array_of(line.clone());
            schema_from(json!({
                "description": "A GeoJSON geometry (SPEC §4.3). A geometry of a type not \
                                listed here is kept verbatim rather than rejected.",
                "oneOf": [
                    shape("Point", position.clone()),
                    shape("MultiPoint", line.clone()),
                    shape("LineString", line.clone()),
                    shape("MultiLineString", rings.clone()),
                    shape("Polygon", rings.clone()),
                    shape("MultiPolygon", array_of(rings)),
                    {
                        "type": "object",
                        "required": ["type", "geometries"],
                        "properties": {
                            "type": {"type": "string", "enum": ["GeometryCollection"]},
                            "geometries": array_of(json!({"$ref": "#/components/schemas/Geometry"}))
                        }
                    },
                    {
                        "type": "object",
                        "description": "Any other geometry, kept as it is.",
                        "properties": {
                            "type": {
                                "type": "string",
                                "pattern": format!("^(?!({})$)", KNOWN.join("|"))
                            }
                        }
                    }
                ]
            }))
        }
    }

    impl ToSchema for Geometry {
        fn schemas(schemas: &mut Vec<(String, RefOr<Schema>)>) {
            schemas.push((Position::name().into(), Position::schema()));
        }
    }
}
