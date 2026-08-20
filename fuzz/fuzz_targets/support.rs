#![allow(dead_code)]

use csgrs::Real;
use hyperlimit::PredicatePolicy;
use hypermesh::MeshContext;
use synaps_cad::compiler::{
    Evaluator,
    evaluator::Value,
    geometry::{Shape, conversions::triangle_mesh_to_mesh_data},
};

const MESH_CONTEXT: MeshContext = MeshContext::new(PredicatePolicy::STRICT);

pub struct ByteCursor<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> ByteCursor<'a> {
    pub const fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    pub fn byte(&mut self) -> u8 {
        let value = self.data[self.offset % self.data.len()];
        self.offset += 1;
        value
    }

    pub fn choice(&mut self, count: usize) -> usize {
        usize::from(self.byte()) % count
    }

    pub fn integer(&mut self) -> i16 {
        i16::from(self.byte() % 17) - 8
    }

    pub fn positive(&mut self) -> u8 {
        self.byte() % 8 + 1
    }

    pub fn real(&mut self) -> Real {
        Real::from(self.integer())
    }

    pub fn positive_real(&mut self) -> Real {
        Real::from(self.positive())
    }
}

pub fn bounded_arbitrary_source(data: &[u8]) -> Option<String> {
    if data.is_empty() {
        return None;
    }
    let raw = String::from_utf8_lossy(&data[..data.len().min(4096)]);
    if raw.matches(';').count() > 64
        || raw.matches('{').count() > 64
        || raw
            .as_bytes()
            .windows(2)
            .any(|pair| matches!(pair, [b'e' | b'E', b'0'..=b'9']))
    {
        return None;
    }

    let mut digits = 0;
    let mut source = String::with_capacity(raw.len());
    for character in raw.chars() {
        if character.is_ascii_digit() {
            digits += 1;
            if digits <= 2 {
                source.push(character);
            }
        } else {
            digits = 0;
            source.push(character);
        }
    }
    Some(source)
}

pub fn evaluate_source(source: &str, fragments: u8) {
    let Ok(file) = openscad_rs::parse(source) else {
        return;
    };
    let mut evaluator = Evaluator::new();
    evaluator.variables.insert(
        "$fn".into(),
        Value::Number(Real::from(fragments.clamp(3, 16))),
    );
    for (shape, color) in evaluator.eval_source_file(&file) {
        if let Some(color) = color {
            assert!(color.into_iter().all(f32::is_finite));
        }
        validate_shape(&shape);
    }
    assert!(evaluator.depth <= 512);
}

pub fn validate_shape(shape: &Shape) {
    match shape {
        Shape::TriangleMesh3D(mesh) => {
            for triangle in mesh.triangles.iter() {
                assert!(
                    triangle
                        .indices()
                        .into_iter()
                        .all(|index| index < mesh.positions.len())
                );
            }
            let valid = mesh
                .has_unique_nondegenerate_triangles(&MESH_CONTEXT)
                .expect("strict exact triangle validation must decide");
            assert!(
                valid.value,
                "SynapsCAD shape retained duplicate or degenerate exact triangles"
            );
            if let Ok(rendered) = triangle_mesh_to_mesh_data(mesh) {
                assert_render_mesh(&rendered);
            }
        }
        Shape::CurveRegion2D(region) => {
            if let Ok(outcome) =
                csgrs::curve::try_triangulate(region, &csgrs::GeometryContext::STRICT)
            {
                let mesh = outcome.into_value();
                assert!(
                    mesh.has_unique_nondegenerate_triangles(&MESH_CONTEXT)
                        .expect("strict exact triangle validation must decide")
                        .value
                );
                if let Ok(rendered) = triangle_mesh_to_mesh_data(&mesh) {
                    assert_render_mesh(&rendered);
                }
            }
        }
        Shape::Failed(error) => {
            assert!(!error.is_empty());
        }
    }
}

pub fn assert_render_mesh(mesh: &synaps_cad::compiler::MeshData) {
    assert_eq!(mesh.positions.len(), mesh.normals.len());
    assert!(
        mesh.positions
            .iter()
            .flatten()
            .all(|value| value.is_finite())
    );
    assert!(mesh.normals.iter().flatten().all(|value| value.is_finite()));
    assert!(
        mesh.indices
            .iter()
            .all(|&index| usize::try_from(index).is_ok_and(|index| index < mesh.positions.len()))
    );
}

fn scalar(cursor: &mut ByteCursor<'_>) -> String {
    let denominator = cursor.positive();
    format!("{}/{denominator}", cursor.integer())
}

fn positive_scalar(cursor: &mut ByteCursor<'_>) -> String {
    let denominator = cursor.positive();
    format!("{}/{denominator}", cursor.positive())
}

