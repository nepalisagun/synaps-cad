use csgrs::Real;
use csgrs::solid;
use hypercurve::{CurveFamily2, CurveGeometry2, CurveRegion2, OffsetCornerStyle2, Point2};
use openscad_rs::ast::Statement;
use std::cmp::Ordering;
use std::fmt;

use super::{Evaluator, Value};
use crate::compiler::geometry::{BoolOp, Shape};

fn offset_result<E: fmt::Display>(operation: &str, result: Result<CurveRegion2, E>) -> Shape {
    match result {
        Ok(offset) => Shape::CurveRegion2D(offset),
        Err(error) => Shape::Failed(format!("exact {operation} failed: {error}")),
    }
}

impl Evaluator {
    pub(super) fn apply_boolean(&mut self, lhs: Shape, rhs: Shape, op: BoolOp) -> Shape {
        if matches!(&lhs, Shape::Failed(_)) {
            return lhs;
        }
        if matches!(&rhs, Shape::Failed(_)) {
            return rhs;
        }

        let lhs_dimension = lhs
            .dimension()
            .expect("a non-failed shape always has a dimension");
        let rhs_dimension = rhs
            .dimension()
            .expect("a non-failed shape always has a dimension");
        if lhs_dimension == rhs_dimension {
            return match op {
                BoolOp::Union => lhs.union(rhs),
                BoolOp::Difference => lhs.difference(rhs),
                BoolOp::Intersection => lhs.intersection(rhs),
            };
        }

        self.warnings.push(format!(
            "Mixing 2D and 3D objects is not supported; ignoring {rhs_dimension} child object for {lhs_dimension} operation"
        ));
        match op {
            // OpenSCAD uses the first child to select the operation's
            // dimension, then ignores mismatched children for union and
            // difference.
            BoolOp::Union | BoolOp::Difference => lhs,
            // OpenSCAD's intersection of mismatched dimensions is empty in
            // the first child's dimension.
            BoolOp::Intersection => Shape::empty(lhs_dimension),
        }
    }

    #[allow(clippy::missing_panics_doc)]
    pub fn eval_boolean_op(&mut self, children: &[Statement], op: BoolOp) -> Option<Shape> {
        let child_shapes = self.eval_children(children);
        if child_shapes.is_empty() {
            return None;
        }

        let mut iter = child_shapes.into_iter();
        let first = iter.next().unwrap();

        match op {
            BoolOp::Union => {
                let rest: Vec<Shape> = iter.collect();
                if rest.is_empty() {
                    return Some(first);
                }
                let mut result = first;
                for child in rest {
                    result = self.apply_boolean(result, child, BoolOp::Union);
                }
                Some(result)
            }
            BoolOp::Difference => {
                let rest: Vec<Shape> = iter.collect();
                if rest.is_empty() {
                    return Some(first);
                }
                let mut result = first;
                for child in rest {
                    result = self.apply_boolean(result, child, BoolOp::Difference);
                }
                Some(result)
            }
            BoolOp::Intersection => {
                let mut result = first;
                for child in iter {
                    result = self.apply_boolean(result, child, BoolOp::Intersection);
                }
                Some(result)
            }
        }
    }

