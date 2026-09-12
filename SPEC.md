# gprinterp — GPR Interpretation JSON Format (Draft)

Status: Draft
Format name: gprinterp
Scope: Exchange of interpreted features (picks, horizons, polygons, annotations) for 2D GPR radargrams, with metadata sufficient to re-anchor those features onto a differently processed version of the same radargram.

---

## 1. Goals & non-goals

### 1.1 Goals
- Provide a simple, permissive JSON container for GPR interpretations.
- Make interpretations easy to generate (e.g., from GUIs and ML models).
- **Survive reprocessing.** An interpretation is authored against one processed
  revision of a radargram, but the reflector it describes is a property of the
  ground, not of that revision. The format therefore carries enough axis
  metadata to map stored coordinates onto a different revision of the same
  radargram (§8).
- Allow optional metadata for:
  - validating compatibility with a radargram (`source`)
  - clarifying coordinate semantics and mappings (`coordinates`)
- Preserve unknown keys to support future extensions and custom workflows.

### 1.2 Non-goals (for v0.x)
- Storing radargram amplitude data.
- Defining processing pipelines, gain settings, filters, etc.
- Enforcing a strict controlled vocabulary for feature semantics (`kind`,
  `layer`, etc.).
- 3D/volume interpretations (explicitly out of scope for v0.x).
- Describing *how* a derived quantity such as depth was computed. A consumer
  that needs depth provenance resolves it from the referenced radargram
  revision (§6), not from this document.

---

## 2. Conformance language

The key words MUST, MUST NOT, SHOULD, SHOULD NOT, and MAY are to be interpreted as described in RFC 2119.

---

## 3. Document: top-level object

A gprinterp document is a single JSON object.

### 3.1 Required fields
- `key` MUST be a string uniquely identifying the target radargram within the producer's scope.
- `features` MUST be present (see §4).

### 3.2 Optional fields (recommended)
- `schema` MAY be the string `"gprinterp"`.
- `schema_version` MAY be a string such as `"0.1"`.
- `date_modified` MAY be an ISO-8601 datetime string.
- `source` SHOULD be present when shape validation or re-anchoring is desired (§6).
- `coordinates` SHOULD be present when coordinate semantics need to be explicit/portable (§7), and MUST be present for re-anchoring (§8).
- `meta` MAY be any JSON object containing arbitrary metadata.

### 3.3 Unknown fields (permissive / round-trip)
- Readers MUST preserve unknown fields at all levels.
- Writers SHOULD preserve unknown fields when rewriting a document (unless explicitly configured not to).

---

## 4. `features`: canonical representation

### 4.1 Canonical form

In canonical form, `features` is an array of GeoJSON Feature objects:

```json
"features": [
  { "type": "Feature", "geometry": { ... }, "properties": { ... } },
  ...
]
```

### 4.2 Requirements
- `features` MUST be an array in canonical form.
- Each entry MUST be an object with:
  - `type` MUST equal `"Feature"`.
  - `geometry` MUST be a valid GeoJSON geometry object.
- `properties` SHOULD be present. If present, it MUST be an object.

### 4.3 Geometry types

Any GeoJSON geometry type is permitted: `Point`, `LineString`, `Polygon`,
`MultiPoint`, `MultiLineString`, `MultiPolygon`, and `GeometryCollection`.

A consumer MAY support only a subset. A consumer that encounters a geometry
type it does not support MUST report it explicitly rather than skipping it
silently, and MUST still round-trip it unchanged if it rewrites the document
(§3.3). Restricting *what can be interpreted* is a consumer policy;
restricting *what can be stored* is not.

### 4.4 `properties.label`: the layer name
- `properties.label` SHOULD be present and SHOULD be a string.
- `label` names the **layer** the feature belongs to — the interpreted
  horizon or class, such as `"bed"` or `"internal_reflector"`. Several
  features MAY share a label; they are then several physically separate
  members of one layer.
- If `properties` is absent, or `label` is missing, consumers SHOULD warn.
- Consumers MAY synthesize a label (e.g. `"unlabeled:<index>"`) for display or indexing.

