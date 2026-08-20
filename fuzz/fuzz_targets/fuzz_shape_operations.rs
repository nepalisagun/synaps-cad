#![no_main]

mod support;

use csgrs::{Real, curve, solid};
use libfuzzer_sys::fuzz_target;
use synaps_cad::compiler::geometry::Shape;

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }
    let mut cursor = support::ByteCursor::new(data);
    let is_2d = cursor.byte() & 1 == 0;
    let mut left = if is_2d {
        Shape::CurveRegion2D(curve::rectangle(
            cursor.positive_real(),
            cursor.positive_real(),
        ))
    } else {
        Shape::from_triangle_mesh(solid::cuboid(
            cursor.positive_real(),
            cursor.positive_real(),
            cursor.positive_real(),
        ))
    };
    let right = if is_2d {
        match curve::try_translated(
            &curve::circle(cursor.positive_real(), cursor.choice(8) + 3),
            cursor.real(),
            cursor.real(),
        ) {
            Ok(region) => Shape::CurveRegion2D(region),
            Err(error) => Shape::Failed(format!("fuzz translation failed: {error}")),
        }
    } else {
        Shape::from_triangle_mesh(solid::sphere(
            cursor.positive_real(),
            cursor.choice(6) + 3,
            cursor.choice(6) + 3,
        ))
        .translate(cursor.real(), cursor.real(), cursor.real())
    };

    left = match cursor.choice(4) {
        0 => left.union(right),
        1 => left.difference(right),
        2 => left.intersection(right),
        _ => right,
    };
    left = match cursor.choice(5) {
        0 => left.translate(cursor.real(), cursor.real(), cursor.real()),
        1 => left.rotate(
            cursor.real() * Real::from(15),
            cursor.real() * Real::from(15),
            cursor.real() * Real::from(15),
        ),
        2 => left.scale(cursor.real(), cursor.real(), cursor.real()),
        3 => left.mirror(cursor.real(), cursor.real(), cursor.real()),
        _ => left.center(),
    };
    support::validate_shape(&left);
});
