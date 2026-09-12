# gprinterp

A small, permissive JSON format for storing GPR interpretation features (lines/points/polygons) tied to a radargram.
Canonical storage is a list of GeoJSON Feature objects; optional metadata describes the source radargram and its axis mappings.

An interpretation is authored against one processed version of a radargram, but the
reflector it describes belongs to the ground rather than to that processing run. The
format therefore carries axis metadata that lets a consumer **re-anchor** stored
coordinates onto a differently processed version of the same radargram — see
[`SPEC.md`](SPEC.md) §8.

**THIS IS A DRAFT AND IS NOT YET READY**

- [`SPEC.md`](SPEC.md) — the format specification
- [`examples/minimal.json`](examples/minimal.json) — the smallest valid document
- [`examples/recommended.json`](examples/recommended.json) — a re-anchorable document