### 4.5 `properties.id`: stable feature identity
- `properties.id` SHOULD be present and SHOULD be a string that is unique
  within the document.
- It identifies one feature across edits. Position within the `features`
  array is not a stable identifier: inserting or deleting a feature
  renumbers everything after it.
- Producers that assign a positional index to features on export (for
  example, "which separate line within this layer") SHOULD derive that index
  at export time and SHOULD NOT treat it as identity.

### 4.6 One feature per connected geometry

Producers SHOULD emit one Feature per physically separate picked line, each
carrying the layer name in `label`, rather than one Feature per layer holding
a `MultiLineString`. This keeps an individual line independently editable and
deletable, and keeps `properties.id` meaningful. Readers MUST nevertheless
accept the multi-part forms (§4.3).

### 4.7 Other feature properties

Any other keys inside `properties` (including `kind`, `layer`, `confidence`,
etc.) MAY be present and are not standardized by v0.x.

---

## 5. GeoJSON compatibility and export

A gprinterp document is not itself a GeoJSON document. However, the `features` array is designed to be directly usable as GeoJSON features.

Note that in the default coordinate space (§7.1) the coordinates are **array
indices, not geographic coordinates**. The resulting FeatureCollection is
structurally valid GeoJSON but is not georeferenced, and MUST NOT be
presented as though it were.

### 5.1 Export to GeoJSON FeatureCollection

A GeoJSON FeatureCollection can be constructed as:

```json
{ "type": "FeatureCollection", "features": <document.features> }
```

### 5.2 Reader permissiveness (FeatureCollection accepted on input)

Readers MAY also accept an input form where the document's `features` value is a GeoJSON FeatureCollection object:

```json
"features": { "type": "FeatureCollection", "features": [ ... ] }
```

If this form is encountered, readers SHOULD normalize it to the canonical array form internally:
`doc.features = doc.features.features`

Writers SHOULD emit the canonical array form by default.

---

## 6. `source`: radargram linkage, shape validation, and revision identity

`source` is an optional object describing the target radargram sufficiently to validate overlay alignment and to detect reprocessing.

### 6.1 Recommended fields
- `source.id` MAY be a string identifying the radargram (e.g., the same value as `key`). It denotes the *conceptual* radargram and SHOULD be stable across reprocessing.
- `source.revision_id` SHOULD be a string identifying the specific processed revision the interpretation was authored against. Unlike `source.id`, it is expected to change whenever the radargram is reprocessed.
- `source.n_traces` SHOULD be an integer when available.
- `source.n_samples` SHOULD be an integer when available.

### 6.2 Validation use

If `source.n_traces` and `source.n_samples` are present and index semantics apply (default or explicit), validators SHOULD check that all feature coordinates lie within:
- `0 <= x < n_traces`
- `0 <= y < n_samples`

(See §7.1 for coordinate defaults if `coordinates` is absent.)

### 6.3 Revision use

`revision_id` is what lets a consumer tell the three cases apart:

| Condition | Meaning | Consumer action |
|---|---|---|
| `revision_id` matches the radargram being opened | Same processing; indices are exact | Use coordinates directly |
| `revision_id` differs, shape identical | Reprocessed; indices may still be wrong | Re-anchor (§8), warn |
| `revision_id` differs, shape differs | Reprocessed with changed geometry | Re-anchor (§8), warn |

Identical `n_traces`/`n_samples` is **not** evidence that two revisions agree
on what a given index means. A filter can change sample values, or crop and
pad, without changing the shape. Consumers MUST NOT treat a shape match as a
substitute for a revision match.

---

## 7. `coordinates`: coordinate semantics and mappings

The `coordinates` object states what the geometry coordinates mean and provides mappings to physical axes.

### 7.1 Default semantics when `coordinates` is absent

If `coordinates` is absent, consumers SHOULD assume:
- Space: 2D index space (`index2d`)
- `x`: trace index (zero-based)
- `y`: sample index (zero-based)
- Origin: upper-left
- Direction: `x` increases to the right, `y` increases downward
- Pixel reference: coordinates refer to pixel centers