    pub fn eval_offset(
        &mut self,
        children: &[Statement],
        args: &[(Option<String>, Value)],
    ) -> Option<Shape> {
        let r = Self::get_arg_real(args, "r", 99);
        let delta = Self::get_arg_real(args, "delta", 99);

        let child_shapes = self.eval_children(children);
        if child_shapes.is_empty() {
            return None;
        }
        let region = match self.shapes_to_curve_region(&child_shapes) {
            Ok(Some(region)) => region,
            Ok(None) => return None,
            Err(error) => return Some(Shape::Failed(error)),
        };

        if let Some(r_val) = r {
            if super::value::reals_equal(&r_val, &Real::zero()) == Some(true) {
                Some(Shape::CurveRegion2D(region))
            } else {
                Some(offset_result(
                    "offset(r=...)",
                    region.offset(r_val, &OffsetCornerStyle2::Round),
                ))
            }
        } else if let Some(d_val) = delta {
            if super::value::reals_equal(&d_val, &Real::zero()) == Some(true) {
                Some(Shape::CurveRegion2D(region))
            } else {
                Some(offset_result(
                    "offset(delta=...)",
                    region.offset(
                        d_val,
                        &OffsetCornerStyle2::Miter {
                            limit: Real::from(4),
                        },
                    ),
                ))
            }
        } else {
            let d = Self::get_arg_real(args, "", 0).unwrap_or_else(Real::zero);
            if super::value::reals_equal(&d, &Real::zero()) == Some(true) {
                Some(Shape::CurveRegion2D(region))
            } else {
                Some(offset_result(
                    "offset(...)",
                    region.offset(d, &OffsetCornerStyle2::Round),
                ))
            }
        }
    }

    pub fn eval_hull(&mut self, children: &[Statement]) -> Option<Shape> {
        let child_shapes = self.eval_children(children);
        if child_shapes.is_empty() {
            return None;
        }
        let Some(dimension) = child_shapes[0].dimension() else {
            return child_shapes.into_iter().next();
        };
        let mut meshes = Vec::new();
        let mut planar_points = Vec::new();
        for shape in child_shapes {
            let Some(shape_dimension) = shape.dimension() else {
                let Shape::Failed(error) = shape else {
                    unreachable!();
                };
                return Some(Shape::Failed(error));
            };
            if shape_dimension != dimension {
                self.warnings.push(format!(
                    "Mixing 2D and 3D objects is not supported; ignoring {shape_dimension} child object for {dimension} hull"
                ));
                continue;
            }
            if dimension == crate::compiler::geometry::ShapeDimension::Two {
                let Shape::CurveRegion2D(region) = shape else {
                    unreachable!("the dimension check establishes a 2D curve region");
                };
                if let Err(error) = collect_linear_region_vertices(&region, &mut planar_points) {
                    return Some(Shape::Failed(format!("hull() failed: {error}")));
                }
                continue;
            }
            let mesh = match shape.try_into_triangle_mesh() {
                Ok(mesh) => mesh,
                Err(error) => return Some(Shape::Failed(format!("hull() failed: {error}"))),
            };
            meshes.push(mesh);
        }
        if dimension == crate::compiler::geometry::ShapeDimension::Two {
            return Some(match exact_planar_hull(planar_points) {
                Ok(region) => Shape::CurveRegion2D(region),
                Err(error) => Shape::Failed(format!("hull() failed: {error}")),
            });
        }
        let combined = solid::merge(&meshes);
        match solid::convex_hull(&combined) {
            Ok(hull) => Some(Shape::from_triangle_mesh(hull)),
            Err(error) => Some(Shape::Failed(format!("hull() failed: {error}"))),
        }
    }

