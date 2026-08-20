use csgrs::TriangleMesh;

use crate::compiler::types::MeshData;

/// Converts exact native triangle geometry at the renderer's finite boundary.
///
/// # Errors
///
/// Returns an error when the mesh is empty or an exact coordinate or normal
/// cannot be represented as a finite `f32`.
pub fn triangle_mesh_to_mesh_data(mesh: &TriangleMesh) -> Result<MeshData, String> {
    let valid = mesh
        .has_unique_nondegenerate_triangles(&crate::compiler::MESH_CONTEXT)
        .map_err(|error| format!("exact triangle validation failed: {error}"))?;
    if !valid.value {
        return Err("mesh contains duplicate or degenerate exact triangles".into());
    }
    let vertex_capacity = mesh
        .triangles
        .len()
        .checked_mul(3)
        .ok_or_else(|| "finite mesh projection exceeds addressable memory".to_owned())?;
    let mut positions = Vec::with_capacity(vertex_capacity);
    let mut normals = Vec::with_capacity(vertex_capacity);
    let mut indices = Vec::with_capacity(vertex_capacity);
    for (triangle_index, triangle) in mesh.triangles.iter().enumerate() {
        let finite_position = |index: usize| -> Result<[f32; 3], String> {
            mesh.positions
                .get(index)
                .ok_or_else(|| {
                    format!("triangle {triangle_index} references missing position {index}")
                })?
                .to_f32_array_lossy()
                .ok_or_else(|| {
                    format!("triangle {triangle_index} has an unrepresentable finite position")
                })
        };
        let [a, b, c] = triangle.indices().map(finite_position);
        let [a, b, c] = [a?, b?, c?];
        let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let cross = [
            ab[1].mul_add(ac[2], -(ab[2] * ac[1])),
            ab[2].mul_add(ac[0], -(ab[0] * ac[2])),
            ab[0].mul_add(ac[1], -(ab[1] * ac[0])),
        ];
        let squared_norm =
            cross[0].mul_add(cross[0], cross[1].mul_add(cross[1], cross[2] * cross[2]));
        if !squared_norm.is_finite() || squared_norm <= 0.0 {
            return Err(format!(
                "triangle {triangle_index} collapses at the finite rendering boundary"
            ));
        }
        let inverse_norm = squared_norm.sqrt().recip();
        let normal = cross.map(|component| component * inverse_norm);
        let base = u32::try_from(positions.len())
            .map_err(|_| "finite mesh projection exceeds u32 indexing".to_owned())?;
        let second = base
            .checked_add(1)
            .ok_or_else(|| "finite mesh projection exceeds u32 indexing".to_owned())?;
        let third = base
            .checked_add(2)
            .ok_or_else(|| "finite mesh projection exceeds u32 indexing".to_owned())?;
        positions.extend([a, b, c]);
        normals.extend([normal; 3]);
        indices.extend([base, second, third]);
    }
    if positions.is_empty() {
        return Err("mesh has no vertices".into());
    }

    // OpenSCAD Z-up -> Bevy Y-up (a proper rotation).
    let positions = positions.into_iter().map(|[x, y, z]| [x, z, -y]).collect();
    let normals = normals.into_iter().map(|[x, y, z]| [x, z, -y]).collect();

    Ok(MeshData {
        positions,
        normals,
        indices,
        color: None,
    })
}

#[cfg(test)]
mod tests {
    use super::triangle_mesh_to_mesh_data;
    use csgrs::{Real, solid};

    #[test]
    fn concave_faces_are_ear_clipped_instead_of_fanned() {
        let zero = Real::zero();
        let points = [
            [Real::from(0), Real::from(0), zero.clone()],
            [Real::from(2), Real::from(0), zero.clone()],
            [Real::from(1), Real::from(1), zero.clone()],
            [Real::from(2), Real::from(2), zero.clone()],
            [Real::from(0), Real::from(2), zero],
        ];
        let face = [0, 1, 2, 3, 4];
        let mesh = solid::polyhedron(&points, &[&face]).unwrap();

        let rendered = triangle_mesh_to_mesh_data(&mesh).unwrap();

        assert_eq!(rendered.positions.len(), 9);
        for triangle in rendered.positions.chunks_exact(3) {
            let [a, b, c] = triangle else { unreachable!() };
            let area = (b[2] - a[2]).mul_add(-(c[0] - a[0]), (b[0] - a[0]) * (c[2] - a[2]));
            assert!(area.abs() > f32::EPSILON);
        }
    }

    #[test]
    fn renderer_boundary_rejects_duplicate_exact_triangle_geometry() {
        let cube = solid::cube(Real::one());
        let mut triangles = cube.triangles.to_vec();
        triangles.push(triangles[0]);
        let duplicate = csgrs::TriangleMesh::new(cube.positions.to_vec(), triangles);

        assert!(
            triangle_mesh_to_mesh_data(&duplicate)
                .unwrap_err()
                .contains("duplicate or degenerate")
        );
    }
}
