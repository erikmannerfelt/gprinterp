# gprinterp — GPR Interpretation JSON Format (Draft)

Status: Draft  
Format name: gprinterp  
Scope: Exchange of interpreted features (picks, horizons, polygons, annotations) for 2D GPR radargrams, with optional metadata for axis mapping and validation.

---

## 1. Goals & non-goals

### 1.1 Goals
- Provide a simple, permissive JSON container for GPR interpretations.
- Make interpretations easy to generate (e.g., from GUIs and ML models).
- Allow optional metadata for:
  - validating compatibility with a radargram (source)
  - clarifying coordinate semantics and optional mappings (coordinates)
- Preserve unknown keys to support future extensions and custom workflows.

### 1.2 Non-goals (for v0.x)
- Storing radargram amplitude data.
- Defining processing pipelines, gain settings, filters, etc.
- Enforcing a strict controlled vocabulary for feature semantics (kind, layer, etc.).
- 3D/volume interpretations (explicitly out of scope for v0.x).

---

## 2. Conformance language

The key words MUST, MUST NOT, SHOULD, SHOULD NOT, and MAY are to be interpreted as described in RFC 2119.

---

## 3. Document: top-level object

A gprinterp document is a single JSON object.

### 3.1 Required fields
- key MUST be a string uniquely identifying the target radargram within the producer’s scope.
- features MUST be present (see §4).

### 3.2 Optional fields (recommended)
- schema MAY be the string "gprinterp".
- schema_version MAY be a string such as "0.1".
- date_modified MAY be an ISO-8601 datetime string.
- source SHOULD be present when shape validation is desired (see §6).
- coordinates SHOULD be present when coordinate semantics need to be explicit/portable (see §7).
- meta MAY be any JSON object containing arbitrary metadata.

### 3.3 Unknown fields (permissive / round-trip)
- Readers MUST preserve unknown fields at all levels.
- Writers SHOULD preserve unknown fields when rewriting a document (unless explicitly configured not to).

---

## 4. features: canonical representation

### 4.1 Canonical form

In canonical form, features is an array of GeoJSON Feature objects:

```json
"features": [
  { "type": "Feature", "geometry": { ... }, "properties": { ... } },
  ...
]
```

### 4.2 Requirements
- features MUST be an array in canonical form.
- Each entry MUST be an object with:
  - type MUST equal "Feature".
  - geometry MUST be a valid GeoJSON geometry object.
- properties MAY be absent. If present:
  - properties SHOULD be present (recommended).
  - properties MUST be an object.

### 4.3 properties.label
- properties.label SHOULD be present and SHOULD be a string.
- If properties is absent, or label is missing, consumers SHOULD warn.
- Consumers MAY synthesize a label (e.g., "unlabeled:<index>") for display or indexing.

### 4.4 Other feature properties

Any other keys inside properties (including kind, layer, confidence, etc.) MAY be present and are not standardized by v0.x.

---

## 5. GeoJSON compatibility and export

A gprinterp document is not itself a GeoJSON document. However, the features array is designed to be directly usable as GeoJSON features.

### 5.1 Export to GeoJSON FeatureCollection

A valid GeoJSON FeatureCollection can be constructed as:

```json
{ "type": "FeatureCollection", "features": <document.features> }
```

### 5.2 Reader permissiveness (FeatureCollection accepted on input)

Readers MAY also accept an input form where the document’s features value is a GeoJSON FeatureCollection object:

```json
"features": { "type": "FeatureCollection", "features": [ ... ] }
```

If this form is encountered, readers SHOULD normalize it to the canonical array form internally:
- doc.features = doc.features.features

Writers SHOULD emit the canonical array form by default.

---

## 6. source (optional): radargram linkage and shape validation

source is an optional object describing the target radargram sufficiently to validate overlay alignment.

### 6.1 Recommended fields
- source.id MAY be a string (e.g., same as key or a dataset identifier).
- source.n_traces SHOULD be an integer when available.
- source.n_samples SHOULD be an integer when available.

### 6.2 Validation use

If source.n_traces and source.n_samples are present and index semantics apply (default or explicit), validators SHOULD check that all feature coordinates lie within:
- 0 <= x < n_traces
- 0 <= y < n_samples

(See §7.1 for coordinate defaults if coordinates is absent.)

---

## 7. coordinates (optional): coordinate semantics and mappings

The coordinates object clarifies what the geometry coordinates mean and optionally provides mappings to physical axes (time, distance, etc.).

### 7.1 Default semantics when coordinates is absent

If coordinates is absent, consumers SHOULD assume:
- Space: 2D index space (index2d)
- x: trace index (zero-based)
- y: sample index (zero-based)
- Origin: upper-left
- Direction: x increases to the right, y increases downward
- Pixel reference: coordinates refer to pixel centers

Non-integer coordinates are allowed in index space. Consumers MUST accept float coordinates (e.g., sub-pixel picks), and bounds checking SHOULD be applied to the numeric values.

These assumptions define the legacy/core interpretation of geometry coordinates.

### 7.2 Structure

When present, coordinates SHOULD be an object with:
- space MAY be a string. Recommended: "index2d".
- convention MAY be an object describing indexing/origin conventions.
- axes MAY be an object describing primary and fallback axes.

Example skeleton:
```json
"coordinates": { "space": "index2d", "convention": { ... }, "axes": { ... } }
```

### 7.3 Recommended convention

If convention is present, recommended fields are:
- origin: "upper-left"
- indexing: "zero-based"
- pixel_reference: "center"
- axis_directions: { "x": "right", "y": "down" }

### 7.4 Axes model: primary and fallback

coordinates.axes may contain x and y. Each axis may define:
- primary: recommended description of the geometry axis (usually index-based).
- fallback: optional array of mappings to other units/spaces.

