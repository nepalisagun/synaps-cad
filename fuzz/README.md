# SynapsCAD fuzzing

This unpublished `cargo-fuzz` package separates cheap syntax/evaluator
campaigns from exact geometry and preview rendering:

- `fuzz_compile_arbitrary` parses and evaluates bounded malformed or valid text;
- `fuzz_compile_structured` covers primitives, transforms, 2D/3D Booleans,
  hulls, offsets, and extrusions;
- `fuzz_expression_control` covers functions, ranges, comprehensions, `let`,
  conditionals, assertions, and built-ins;
- `fuzz_shape_operations` directly exercises exact `Shape` transforms and
  same-dimensional Booleans;
- `fuzz_shape_catalog` drives the CSGRS planar and solid shape libraries
  through extrusion, twisted extrusion, revolution, lofting, and exact
  duplicate/degenerate-triangle validation;
- `fuzz_mesh_conversion` probes renderer-boundary mesh validation;
- `fuzz_text_pipeline` covers fonts, alignment, direction, and extrusion;
- `fuzz_full_pipeline` includes mesh conversion and PNG preview rendering.

List and run bounded campaigns:

```sh
cargo +nightly fuzz list
cargo +nightly fuzz run fuzz_compile_arbitrary -- -max_total_time=60
cargo +nightly fuzz run fuzz_compile_structured -- -timeout=30 -max_total_time=60
cargo +nightly fuzz run fuzz_expression_control -- -max_total_time=60
cargo +nightly fuzz run fuzz_shape_operations -- -timeout=30 -max_total_time=60
cargo +nightly fuzz run fuzz_shape_catalog -- -timeout=30 -max_total_time=60
cargo +nightly fuzz run fuzz_mesh_conversion -- -max_total_time=60
cargo +nightly fuzz run fuzz_text_pipeline -- -max_total_time=60
cargo +nightly fuzz run fuzz_full_pipeline -- -timeout=30 -max_total_time=60
```

The fuzz package enables `fuzz-bounded-campaign`, which caps fragment counts at
32 and range expansion at 256 elements. Production builds retain normal
OpenSCAD behavior. Exact geometry targets also bound syntax depth, primitive
size, and operation count to keep sanitizer throughput useful.
