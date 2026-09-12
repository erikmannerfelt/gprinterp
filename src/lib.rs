//! Reference implementation of the [gprinterp] GPR interpretation exchange
//! format.
//!
//! gprinterp stores interpreted features (picks, horizons, polygons) for a 2D
//! radargram as GeoJSON features in **trace/sample index space**, together
//! with the axis metadata needed to move them onto a differently processed
//! version of the same radargram. See `SPEC.md` in this repository; every
//! type here cites the section it implements.
//!
//! # The two things this crate is careful about
//!
//! **Round-tripping.** Every struct keeps an `extra` map of fields it does
//! not model, and unmodelled geometry types survive as
//! [`Geometry::Unknown`]. A consumer that dropped what it did not understand
//! would destroy another tool's data on the next write, so this is a
//! requirement of the format (SPEC §3.3), not an implementation nicety.
//!
//! **Re-anchoring, or refusing to.** [`reanchor`] never falls back to using
//! raw indices when it cannot find a shared invariant axis, because indices
//! from two different processing runs are not comparable and the failure
//! looks exactly like success. It returns an error instead.
//!
//! # Example
//!
//! ```
//! use gprinterp::Document;
//!
//! let document = Document::from_json(r#"{
//!     "key": "line_07_003",
//!     "features": [{
//!         "type": "Feature",
//!         "geometry": {"type": "LineString", "coordinates": [[10.5, 200.0], [200.25, 210.75]]},
//!         "properties": {"id": "f-0001", "label": "bed"}
//!     }]
//! }"#).unwrap();
//!
//! assert_eq!(document.key, "line_07_003");
//! let layers = document.layers();
//! assert_eq!(layers.len(), 1);
//! assert_eq!(layers[0].0, Some("bed"));
//! ```
//!
//! [gprinterp]: https://github.com/erikmannerfelt/gprinterp

pub mod document;
pub mod geometry;
pub mod mapping;
pub mod reanchor;
pub mod validate;

pub use document::{
    AnchorAxis, Axes, Axis, Convention, Coordinates, Document, Feature, PrimaryAxis, Source,
};
pub use geometry::{Geometry, Position};
pub use mapping::{Mapping, MappingError, Tiepoint};
pub use reanchor::{reanchor, ReanchorError, ReanchorOutcome, RevisionAxes};
pub use validate::{validate, Report, ValidationError, ValidationWarning};

/// The `schema` string a conforming document uses.
pub const SCHEMA: &str = "gprinterp";

/// The specification version this crate implements.
pub const SCHEMA_VERSION: &str = "0.1";
