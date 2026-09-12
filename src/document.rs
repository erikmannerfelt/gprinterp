//! The gprinterp document and its parts (SPEC §3, §4, §6, §7).
//!
//! Every struct here carries an `extra` map flattened into it. SPEC §3.3
//! requires readers to preserve unknown fields at all levels, so this is
//! load-bearing rather than defensive: a producer that adds a field this
//! version does not model must still find it intact after a read/write
//! cycle. `serde_json`'s `preserve_order` feature keeps key order stable
//! too, so a rewrite produces a minimal diff.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use crate::geometry::Geometry;
use crate::mapping::{Mapping, MappingError, Tiepoint};

/// A gprinterp document (SPEC §3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    /// Uniquely identifies the target radargram within the producer's scope.
    pub key: String,

    /// Always stored canonically as an array (SPEC §4.1), even when the
    /// input used the permitted FeatureCollection form (SPEC §5.2).
    #[serde(deserialize_with = "deserialize_features")]
    pub features: Vec<Feature>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_modified: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coordinates: Option<Coordinates>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Map<String, Value>>,

    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Accept both the canonical array and the FeatureCollection object
/// (SPEC §5.2), normalizing to the array.
fn deserialize_features<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Feature>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Repr {
        Array(Vec<Feature>),
        Collection { features: Vec<Feature> },
    }
    Ok(match Repr::deserialize(deserializer)? {
        Repr::Array(v) => v,
        Repr::Collection { features } => features,
    })
}

impl Document {
    /// Parse a document from JSON.
    pub fn from_json(text: &str) -> Result<Document, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// Serialize canonically (SPEC §5.2: writers emit the array form).
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Features grouped by layer name, in first-appearance order.
    ///
    /// `properties.label` is the layer (SPEC §4.4); several features sharing
    /// a label are separate lines of one layer (SPEC §4.6). The position
    /// within each group is the line index a level-2 export reports, derived
    /// here rather than stored, since array position is not identity
    /// (SPEC §4.5).
    pub fn layers(&self) -> Vec<(Option<&str>, Vec<&Feature>)> {
        let mut order: Vec<Option<&str>> = Vec::new();
        let mut groups: Vec<Vec<&Feature>> = Vec::new();
        for feature in &self.features {
            let label = feature.label();
            match order.iter().position(|l| *l == label) {
                Some(i) => groups[i].push(feature),
                None => {
                    order.push(label);
                    groups.push(vec![feature]);
                }
            }
        }
        order.into_iter().zip(groups).collect()
    }
}

/// A GeoJSON Feature (SPEC §4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Feature {
    #[serde(rename = "type")]
    pub type_: String,
    pub geometry: Geometry,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub properties: Option<Map<String, Value>>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Feature {
    /// `properties.label`: the layer name (SPEC §4.4).
    pub fn label(&self) -> Option<&str> {
        self.properties.as_ref()?.get("label")?.as_str()
    }

    /// `properties.id`: stable identity across edits (SPEC §4.5).
    pub fn id(&self) -> Option<&str> {
        self.properties.as_ref()?.get("id")?.as_str()
    }
}

/// Radargram linkage (SPEC §6).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Source {
    /// The conceptual radargram; stable across reprocessing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The specific processed revision; changes on every reprocess.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n_traces: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n_samples: Option<u64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Coordinate semantics and mappings (SPEC §7).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Coordinates {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub convention: Option<Convention>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axes: Option<Axes>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Indexing conventions (SPEC §7.3). The permitted values are fixed; see
/// [`crate::validate`], which rejects anything else rather than
/// reinterpreting it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Convention {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indexing: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pixel_reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis_directions: Option<Map<String, Value>>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Axes {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<Axis>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<Axis>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One axis: the units the geometry is stored in, plus any anchor mappings
/// (SPEC §7.4).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Axis {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<PrimaryAxis>,
    /// Anchor mappings, in the producer's preference order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchor: Vec<AnchorAxis>,
    /// The pre-0.1 name for `anchor`, accepted on read (SPEC §7.4) and
    /// re-emitted only if it was present, so an old document round-trips
    /// unchanged rather than being silently rewritten.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fallback: Vec<AnchorAxis>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Axis {
    /// Anchor mappings from `anchor`, falling back to the deprecated
    /// `fallback` key when `anchor` is absent.
    pub fn anchors(&self) -> &[AnchorAxis] {
        if self.anchor.is_empty() {
            &self.fallback
        } else {
            &self.anchor
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PrimaryAxis {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// An anchor axis: a named, invariant quantity plus the mapping from index
/// to it (SPEC §7.4.2).
///
/// The mapping's fields sit alongside `name`/`unit`/`synthetic` in the same
/// JSON object, so they are stored flat here and interpreted on demand by
/// [`AnchorAxis::mapping`]. Keeping structural parsing separate from
/// semantic validation is what lets an unusable mapping still round-trip.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AnchorAxis {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    /// `true` when the producer fabricated these values rather than the
    /// instrument recording them (SPEC §8.4).
    #[serde(default, skip_serializing_if = "is_false")]
    pub synthetic: bool,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub t0: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dt: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub points: Option<Vec<Tiepoint>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interpolation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Vec<f64>>,

    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl AnchorAxis {
    /// Interpret and validate the mapping (SPEC §7.5).
    pub fn mapping(&self) -> Result<Mapping, MappingError> {
        Mapping::from_parts(
            self.type_.as_deref(),
            self.t0,
            self.dt,
            self.points.as_deref(),
            self.values.as_deref(),
        )
    }
}