Non-integer coordinates are allowed in index space. Consumers MUST accept float coordinates (e.g., sub-pixel picks), and bounds checking SHOULD be applied to the numeric values.

A document without `coordinates` cannot be re-anchored (§8). It is valid, but
it is only meaningful against the exact revision it was authored on.

### 7.2 Structure

When present, `coordinates` SHOULD be an object with:
- `space` MAY be a string. Recommended: `"index2d"`.
- `convention` MAY be an object (§7.3).
- `axes` MAY be an object describing primary and anchor axes (§7.4).

### 7.3 Convention

The conventions listed in §7.1 are normative defaults for `space: "index2d"`.
A `convention` object MAY be present to state them explicitly:

```json
"convention": {
  "origin": "upper-left",
  "indexing": "zero-based",
  "pixel_reference": "center",
  "axis_directions": { "x": "right", "y": "down" }
}
```

For v0.x these are the only permitted values. A document stating anything
else MUST be rejected rather than reinterpreted — silently accepting an
unsupported origin would flip an interpretation vertically.

### 7.4 Axes model: primary and anchor

`coordinates.axes` may contain `x` and `y`. Each axis may define:
- `primary`: the units the stored geometry is expressed in. For `index2d`
  this is always the index axis.
- `anchor`: an array of mappings from the primary index to a quantity that
  is invariant under reprocessing, used for re-anchoring (§8).

In v0.x, geometries are always expressed in the primary axis units (index).
Anchor mappings never change what the stored coordinates mean; they only
allow a consumer to translate them onto another revision.

> **Naming note.** Earlier drafts called this `fallback`. `anchor` is used
> here because these mappings are the mechanism by which an interpretation is
> tied to physical reality, not a degraded alternative to the primary axis.
> Readers SHOULD accept `fallback` as a deprecated alias.

#### 7.4.1 Primary axes
Typical `index2d` primary axes:
- `{ "name": "trace_index", "unit": "index" }`
- `{ "name": "sample_index", "unit": "index" }`

#### 7.4.2 Anchor axes

Each anchor entry MUST include:
- `name` — see §8.2 for the recognized anchor axis names
- `unit` (e.g. `"index"`, `"s"`, `"ns"`, `"m"`)
- `type` — the mapping type (§7.5)

Each anchor entry MAY include:
- `synthetic` — boolean, default `false`. `true` means the values were
  fabricated by the producer rather than recorded by the instrument
  (§8.4).

### 7.5 Mapping types

A mapping used for re-anchoring MUST be invertible: strictly monotone
increasing in index over the full index range. This is a real constraint,
not a formality — re-anchoring evaluates the mapping forwards on one
revision and backwards on another (§8.1). Validators SHOULD check it.

#### 7.5.1 `type: "regular"`
A regular grid: `value = t0 + index * dt`.

Fields:
- `t0` MUST be a number
- `dt` MUST be a number strictly greater than zero

Example (y TWTT):
```json
{ "name": "twtt", "unit": "ns", "type": "regular", "t0": -12.0, "dt": 0.4 }
```

`t0` is load-bearing: it is what makes two revisions with different
time-zero corrections comparable. A producer that has cropped leading
samples MUST report the resulting offset in `t0` rather than reporting
`t0: 0` against a shifted origin.

#### 7.5.2 `type: "tiepoints"`
A sparse mapping defined by tiepoints and an interpolation method.

Fields:
- `points` MUST be an array of objects each containing:
  - `trace` (number, index along the primary axis)
  - `x` (number, value in the target unit)
- `interpolation` MAY be `"linear"` (the default, and the only value defined in v0.x)

`points` MUST be strictly increasing in both `trace` and `x`. Consumers
SHOULD extrapolate outside the tiepoint range using the first and last
segments, and SHOULD flag coordinates resolved by extrapolation.

Example (acquisition time):
```json
{ "name": "trace_time", "unit": "s", "type": "tiepoints", "interpolation": "linear",
  "points": [ { "trace": 0, "x": 1677501559.0 }, { "trace": 1199, "x": 1677502104.5 } ] }
```

