#![no_main]

mod support;

use csgrs::{Real, curve, solid};
use hyperlattice::Point3;
use libfuzzer_sys::fuzz_target;
use synaps_cad::compiler::geometry::Shape;

fn planar_shape(cursor: &mut support::ByteCursor<'_>) -> hypercurve::CurveRegion2 {
    let a = Real::from(cursor.positive());
    let b = Real::from(cursor.positive());
    let segments = cursor.choice(14) + 3;
    let teeth = cursor.choice(6) + 4;
    match cursor.choice(12) {
        0 => curve::rectangle(a, b),
        1 => curve::circle(a, segments),
        2 => curve::ellipse(a, b, segments),
        3 => curve::star(teeth, a.clone() + b.clone(), a),
        4 => curve::heart(a, b, segments),
        5 => curve::egg(a, b, segments),
        6 => curve::teardrop(a.clone(), a + b, segments),
        7 => curve::ring(a, b, segments),
        8 => curve::involute_gear(
            a,
            teeth,
            Real::from(20_u8),
            Real::zero(),
            Real::zero(),
            segments.clamp(2, 5),
        ),
        9 => curve::cycloidal_gear(
            a,
            teeth,
            (Real::from(3_u8) / Real::from(4_u8)).expect("four is nonzero"),
            Real::zero(),
            segments.clamp(2, 5),
        ),
        10 => curve::airfoil_naca4(
            Real::from(2_u8),
            Real::from(4_u8),
            Real::from(12_u8),
            a,
            segments.max(10),
        ),
        _ => curve::supershape(
            a,
            b,
            Real::from(5_u8),
            Real::from(2_u8),
            Real::one(),
            Real::one(),
            segments,
        ),
    }
}

fn solid_shape(cursor: &mut support::ByteCursor<'_>) -> hypermesh::TriangleMesh {
    let a = Real::from(cursor.positive());
    let b = Real::from(cursor.positive());
    let c = Real::from(cursor.positive());
    let segments = cursor.choice(10) + 3;
    let teeth = cursor.choice(6) + 4;
    match cursor.choice(12) {
        0 => solid::cuboid(a, b, c),
        1 => solid::sphere(a, segments, segments),
        2 => solid::cylinder(a, b, segments),
        3 => solid::ellipsoid(a, b, c, segments, segments),
        4 => solid::torus(a.clone() + b.clone() + Real::one(), b, segments, segments),
        5 => solid::octahedron(a),
        6 => solid::icosahedron(a),
        7 => solid::teardrop_cylinder(
            a.clone(),
            a + b,
            c,
            segments,
            &csgrs::GeometryContext::STRICT,
        )
        .map(csgrs::GeometryOutcome::into_value)
        .unwrap_or_else(|_| solid::empty()),
        8 => solid::spur_gear_involute(
            a,
            teeth,
            Real::from(20_u8),
            Real::zero(),
            Real::zero(),
            segments.clamp(2, 5),
            c,
            &csgrs::GeometryContext::STRICT,
        )
        .map(csgrs::GeometryOutcome::into_value)
        .unwrap_or_else(|_| solid::empty()),
        9 => solid::spur_gear_cycloid(
            a,
            teeth,
            (Real::from(3_u8) / Real::from(4_u8)).expect("four is nonzero"),
            Real::zero(),
            segments.clamp(2, 5),
            c,
            &csgrs::GeometryContext::STRICT,
        )
        .map(csgrs::GeometryOutcome::into_value)
        .unwrap_or_else(|_| solid::empty()),
        10 => solid::helical_involute_gear(
            a,
            teeth,
            Real::from(20_u8),
            Real::zero(),
            Real::zero(),
            segments.clamp(2, 5),
            c,
            Real::from(30_u8),
            segments,
            &csgrs::GeometryContext::STRICT,
        )
        .map(csgrs::GeometryOutcome::into_value)
        .unwrap_or_else(|_| solid::empty()),
        _ => {
            let bottom = vec![
                Point3::origin(),
                Point3::new(a.clone(), Real::zero(), Real::zero()),
                Point3::new(a.clone(), b.clone(), Real::zero()),
                Point3::new(Real::zero(), b.clone(), Real::zero()),
            ];
            let top = bottom
                .iter()
                .map(|point| {
                    Point3::new(
                        point.x.clone() + Real::one(),
                        point.y.clone() + Real::one(),
                        c.clone(),
                    )
                })
                .collect::<Vec<_>>();
            solid::loft(&[bottom, top]).unwrap_or_else(|_| solid::empty())
        }
    }
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }
    let mut cursor = support::ByteCursor::new(data);
    let is_planar = cursor.byte() & 1 == 0;
    let shape = if is_planar {
        let region = planar_shape(&mut cursor);
        match cursor.choice(4) {
            0 => Shape::CurveRegion2D(region),
            1 => match curve::try_extrude(
                &region,
                Real::from(cursor.positive()),
                &csgrs::GeometryContext::STRICT,
            ) {
                Ok(outcome) => Shape::from_triangle_mesh(outcome.into_value()),
                Err(error) => Shape::Failed(format!("fuzz extrusion failed: {error}")),
            },
            2 => match curve::extrude_twisted(
                &region,
                Real::from(cursor.positive()),
                Real::from(cursor.integer() * 15),
                [Real::one(), Real::one()],
                cursor.choice(12) + 1,
                &csgrs::GeometryContext::STRICT,
            ) {
                Ok(outcome) => Shape::from_triangle_mesh(outcome.into_value()),
                Err(error) => Shape::Failed(format!("fuzz twisted extrusion failed: {error}")),
            },
            _ => {
                let offset = Real::from(32_u8);
                match curve::try_translated(&region, offset, Real::zero()) {
                    Ok(radial) => {
                        match curve::revolve(
                            &radial,
                            Real::from(360_u16),
                            cursor.choice(12) + 3,
                            &csgrs::GeometryContext::STRICT,
                        ) {
                            Ok(outcome) => Shape::from_triangle_mesh(outcome.into_value()),
                            Err(error) => Shape::Failed(format!("fuzz revolution failed: {error}")),
                        }
                    }
                    Err(error) => Shape::Failed(format!("fuzz translation failed: {error}")),
                }
            }
        }
    } else {
        Shape::from_triangle_mesh(solid_shape(&mut cursor))
    };

    support::validate_shape(&shape);
});