    pub fn eval_color_into(
        &mut self,
        children: &[Statement],
        args: &[(Option<String>, Value)],
        shapes: &mut Vec<(Shape, Option<[f32; 3]>)>,
    ) {
        let rgb = Self::parse_color_args(args);
        if let Some(c) = rgb {
            self.color_stack.push(c);
        }
        for stmt in children {
            self.eval_statement(stmt, shapes);
        }
        if rgb.is_some() {
            self.color_stack.pop();
        }
    }

    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn parse_color_args(args: &[(Option<String>, Value)]) -> Option<[f32; 3]> {
        let first = args.first().map(|(_, v)| v)?;
        match first {
            Value::String(name) => parse_hex_color(name)
                .or_else(|| crate::compiler::rendering::colors::named_color(name)),
            Value::List(items) => {
                if items.len() >= 3 {
                    let r = items[0].to_f64_lossy()? as f32;
                    let g = items[1].to_f64_lossy()? as f32;
                    let b = items[2].to_f64_lossy()? as f32;
                    Some([r, g, b])
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

fn collect_linear_region_vertices(
    region: &CurveRegion2,
    points: &mut Vec<Point2>,
) -> Result<(), String> {
    for boundary in region.boundary_loops() {
        for curve in boundary.curves() {
            match curve.geometry() {
                Some(geometry) => push_linear_geometry(geometry, points)?,
                None if curve.family() == CurveFamily2::Line => {
                    let start = curve.start();
                    let end = curve.end();
                    let (Some(start), Some(end)) = (start.coordinates(), end.coordinates()) else {
                        return Err(
                            "2D hull cannot yet materialize algebraic boundary curves exactly"
                                .into(),
                        );
                    };
                    points.push(start.clone());
                    points.push(end.clone());
                }
                None => {
                    return Err(
                        "2D hull cannot yet materialize algebraic boundary curves exactly".into(),
                    );
                }
            }
        }
    }
    Ok(())
}

fn push_linear_geometry(geometry: &CurveGeometry2, points: &mut Vec<Point2>) -> Result<(), String> {
    let (start, end, controls) = match geometry {
        CurveGeometry2::Line(line) => {
            points.push(line.start().clone());
            points.push(line.end().clone());
            return Ok(());
        }
        CurveGeometry2::QuadraticBezier(curve) => {
            let controls = curve.control_points();
            (controls[0], controls[2], vec![controls[1]])
        }
        CurveGeometry2::CubicBezier(curve) => {
            let controls = curve.control_points();
            (controls[0], controls[3], vec![controls[1], controls[2]])
        }
        CurveGeometry2::RationalQuadraticBezier(curve) => {
            let controls = curve.control_points();
            (controls[0], controls[2], vec![controls[1]])
        }
        CurveGeometry2::RationalBezier(curve) => {
            let Some(controls) = curve.affine_control_points() else {
                return Err(
                    "2D hull cannot yet materialize algebraic boundary curves exactly".into(),
                );
            };
            if controls.len() < 2 {
                return Err(
                    "2D hull of retained curved boundaries is not yet implemented exactly".into(),
                );
            }
            let start = &controls[0];
            let end = &controls[controls.len() - 1];
            (start, end, controls[1..controls.len() - 1].iter().collect())
        }
        CurveGeometry2::CircularArc(_)
        | CurveGeometry2::PolynomialBSpline(_)
        | CurveGeometry2::Nurbs(_) => {
            return Err(
                "2D hull of retained curved boundaries is not yet implemented exactly".into(),
            );
        }
    };
    for control in controls {
        match orient_curve_points(start, end, control)? {
            hyperlimit::Sign::Zero => {}
            _ => {
                return Err(
                    "2D hull of retained curved boundaries is not yet implemented exactly".into(),
                );
            }
        }
    }
    points.push(start.clone());
    points.push(end.clone());
    Ok(())
}

fn exact_planar_hull(mut points: Vec<Point2>) -> Result<CurveRegion2, String> {
    for index in 1..points.len() {
        let mut position = index;
        while position > 0 {
            let ordering = compare_curve_points(&points[position - 1], &points[position])?;
            if ordering != Ordering::Greater {
                break;
            }
            points.swap(position - 1, position);
            position -= 1;
        }
    }

    let mut unique = Vec::with_capacity(points.len());
    for point in points {
        if let Some(previous) = unique.last()
            && compare_curve_points(previous, &point)? == Ordering::Equal
        {
            continue;
        }
        unique.push(point);
    }
    if unique.len() < 3 {
        return Err("2D hull requires at least three distinct points".into());
    }

    let mut lower = Vec::new();
    for point in &unique {
        append_hull_point(&mut lower, point)?;
    }
    let mut upper = Vec::new();
    for point in unique.iter().rev() {
        append_hull_point(&mut upper, point)?;
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    if lower.len() < 3 {
        return Err("2D hull points are collinear".into());
    }

    let coordinates = lower
        .into_iter()
        .map(|point| [point.x().clone(), point.y().clone()])
        .collect::<Vec<_>>();
    Ok(csgrs::curve::polygon(&coordinates))
}

fn append_hull_point(hull: &mut Vec<Point2>, point: &Point2) -> Result<(), String> {
    while hull.len() >= 2 {
        let turn = orient_curve_points(&hull[hull.len() - 2], &hull[hull.len() - 1], point)?;
        if turn == hyperlimit::Sign::Positive {
            break;
        }
        hull.pop();
    }
    hull.push(point.clone());
    Ok(())
}

fn compare_curve_points(left: &Point2, right: &Point2) -> Result<Ordering, String> {
    let x = super::value::compare_reals(left.x(), right.x())
        .ok_or_else(|| "2D hull x-coordinate ordering is undecided".to_owned())?;
    if x != Ordering::Equal {
        return Ok(x);
    }
    super::value::compare_reals(left.y(), right.y())
        .ok_or_else(|| "2D hull y-coordinate ordering is undecided".to_owned())
}

fn orient_curve_points(a: &Point2, b: &Point2, c: &Point2) -> Result<hyperlimit::Sign, String> {
    let a = hyperlimit::Point2::new(a.x().clone(), a.y().clone());
    let b = hyperlimit::Point2::new(b.x().clone(), b.y().clone());
    let c = hyperlimit::Point2::new(c.x().clone(), c.y().clone());
    hyperlimit::orient2(&a, &b, &c, crate::compiler::PREDICATE_POLICY)
        .value()
        .ok_or_else(|| "2D hull orientation is undecided".to_owned())
}

/// Parse a hex color string like "#D4A76A", "#fff", or "D4A76A" into [r, g, b] in 0.0–1.0 range.
fn parse_hex_color(s: &str) -> Option<[f32; 3]> {
    let hex = s.strip_prefix('#').unwrap_or(s);
    match hex.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some([
                f32::from(r) / 255.0,
                f32::from(g) / 255.0,
                f32::from(b) / 255.0,
            ])
        }
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
            Some([
                f32::from(r) / 15.0,
                f32::from(g) / 15.0,
                f32::from(b) / 15.0,
            ])
        }
        8 => {
            // OpenSCAD geometry colors ignore the `#RRGGBBAA` alpha component.
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some([
                f32::from(r) / 255.0,
                f32::from(g) / 255.0,
                f32::from(b) / 255.0,
            ])
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_offset_remains_an_explicit_failed_shape() {
        let shape = offset_result(
            "offset(delta=...)",
            Err::<CurveRegion2, _>("unsupported topology"),
        );
        let Shape::Failed(error) = shape else {
            panic!("an offset failure must not preserve the unchanged input");
        };
        assert_eq!(
            error,
            "exact offset(delta=...) failed: unsupported topology"
        );
    }

    #[test]
    fn planar_hull_uses_exact_curve_vertices_without_inventing_thickness() {
        let source =
            openscad_rs::parse("hull() { square([2, 2]); translate([3, 1]) square([1, 1]); }")
                .unwrap();
        let mut evaluator = Evaluator::new();
        let shapes = evaluator.eval_source_file(&source);

        assert!(evaluator.warnings.is_empty(), "{:?}", evaluator.warnings);
        assert_eq!(shapes.len(), 1);
        let Shape::CurveRegion2D(region) = &shapes[0].0 else {
            panic!("OpenSCAD 2D hull must remain a 2D curve region");
        };
        let bounds = csgrs::curve::try_bounding_box(region).unwrap();
        assert_eq!(bounds.mins.x, Real::zero());
        assert_eq!(bounds.mins.y, Real::zero());
        assert_eq!(bounds.maxs.x, Real::from(4_u8));
        assert_eq!(bounds.maxs.y, Real::from(2_u8));
    }
}