For v0.x, geometries are assumed to be expressed in the primary axis units (index) unless explicitly stated otherwise by a future version.

#### 7.4.1 Primary axes (recommended)
Typical index2d primary axes:
- { "name": "trace_index", "unit": "index" }
- { "name": "sample_index", "unit": "index" }

#### 7.4.2 Fallback axis mappings (optional)
Fallback mappings allow conversion from index coordinates to physical coordinates (and vice versa) without changing stored feature geometries.

Each fallback entry MAY include:
- name (e.g., "twtt", "trace_time", "distance")
- unit (e.g., "ns", "s", "m")
- type describing the mapping (see §7.5)

---

## 7.5 Standard fallback mapping types (v0.x)

To keep documents compact for large datasets, v0.x supports a small set of mapping types. Readers MAY support additional types; unknown types SHOULD be preserved and SHOULD emit a warning if used for computation.

### 7.5.1 type: "regular"
Represents a regular grid:
- For y (TWTT): value = t0 + index * dt
- For x (trace_time): value = t0 + index * dt plus optional gaps (see §7.6)

Fields:
- t0 MUST be a number
- dt MUST be a positive number

Optional:
- gaps MAY be provided for time-triggered acquisition pauses (see §7.6)

Example (y TWTT):
```json
{ "name": "twtt", "unit": "ns", "type": "regular", "t0": -12.0, "dt": 0.4 }
```

### 7.5.2 type: "tiepoints"
Represents a sparse mapping defined by tiepoints and an interpolation method (useful for distance).

Fields:
- points MUST be an array of objects each containing:
  - trace (integer index)
  - x (number in the target unit)
- interpolation MAY be "linear" (recommended default)

Example (distance):
```json
{ "name": "distance", "unit": "m", "type": "tiepoints", "interpolation": "linear",
  "points": [ { "trace": 0, "x": 0.0 }, { "trace": 1200, "x": 152.3 } ] }
```

### 7.5.3 type: "explicit" (optional / extension-friendly)
Represents an explicit per-index array. This is discouraged for very large n_traces in plain JSON, but may be useful via external references.

Fields:
- values MUST be either:
  - a JSON array of numbers, or
  - a string reference (e.g., "external:trace_time.float64")

(External array conventions are out of scope for v0.x, but allowed as user extensions.)

---

## 7.6 Time gaps for time-triggered acquisition (compact encoding)

For time-triggered data with pauses, a regular fallback may include gaps:

```json
{ "name": "trace_time", "unit": "s", "type": "regular", "t0": 0.0, "dt": 0.1,
  "gaps": [ { "before_trace": 25000, "duration": 120.5 } ] }
```

### Semantics (normative)
- A gap entry { before_trace: i, duration: d } inserts an additional time offset of d seconds between traces i-1 and i.
- before_trace MUST be an integer in [0, n_traces] when source.n_traces is known.
- duration MUST be a number >= 0.
- If multiple gaps exist, they are cumulative: all gaps with before_trace <= i contribute to the timestamp of trace i.

Readers SHOULD accept gaps unsorted but SHOULD sort them internally for evaluation.

---

## 8. Validation philosophy (permissive)

### 8.1 Minimal validation (core)

A core validator MUST reject a document only if:
- key is missing or not a string
- features is missing or not an array (after optional normalization in §5.2)
- any feature entry is missing type:"Feature" or geometry

A core validator SHOULD warn if:
- properties is absent
- properties.label is absent or not a string

### 8.2 Recommended validation (when source/coordinates exist)

If source.n_traces/n_samples exist and index semantics apply (default or explicit), validators SHOULD check coordinate bounds and report out-of-bounds coordinates as warnings or errors depending on consumer policy.

If coordinates includes fallback mappings used for computation, validators SHOULD check basic mapping sanity (e.g., dt > 0, gap durations non-negative).

---

## 9. Minimal examples

### 9.1 Core example (minimal)

```json
{
  "key": "line_07_003",
  "features": [
    {
      "type": "Feature",
      "geometry": { "type": "LineString", "coordinates": [[10.5, 200.0], [200.25, 210.75], [500.0, 220.0]] }
    }
  ]
}
```

### 9.2 Recommended example (explicit coordinates + shape)

```json
{
  "schema": "gprinterp",
  "schema_version": "0.1",
  "key": "line_07_003",
  "date_modified": "2026-02-27T16:19:19+01:00",
  "source": { "n_traces": 1200, "n_samples": 2048 },
  "coordinates": {
    "space": "index2d",
    "convention": {
      "origin": "upper-left",
      "indexing": "zero-based",
      "pixel_reference": "center",
      "axis_directions": { "x": "right", "y": "down" }
    },
    "axes": {
      "x": { "primary": { "name": "trace_index", "unit": "index" } },
      "y": {
        "primary": { "name": "sample_index", "unit": "index" },
        "fallback": [
          { "name": "twtt", "unit": "ns", "type": "regular", "t0": -12.0, "dt": 0.4 }
        ]
      }
    }
  },
  "features": [
    {
      "type": "Feature",
      "geometry": { "type": "LineString", "coordinates": [[10.5, 200.0], [200.25, 210.75]] },
      "properties": { "label": "bed" }
    }
  ]
}
```

---

## 10. Forward compatibility

- New fields may be added at any time; readers MUST preserve unknown fields.
- Consumers should rely on schema_version when strict behavior is needed, but MUST NOT reject documents solely because schema_version is missing in v0.x.

---

## Notes for implementers (non-normative)

- Prefer “accept broadly, write canonically”: accept FeatureCollection inputs if encountered, but write features as a Feature array.
- Keep semantic validation separate from structural parsing.