#### 7.5.3 `type: "explicit"`
An explicit per-index array.

Fields:
- `values` MUST be an array of numbers, strictly increasing, with one entry
  per index along the axis.

This is exact but verbose. Producers SHOULD prefer `regular` or `tiepoints`
where either describes the axis adequately.

---

## 8. Re-anchoring (normative)

This section defines how an interpretation authored against one processed
revision is applied to another revision of the same radargram. It is the
central mechanism of the format.

### 8.1 Procedure

Given a document authored against revision **A**, applied to revision **B**
of the same radargram (`source.id` equal):

1. Select an anchor axis name present in **both** A's `coordinates.axes.x.anchor` and B's corresponding axis metadata, using the preference order in §8.2. Repeat independently for `y`.
2. For each stored coordinate `x_a`, evaluate `v = A_x(x_a)` using A's mapping.
3. Invert B's mapping to obtain `x_b = B_x⁻¹(v)`.
4. Repeat for `y`.
5. The re-anchored coordinate is `(x_b, y_b)`.

If no shared anchor axis exists for an axis, the consumer MUST NOT silently
fall back to using the raw index. It MUST either refuse, or proceed only
after reporting that the axis could not be re-anchored.

If `v` falls outside B's range, the coordinate lies outside revision B (for
example, it was picked in a region B subsets away). Consumers SHOULD drop
such coordinates and report how many were dropped, rather than clamping them
to the edge.

### 8.2 Recognized anchor axes

For `x`, in decreasing order of preference:

| `name` | Unit | Invariant under | Not available / not valid when |
|---|---|---|---|
| `original_trace` | `index` | Any operation that selects or reorders whole traces: subsetting, trace removal, cropping | The traces were resampled, stacked, or interpolated onto a new grid; or the radargram concatenates several acquisition files without a globally monotone renumbering |
| `trace_time` | `s` (epoch seconds) | All of the above, plus resampling and stacking, which interpolate time meaningfully | The instrument recorded no timing |

For `y`:

| `name` | Unit | Notes |
|---|---|---|
| `twtt` | `ns` | Two-way travel time. See §8.3 for its limits. |

Producers SHOULD emit at least one `x` anchor axis and SHOULD emit `twtt` for
`y`. Producers that can emit both `x` anchors SHOULD do so; the two degrade
in different circumstances, and a consumer picks whichever both revisions
share.

`original_trace` is preferred where valid because it is an exact integer
identity rather than an interpolated physical quantity, and because it is
available for archival data that carries no timing at all. It is the weaker
choice precisely where trace identity stops being meaningful — after
resampling — which is where `trace_time` remains well defined.

### 8.3 Limits of `twtt` as a `y` anchor

Two processing operations change what a sample index means in ways `twtt`
alone does not fully capture:

- **Time-zero correction** crops leading samples. Two revisions that crop
  differently and both report `t0: 0` will re-anchor incorrectly. A
  conforming producer avoids this by reporting the true offset in `t0`
  (§7.5.1), but a producer that does not track it cannot.
- **Antenna-separation correction** resamples the data onto a uniform
  *depth* grid. After it, `twtt` denotes vertical-equivalent travel time
  rather than recorded travel time. A revision with the correction and one
  without do not share a `y` anchor axis at all, even though both label it
  `twtt`.

Consumers SHOULD warn whenever `y` is re-anchored across differing
`revision_id`s, and MUST NOT present a re-anchored `y` as exact.

### 8.4 Synthetic anchor values

A producer MAY fabricate an anchor axis that the instrument did not record —
most commonly a `trace_time` synthesized as `t = index * constant` for data
with no timing, which makes it equivalent to `original_trace`.

Such an axis MUST be marked `"synthetic": true`. An unmarked fabricated axis
is indistinguishable from a recorded one and will be trusted across
revisions where it carries no information. Consumers MUST NOT re-anchor
across revisions using a synthetic axis unless the same synthesis rule
demonstrably applies to both.

---

## 9. Validation philosophy (permissive)

### 9.1 Minimal validation (core)

