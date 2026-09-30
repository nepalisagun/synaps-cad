use csgrs::Real;
use csgrs::{curve, solid};
use hypercurve::CurveRegion2;
use openscad_rs::ast::Statement;

use super::{Evaluator, Value};
use crate::compiler::geometry::{Shape, TransformKind};

impl Evaluator {
    pub fn eval_transform(
        &mut self,
        children: &[Statement],
        args: &[(Option<String>, Value)],
        kind: TransformKind,
    ) -> Option<Shape> {
        let child = self.eval_passthrough_children(children)?;
        Some(Self::apply_transform(child, &kind, args))
    }

    /// Evaluate children preserving per-shape colors, then apply a transform to each.
    pub fn eval_transform_into(
        &mut self,
        children: &[Statement],
        args: &[(Option<String>, Value)],
        kind: TransformKind,
        shapes: &mut Vec<(Shape, Option<[f32; 3]>)>,
    ) {
        let before = shapes.len();
        for stmt in children {
            self.eval_statement(stmt, shapes);
        }
        // Only shapes emitted by these children receive the transform.
        let new_shapes: Vec<_> = shapes.drain(before..).collect();
        for (s, color) in new_shapes {
            shapes.push((Self::apply_transform(s, &kind, args), color));
        }
    }

    /// Apply a single transform to a shape.
    pub fn apply_transform(
        shape: Shape,
        kind: &TransformKind,
        args: &[(Option<String>, Value)],
    ) -> Shape {
        match kind {
            TransformKind::Translate => {
                let value =
                    Self::get_positional_arg(args, 0).or_else(|| Self::get_named_arg(args, "v"));
                let v = match value {
                    Some(Value::List(_)) => match value.and_then(Value::to_real_list) {
                        Some(values) => values,
                        None => {
                            return Shape::Failed(
                                "translate() vector must contain only numbers".into(),
                            );
                        }
                    },
                    Some(_) => {
                        return Shape::Failed("translate() requires a numeric vector".into());
                    }
                    None => Vec::new(),
                };
                let (x, y, z) = (
                    v.first().cloned().unwrap_or_else(Real::zero),
                    v.get(1).cloned().unwrap_or_else(Real::zero),
                    v.get(2).cloned().unwrap_or_else(Real::zero),
                );
                shape.translate(x, y, z)
            }
            TransformKind::Rotate => {
                let axis_vec = match Self::get_named_arg(args, "v") {
                    Some(Value::List(_)) => {
                        match Self::get_named_arg(args, "v").and_then(Value::to_real_list) {
                            Some(values) => Some(values),
                            None => {
                                return Shape::Failed(
                                    "rotate() axis vector must contain only numbers".into(),
                                );
                            }
                        }
                    }
                    Some(_) => {
                        return Shape::Failed("rotate() axis must be a numeric vector".into());
                    }
                    None => None,
                };
                let a_val =
                    Self::get_positional_arg(args, 0).or_else(|| Self::get_named_arg(args, "a"));

                if let (Some(angle), Some(axis)) =
                    (a_val.as_ref().and_then(|v| v.as_real()), axis_vec)
                    && axis.len() >= 3
                {
                    let axis = hyperlattice::Vector3::new([
                        axis[0].clone(),
                        axis[1].clone(),
                        axis[2].clone(),
                    ]);
                    shape.rotate_axis_angle(&axis, &angle)
                } else if matches!(a_val, Some(Value::List(_))) {
                    let Some(v) = a_val.and_then(Value::to_real_list) else {
                        return Shape::Failed(
                            "rotate() angle vector must contain only numbers".into(),
                        );
                    };
                    let (x, y, z) = (
                        v.first().cloned().unwrap_or_else(Real::zero),
                        v.get(1).cloned().unwrap_or_else(Real::zero),
                        v.get(2).cloned().unwrap_or_else(Real::zero),
                    );
                    shape.rotate(x, y, z)
                } else if let Some(angle) = a_val.and_then(Value::as_real) {
                    shape.rotate(Real::zero(), Real::zero(), angle)
                } else if a_val.is_some() {
                    Shape::Failed("rotate() angle must be numeric".into())
                } else {
                    shape
                }
            }
            TransformKind::Scale => {
                let val =
                    Self::get_positional_arg(args, 0).or_else(|| Self::get_named_arg(args, "v"));
                match val {
                    Some(Value::List(_)) => {
                        let Some(v) = val.and_then(Value::to_real_list) else {
                            return Shape::Failed(
                                "scale() vector must contain only numbers".into(),
                            );
                        };
                        let (x, y, z) = (
                            v.first().cloned().unwrap_or_else(Real::one),
                            v.get(1).cloned().unwrap_or_else(Real::one),
                            v.get(2).cloned().unwrap_or_else(Real::one),
                        );
                        shape.scale(x, y, z)
                    }
                    Some(Value::Number(_)) => {
                        let s = val.and_then(Value::as_real).unwrap_or_else(Real::one);
                        shape.scale(s.clone(), s.clone(), s)
                    }
                    _ => shape,
                }
            }
            TransformKind::Mirror => {
                let value =
                    Self::get_positional_arg(args, 0).or_else(|| Self::get_named_arg(args, "v"));
                let v = match value {
                    Some(Value::List(_)) => match value.and_then(Value::to_real_list) {
                        Some(values) => values,
                        None => {
                            return Shape::Failed(
                                "mirror() vector must contain only numbers".into(),
                            );
                        }
                    },
                    Some(_) => return Shape::Failed("mirror() requires a numeric vector".into()),
                    None => vec![Real::one(), Real::zero(), Real::zero()],
                };
                let (nx, ny, nz) = (
                    v.first().cloned().unwrap_or_else(Real::one),
                    v.get(1).cloned().unwrap_or_else(Real::zero),
                    v.get(2).cloned().unwrap_or_else(Real::zero),
                );
                shape.mirror(nx, ny, nz)
            }
        }
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::float_cmp
    )]
    pub fn eval_linear_extrude(
        &mut self,
        children: &[Statement],
        args: &[(Option<String>, Value)],
    ) -> Option<Shape> {
        let height = Self::get_arg_real(args, "height", 0).unwrap_or_else(Real::one);
        let twist = Self::get_arg_real(args, "twist", 99).unwrap_or_else(Real::zero);
        let scale = match Self::get_named_arg(args, "scale") {
            None => [Real::one(), Real::one()],
            Some(Value::Number(value)) => [value.clone(), value.clone()],
            Some(Value::List(_)) => {
                let Some(values) = Self::get_named_arg(args, "scale").and_then(Value::to_real_list)
                else {
                    return Some(Shape::Failed(
                        "linear_extrude() scale vector must contain only numbers".into(),
                    ));
                };
                [
                    values.first().cloned().unwrap_or_else(Real::one),
                    values.get(1).cloned().unwrap_or_else(Real::one),
                ]
            }
            Some(_) => {
                return Some(Shape::Failed(
                    "linear_extrude() scale must be a number or numeric vector".into(),
                ));
            }
        };
        let center = match Self::get_arg_bool(args, "center", 99, false) {
            Ok(value) => value,
            Err(error) => {
                return Some(Shape::Failed(format!("linear_extrude() failed: {error}")));
            }
        };
        let slices = Self::get_arg_real(args, "slices", 99)
            .and_then(|value| value.round_certified().ok())
            .and_then(|integer| usize::try_from(integer).ok())
            .filter(|value| *value >= 1)
            .unwrap_or_else(|| self.resolve_fn(args));

        let child_shapes = self.eval_children(children);
        if child_shapes.is_empty() {
            return None;
        }

        let region = match self.shapes_to_curve_region(&child_shapes) {
            Ok(Some(region)) => region,
            Ok(None) => return None,
            Err(error) => return Some(Shape::Failed(error)),
        };

        let is_plain_extrusion = super::value::reals_equal(&twist, &Real::zero()) == Some(true)
            && super::value::reals_equal(&scale[0], &Real::one()) == Some(true)
            && super::value::reals_equal(&scale[1], &Real::one()) == Some(true);
        let mesh = if is_plain_extrusion {
            match curve::try_extrude(&region, height, &csgrs::GeometryContext::STRICT) {
                Ok(outcome) => outcome.into_value(),
                Err(error) => {
                    return Some(Shape::Failed(format!("linear_extrude() failed: {error:?}")));
                }
            }
        } else {
            match curve::extrude_twisted(
                &region,
                height,
                twist,
                scale,
                slices.max(1),
                &csgrs::GeometryContext::STRICT,
            ) {
                Ok(outcome) => outcome.into_value(),
                Err(error) => {
                    return Some(Shape::Failed(format!("linear_extrude() failed: {error:?}")));
                }
            }
        };

        let mesh = if center { solid::center(&mesh) } else { mesh };
        Some(Shape::from_triangle_mesh(mesh))
    }

    pub fn eval_rotate_extrude(
        &mut self,
        children: &[Statement],
        args: &[(Option<String>, Value)],
    ) -> Option<Shape> {
        let angle = Self::get_arg_real(args, "angle", 0).unwrap_or_else(|| Real::from(360));
        let slices = self.resolve_fn(args);

        let child_shapes = self.eval_children(children);
        if child_shapes.is_empty() {
            return None;
        }

        let region = match self.shapes_to_curve_region(&child_shapes) {
            Ok(Some(region)) => region,
            Ok(None) => return None,
            Err(error) => return Some(Shape::Failed(error)),
        };
        let mesh = match curve::revolve(&region, angle, slices, &csgrs::GeometryContext::STRICT) {
            Ok(outcome) => outcome.into_value(),
            Err(e) => {
                return Some(Shape::Failed(format!("rotate_extrude() failed: {e:?}")));
            }
        };
        Some(Shape::from_triangle_mesh(mesh))
    }

    /// Converts shapes to one filled curve region. 3D meshes are skipped with
    /// the same warning behavior as `OpenSCAD` extrusion.
    ///
    /// # Errors
    ///
    /// Returns an error when a child already failed or when exact union of the
    /// child regions cannot be certified.
    pub fn shapes_to_curve_region(
        &mut self,
        shapes: &[Shape],
    ) -> Result<Option<CurveRegion2>, String> {
        let mut result: Option<CurveRegion2> = None;
        for shape in shapes {
            match shape {
                Shape::CurveRegion2D(region) => {
                    result = match result {
                        Some(current) => {
                            match current.boolean_region(
                                region,
                                hypercurve::BooleanOp::Union,
                                &hypercurve::CurveContext::STRICT,
                            ) {
                                Ok(union) => Some(union.into_value()),
                                Err(error) => {
                                    return Err(format!(
                                        "exact 2D union inside extrusion failed: {error}"
                                    ));
                                }
                            }
                        }
                        None => Some(region.clone()),
                    };
                }
                Shape::TriangleMesh3D(_) => {
                    self.warnings
                        .push("3D mesh child inside extrude, skipping".into());
                }
                Shape::Failed(e) => {
                    return Err(format!("failed child inside extrusion: {e}"));
                }
            }
        }
        Ok(result)
    }
}
