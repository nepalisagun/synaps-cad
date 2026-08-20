#![no_main]

mod support;

use hyperlattice::Point3;
use hypermesh::{Triangle, TriangleMesh};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let mut cursor = support::ByteCursor::new(data);
    let point_count = cursor.choice(16);
    let triangle_count = cursor.choice(16);
    let positions = (0..point_count)
        .map(|_| Point3::new(cursor.real(), cursor.real(), cursor.real()))
        .collect();
    let index_bound = point_count.saturating_add(4).max(1);
    let triangles = (0..triangle_count)
        .map(|_| {
            Triangle::new(
                cursor.choice(index_bound),
                cursor.choice(index_bound),
                cursor.choice(index_bound),
            )
        })
        .collect();
    let mesh = TriangleMesh::new(positions, triangles);
    if let Ok(rendered) =
        synaps_cad::compiler::geometry::conversions::triangle_mesh_to_mesh_data(&mesh)
    {
        support::assert_render_mesh(&rendered);
    }
});
