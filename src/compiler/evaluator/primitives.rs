use csgrs::Real;
use csgrs::{curve, solid};

use super::{Evaluator, Value};
use crate::compiler::geometry::Shape;
use crate::compiler::rendering::fonts::{
    apply_text_alignment, render_text_with_direction, resolve_font_data,
};

impl Evaluator {
    pub fn eval_cube(&mut self, args: &[(Option<String>, Value)]) -> Option<Shape> {
        let default_size = Value::Number(Real::one());
        let size_val = Self::get_arg(args, "size", 0).unwrap_or(&default_size);
        let center = match Self::get_arg_bool(args, "center", 1, false) {
            Ok(value) => value,
            Err(error) => return Some(Shape::Failed(format!("cube() failed: {error}"))),
        };

        let mesh = match size_val {
            Value::Number(_) => {
                let mesh = solid::cube(size_val.as_real()?);
                if center { solid::center(&mesh) } else { mesh }
            }
            Value::List(dims) => {
                let Some(nums): Option<Vec<Real>> = dims.iter().map(Value::as_real).collect()
                else {
                    return Some(Shape::Failed(
                        "cube() size vector must contain only numbers".into(),
                    ));
                };
                let (x, y, z) = match nums.len() {
                    1 => (nums[0].clone(), nums[0].clone(), nums[0].clone()),
                    2 => (nums[0].clone(), nums[1].clone(), Real::one()),
                    _ => (
                        nums.first().cloned().unwrap_or_else(Real::one),
                        nums.get(1).cloned().unwrap_or_else(Real::one),
                        nums.get(2).cloned().unwrap_or_else(Real::one),
                    ),
                };
                let mesh = solid::cuboid(x, y, z);
                if center { solid::center(&mesh) } else { mesh }
            }
            _ => return None,
        };

        Some(Shape::from_triangle_mesh(mesh))
    }

    #[must_use]
    pub fn eval_sphere(&mut self, args: &[(Option<String>, Value)]) -> Option<Shape> {
        let r = Self::get_arg_real(args, "r", 0)
            .or_else(|| Self::get_arg_real(args, "d", 0).and_then(|d| (d / Real::from(2_u8)).ok()))
            .unwrap_or_else(Real::one);

        let slices = self.resolve_fn_with_radius(args, Some(&r));
        let stacks = slices / 2;

        Some(Shape::from_triangle_mesh(solid::sphere(r, slices, stacks)))
    }

    #[must_use]
    pub fn eval_cylinder(&mut self, args: &[(Option<String>, Value)]) -> Option<Shape> {
        let h = Self::get_arg_real(args, "h", 0)
            .or_else(|| Self::get_arg_real(args, "height", 0))
            .unwrap_or_else(Real::one);

        // Diameter arguments take precedence over their radius equivalents.
        let half = |d: Real| (d / Real::from(2_u8)).ok();
        let r1 = Self::get_arg_real(args, "r1", 99)
            .or_else(|| Self::get_arg_real(args, "d1", 99).and_then(half))
            .or_else(|| Self::get_arg_real(args, "r", 1))
            .or_else(|| Self::get_arg_real(args, "d", 1).and_then(half))
            .unwrap_or_else(Real::one);
        let r2 = Self::get_arg_real(args, "r2", 99)
            .or_else(|| Self::get_arg_real(args, "d2", 99).and_then(half))
            .unwrap_or_else(|| r1.clone());

        let center = match Self::get_arg_bool(args, "center", 99, false) {
            Ok(value) => value,
            Err(error) => return Some(Shape::Failed(format!("cylinder() failed: {error}"))),
        };
        // Tessellation follows the larger endpoint radius.
        let max_radius = hyperlimit::real_max(&r1, &r2, crate::compiler::PREDICATE_POLICY)
            .value()
            .cloned()
            .unwrap_or_else(|| r1.abs() + r2.abs());
        let slices = self.resolve_fn_with_radius(args, Some(&max_radius));

        // `frustum` handles zero-radius cone tips without degenerate quads.
        let m = if super::value::reals_equal(&r1, &r2) == Some(true) {
            solid::cylinder(r1, h, slices)
        } else {
            solid::frustum(r1, r2, h, slices)
        };
        let m = if center { solid::center(&m) } else { m };

        Some(Shape::from_triangle_mesh(m))
    }