fn primitive_3d(cursor: &mut ByteCursor<'_>) -> String {
    match cursor.choice(5) {
        0 => format!(
            "cube([{}, {}, {}], center = {});",
            cursor.positive(),
            cursor.positive(),
            cursor.positive(),
            cursor.byte() & 1 == 0
        ),
        1 => format!(
            "sphere(r = {}, $fn = {});",
            positive_scalar(cursor),
            cursor.positive() + 2
        ),
        2 => format!(
            "cylinder(h = {}, r1 = {}, r2 = {}, center = {}, $fn = {});",
            cursor.positive(),
            positive_scalar(cursor),
            positive_scalar(cursor),
            cursor.byte() & 1 == 0,
            cursor.positive() + 2
        ),
        3 => "polyhedron(points=[[0,0,0],[3,0,0],[0,3,0],[0,0,3]], faces=[[0,2,1],[0,1,3],[0,3,2],[1,2,3]]);".into(),
        _ => format!(
            "linear_extrude(height = {}, twist = {}, slices = {}) {}",
            cursor.positive(),
            cursor.integer() * 15,
            cursor.positive(),
            primitive_2d(cursor)
        ),
    }
}

fn primitive_2d(cursor: &mut ByteCursor<'_>) -> String {
    match cursor.choice(4) {
        0 => format!(
            "square([{}, {}], center = {});",
            cursor.positive(),
            cursor.positive(),
            cursor.byte() & 1 == 0
        ),
        1 => format!(
            "circle(r = {}, $fn = {});",
            positive_scalar(cursor),
            cursor.positive() + 2
        ),
        2 => {
            let extent = cursor.positive();
            format!("polygon(points=[[0, 0], [{extent}, 0], [{extent}, {extent}], [0, {extent}]]);")
        }
        _ => format!(
            "offset(r = {}) square([{}, {}], center = true);",
            positive_scalar(cursor),
            cursor.positive(),
            cursor.positive()
        ),
    }
}

pub fn structured_geometry_program(data: &[u8]) -> Option<String> {
    if data.is_empty() {
        return None;
    }
    let mut cursor = ByteCursor::new(data);
    let fragments = cursor.positive() + 2;
    let body = match cursor.choice(10) {
        0 => primitive_3d(&mut cursor),
        1 => primitive_2d(&mut cursor),
        2 => format!(
            "translate([{}, {}, {}]) rotate([{}, {}, {}]) scale([{}, {}, {}]) {}",
            cursor.integer(),
            cursor.integer(),
            cursor.integer(),
            cursor.integer() * 15,
            cursor.integer() * 15,
            cursor.integer() * 15,
            scalar(&mut cursor),
            scalar(&mut cursor),
            scalar(&mut cursor),
            primitive_3d(&mut cursor)
        ),
        3 => "union() { cube(3); translate([1, 1, 1]) cube(3); }".into(),
        4 => "difference() { cube(4); translate([2, 2, 2]) cube(3); }".into(),
        5 => "intersection() { cube(4); translate([1, 1, 1]) cube(4); }".into(),
        6 => "union() { square(3); translate([1, 1]) circle(2); }".into(),
        7 => "difference() { square(4); translate([1, 1]) circle(2); }".into(),
        8 => "hull() { cube(1); translate([2, 1, 1]) cube(1); }".into(),
        _ => format!(
            "mirror([{}, {}, {}]) {}",
            cursor.integer(),
            cursor.integer(),
            cursor.integer(),
            primitive_3d(&mut cursor)
        ),
    };
    Some(format!("$fn = {fragments};\n{body}\n"))
}

pub fn expression(cursor: &mut ByteCursor<'_>, depth: usize) -> String {
    if depth == 0 {
        return match cursor.choice(6) {
            0 => cursor.integer().to_string(),
            1 => scalar(cursor),
            2 => "true".into(),
            3 => "false".into(),
            4 => "x".into(),
            _ => format!(
                "[{}, {}, {}]",
                cursor.integer(),
                cursor.integer(),
                cursor.integer()
            ),
        };
    }
    match cursor.choice(9) {
        0 => format!(
            "({} + {})",
            expression(cursor, depth - 1),
            expression(cursor, depth - 1)
        ),
        1 => format!(
            "({} * {})",
            expression(cursor, depth - 1),
            expression(cursor, depth - 1)
        ),
        2 => format!(
            "({} < {} ? {} : {})",
            expression(cursor, depth - 1),
            expression(cursor, depth - 1),
            expression(cursor, depth - 1),
            expression(cursor, depth - 1)
        ),
        3 => format!(
            "(let (x = {}) {})",
            expression(cursor, depth - 1),
            expression(cursor, depth - 1)
        ),
        4 => format!("abs({})", expression(cursor, depth - 1)),
        5 => format!(
            "min({}, {})",
            expression(cursor, depth - 1),
            expression(cursor, depth - 1)
        ),
        6 => format!("[for (i = [0:3]) i + ({})]", expression(cursor, depth - 1)),
        7 => format!("[1, 2, {}][0]", expression(cursor, depth - 1)),
        _ => format!(
            "len([{}, {}])",
            expression(cursor, depth - 1),
            expression(cursor, depth - 1)
        ),
    }
}