A core validator MUST reject a document only if:
- `key` is missing or not a string
- `features` is missing or not an array (after optional normalization in §5.2)
- any feature entry is missing `type: "Feature"` or `geometry`
- `coordinates.convention` is present and states a value not permitted by §7.3

A core validator SHOULD warn if:
- `properties` is absent
- `properties.label` is absent or not a string
- `properties.id` is absent

### 9.2 Recommended validation

If `source.n_traces`/`n_samples` exist and index semantics apply, validators SHOULD check coordinate bounds and report out-of-bounds coordinates as warnings or errors depending on consumer policy.

If anchor mappings are present, validators SHOULD check that each is strictly
monotone (§7.5), that `dt > 0`, and that `tiepoints` are strictly increasing
in both fields.

---

## 10. Examples

### 10.1 Core example (minimal)

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

### 10.2 Recommended example (re-anchorable)

```json
{
  "schema": "gprinterp",
  "schema_version": "0.1",
  "key": "line_07_003",
  "date_modified": "2026-02-27T16:19:19+01:00",
  "source": {
    "id": "line_07_003",
    "revision_id": "9f2c1ab4e7d05836",
    "n_traces": 1200,
    "n_samples": 2048
  },
  "coordinates": {
    "space": "index2d",
    "convention": {
      "origin": "upper-left",
      "indexing": "zero-based",
      "pixel_reference": "center",
      "axis_directions": { "x": "right", "y": "down" }
    },
    "axes": {
      "x": {
        "primary": { "name": "trace_index", "unit": "index" },
        "anchor": [
          { "name": "original_trace", "unit": "index", "type": "tiepoints",
            "points": [ { "trace": 0, "x": 340 }, { "trace": 1199, "x": 1539 } ] },
          { "name": "trace_time", "unit": "s", "type": "tiepoints",
            "points": [ { "trace": 0, "x": 1677501559.0 }, { "trace": 1199, "x": 1677502104.5 } ] }
        ]
      },
      "y": {
        "primary": { "name": "sample_index", "unit": "index" },
        "anchor": [
          { "name": "twtt", "unit": "ns", "type": "regular", "t0": -12.0, "dt": 0.4 }
        ]
      }
    }
  },
  "features": [
    {
      "type": "Feature",
      "geometry": { "type": "LineString", "coordinates": [[10.5, 200.0], [200.25, 210.75]] },
      "properties": { "id": "f-0001", "label": "bed", "kind": "horizon" }
    },
    {
      "type": "Feature",
      "geometry": { "type": "LineString", "coordinates": [[620.0, 244.0], [880.0, 251.5]] },
      "properties": { "id": "f-0002", "label": "bed", "kind": "horizon" }
    }
  ]
}
```

Both features carry `label: "bed"`: they are two physically separate lines
of the same interpreted layer (§4.4, §4.6).

---

## 11. Forward compatibility

- New fields may be added at any time; readers MUST preserve unknown fields.
- Unknown anchor axis names and unknown mapping types SHOULD be preserved,
  and SHOULD emit a warning if a consumer needed them for computation.
- Consumers should rely on `schema_version` when strict behavior is needed, but MUST NOT reject documents solely because `schema_version` is missing in v0.x.

### 11.1 Removed from earlier drafts

- **`gaps` on `regular` mappings** (compact encoding of time-triggered
  acquisition pauses). A producer with real per-trace times expresses them
  with `tiepoints` instead. The encoding added a second, subtler way to get
  the most correctness-critical axis wrong.
- **External array references** (`"values": "external:..."` on `explicit`).
  Out of scope for a single-file JSON exchange format.

---

## Notes for implementers (non-normative)

- Prefer "accept broadly, write canonically": accept FeatureCollection inputs if encountered, but write `features` as a Feature array.
- Keep semantic validation separate from structural parsing.
- Round-tripping is a hard requirement, not a nicety. An implementation that
  drops geometry types or axis metadata it does not itself use will quietly
  destroy other tools' data.
- When in doubt about a re-anchoring result, say so. A pick that is silently
  20 samples off is worse than one that refused to load.