    #[allow(
        clippy::unused_self,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::missing_panics_doc
    )]
    #[must_use]
    pub fn eval_polyhedron(&self, args: &[(Option<String>, Value)]) -> Option<Shape> {
        let points_val = Self::get_arg(args, "points", 0)?;
        let faces_val =
            Self::get_arg(args, "faces", 1).or_else(|| Self::get_arg(args, "triangles", 1));

        let Some(points) = points_val
            .as_list()?
            .iter()
            .map(|value| {
                let numbers = value.to_real_list()?;
                (numbers.len() >= 3)
                    .then(|| [numbers[0].clone(), numbers[1].clone(), numbers[2].clone()])
            })
            .collect::<Option<Vec<[Real; 3]>>>()
        else {
            return Some(Shape::Failed(
                "polyhedron() points must be numeric 3D vectors".into(),
            ));
        };

        let Some(faces) = faces_val?
            .as_list()?
            .iter()
            .map(|value| {
                value
                    .as_list()?
                    .iter()
                    .map(Value::to_usize_exact)
                    .collect::<Option<Vec<_>>>()
            })
            .collect::<Option<Vec<Vec<usize>>>>()
        else {
            return Some(Shape::Failed(
                "polyhedron() faces must be vectors of nonnegative exact integers".into(),
            ));
        };

        let faces = {
            let mut seen = std::collections::HashSet::new();
            let mut deduped = Vec::with_capacity(faces.len());
            for face in &faces {
                if face.is_empty() {
                    continue;
                }
                // Canonical rotation makes cyclically equivalent faces equal.
                let min_pos = face
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, v)| *v)
                    .map(|(i, _)| i)
                    .unwrap();
                let mut canonical: Vec<usize> = face[min_pos..].to_vec();
                canonical.extend_from_slice(&face[..min_pos]);
                if seen.insert(canonical) {
                    deduped.push(face.clone());
                }
            }
            deduped
        };

        if faces.is_empty() {
            return None;
        }
        let face_refs = faces.iter().map(Vec::as_slice).collect::<Vec<_>>();
        Some(match solid::polyhedron(&points, &face_refs) {
            Ok(mesh) => Shape::from_triangle_mesh(mesh),
            Err(error) => Shape::Failed(format!("polyhedron() failed: {error}")),
        })
    }

    #[must_use]
    pub fn eval_circle(&mut self, args: &[(Option<String>, Value)]) -> Option<Shape> {
        let r = Self::get_arg_real(args, "r", 0)
            .or_else(|| Self::get_arg_real(args, "d", 0).and_then(|d| (d / Real::from(2_u8)).ok()))
            .unwrap_or_else(Real::one);

        let slices = self.resolve_fn_with_radius(args, Some(&r));
        Some(Shape::CurveRegion2D(curve::circle(r, slices)))
    }

    pub fn eval_square(&mut self, args: &[(Option<String>, Value)]) -> Option<Shape> {
        let default_size = Value::Number(Real::one());
        let size_val = Self::get_arg(args, "size", 0).unwrap_or(&default_size);
        let center = match Self::get_arg_bool(args, "center", 1, false) {
            Ok(value) => value,
            Err(error) => return Some(Shape::Failed(format!("square() failed: {error}"))),
        };

        let region = match size_val {
            Value::Number(_) => curve::square(size_val.as_real()?),
            Value::List(dims) => {
                let Some(nums): Option<Vec<Real>> = dims.iter().map(Value::as_real).collect()
                else {
                    return Some(Shape::Failed(
                        "square() size vector must contain only numbers".into(),
                    ));
                };
                let w = nums.first().cloned().unwrap_or_else(Real::one);
                let h = nums.get(1).cloned().unwrap_or_else(|| w.clone());
                curve::rectangle(w, h)
            }
            _ => return None,
        };

        let shape = Shape::CurveRegion2D(region);
        Some(if center { shape.center() } else { shape })
    }

    #[allow(clippy::unused_self)]
    #[must_use]
    pub fn eval_polygon(&self, args: &[(Option<String>, Value)]) -> Option<Shape> {
        let points_val = Self::get_arg(args, "points", 0)?;
        let Some(points) = points_val
            .as_list()?
            .iter()
            .map(|value| {
                let numbers = value.to_real_list()?;
                (numbers.len() >= 2).then(|| [numbers[0].clone(), numbers[1].clone()])
            })
            .collect::<Option<Vec<[Real; 2]>>>()
        else {
            return Some(Shape::Failed(
                "polygon() points must be numeric 2D vectors".into(),
            ));
        };

        if points.len() < 3 {
            return None;
        }
        Some(Shape::CurveRegion2D(curve::polygon(&points)))
    }

    #[must_use]
    pub fn eval_text(&self, args: &[(Option<String>, Value)]) -> Option<Shape> {
        // OpenSCAD signature: text, size, font, halign, valign, spacing,
        // direction, language, script, and $fn.
        let text_str = match Self::get_arg(args, "text", 0) {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Number(n)) => format!("{n}"),
            _ => return None,
        };

        if text_str.is_empty() {
            return None;
        }

        let size = Self::get_arg_real(args, "size", 1).unwrap_or_else(|| Real::from(10_u8));
        let spacing_val = Self::get_arg_real(args, "spacing", 5).unwrap_or_else(Real::one);

        // Prefer the requested system font, then bundled Liberation Sans.
        let font_param = match Self::get_arg(args, "font", 2) {
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        };
        let font_data = resolve_font_data(font_param.as_deref());

        let direction = match Self::get_arg(args, "direction", 6) {
            Some(Value::String(s)) => s.to_lowercase(),
            _ => "ltr".to_string(),
        };

        let region = match render_text_with_direction(
            &text_str,
            &font_data,
            &size,
            &spacing_val,
            &direction,
        ) {
            Ok(region) => region,
            Err(error) => return Some(Shape::Failed(error)),
        };

        let halign = match Self::get_arg(args, "halign", 3) {
            Some(Value::String(s)) => s.clone(),
            _ => "left".to_string(),
        };
        let valign = match Self::get_arg(args, "valign", 4) {
            Some(Value::String(s)) => s.clone(),
            _ => "baseline".to_string(),
        };

        let region = match apply_text_alignment(region, &halign, &valign) {
            Ok(region) => region,
            Err(error) => return Some(Shape::Failed(error)),
        };

        Some(Shape::CurveRegion2D(region))
    }
}
