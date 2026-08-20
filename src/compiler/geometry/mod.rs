use std::fmt;

use csgrs::curve::CurveRegionExt;
use csgrs::solid::{self, SolidExt};
use csgrs::{Real, TriangleMesh};
use hypercurve::{CurvePolicy, CurveRegion2};
use hyperlattice::{Matrix4, Point3, Vector3};
use hypermesh::Plane;

#[derive(Clone, Copy, Debug)]
pub enum BoolOp {
    Union,
    Difference,
    Intersection,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShapeDimension {
    Two,
    Three,
}

impl fmt::Display for ShapeDimension {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Two => f.write_str("2D"),
            Self::Three => f.write_str("3D"),
        }
    }
}

#[derive(Clone, Copy)]
pub enum TransformKind {
    Translate,
    Rotate,
    Scale,
    Mirror,
}

#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
pub enum Shape {
    TriangleMesh3D(TriangleMesh),
    CurveRegion2D(CurveRegion2),
    /// Boolean/transform operations failed with this error.
    Failed(String),
}

impl fmt::Debug for Shape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TriangleMesh3D(_) => write!(f, "Shape::TriangleMesh3D"),
            Self::CurveRegion2D(_) => write!(f, "Shape::CurveRegion2D"),
            Self::Failed(e) => write!(f, "Shape::Failed({e})"),
        }
    }
}

impl Shape {
    /// Creates a 3D shape from native triangle geometry.
    #[must_use]
    pub const fn from_triangle_mesh(mesh: TriangleMesh) -> Self {
        Self::TriangleMesh3D(mesh)
    }

    /// Extract 3D mesh data without inventing thickness for a 2D curve region.
    ///
    /// # Errors
    ///
    /// Returns the retained geometry error or rejects a 2D curve region that
    /// has not been explicitly extruded.
    pub fn try_into_triangle_mesh(self) -> Result<TriangleMesh, String> {
        match self {
            Self::TriangleMesh3D(mesh) => Ok(mesh),
            Self::CurveRegion2D(_) => Err(
                "cannot implicitly coerce 2D geometry to 3D; use linear_extrude() explicitly"
                    .into(),
            ),
            Self::Failed(error) => Err(error),
        }
    }

    #[must_use]
    pub const fn dimension(&self) -> Option<ShapeDimension> {
        match self {
            Self::TriangleMesh3D(_) => Some(ShapeDimension::Three),
            Self::CurveRegion2D(_) => Some(ShapeDimension::Two),
            Self::Failed(_) => None,
        }
    }

    #[must_use]
    pub fn empty(dimension: ShapeDimension) -> Self {
        match dimension {
            ShapeDimension::Two => Self::CurveRegion2D(CurveRegion2::empty()),
            ShapeDimension::Three => Self::TriangleMesh3D(solid::empty()),
        }
    }

    #[must_use]
    pub fn union(self, other: Self) -> Self {
        if let Self::Failed(e) = &self {
            return Self::Failed(e.clone());
        }
        if let Self::Failed(e) = &other {
            return Self::Failed(e.clone());
        }
        match (self, other) {
            (Self::CurveRegion2D(a), Self::CurveRegion2D(b)) => {
                match a.try_union(&b, &hypercurve::CurvePolicy::STRICT) {
                    Ok(region) => Self::CurveRegion2D(region.into_value()),
                    Err(error) => Self::Failed(format!("exact 2D union failed: {error}")),
                }
            }
            (Self::TriangleMesh3D(a), Self::TriangleMesh3D(b)) => {
                triangle_mesh_boolean(a, b, BoolOp::Union)
            }
            _ => Self::Failed("cannot union mixed 2D and 3D geometry".into()),
        }
    }

    #[must_use]
    pub fn difference(self, other: Self) -> Self {
        if let Self::Failed(e) = &self {
            return Self::Failed(e.clone());
        }
        if let Self::Failed(e) = &other {
            return Self::Failed(e.clone());
        }
        match (self, other) {
            (Self::CurveRegion2D(a), Self::CurveRegion2D(b)) => {
                match a.try_difference(&b, &hypercurve::CurvePolicy::STRICT) {
                    Ok(region) => Self::CurveRegion2D(region.into_value()),
                    Err(error) => Self::Failed(format!("exact 2D difference failed: {error}")),
                }
            }
            (Self::TriangleMesh3D(a), Self::TriangleMesh3D(b)) => {
                triangle_mesh_boolean(a, b, BoolOp::Difference)
            }
            _ => Self::Failed("cannot subtract mixed 2D and 3D geometry".into()),
        }
    }

    #[must_use]
    pub fn intersection(self, other: Self) -> Self {
        if let Self::Failed(e) = &self {
            return Self::Failed(e.clone());
        }
        if let Self::Failed(e) = &other {
            return Self::Failed(e.clone());
        }
        match (self, other) {
            (Self::CurveRegion2D(a), Self::CurveRegion2D(b)) => {
                match a.try_intersection(&b, &hypercurve::CurvePolicy::STRICT) {
                    Ok(region) => Self::CurveRegion2D(region.into_value()),
                    Err(error) => Self::Failed(format!("exact 2D intersection failed: {error}")),
                }
            }
            (Self::TriangleMesh3D(a), Self::TriangleMesh3D(b)) => {
                triangle_mesh_boolean(a, b, BoolOp::Intersection)
            }
            _ => Self::Failed("cannot intersect mixed 2D and 3D geometry".into()),
        }
    }

    #[must_use]
    pub fn translate(self, x: Real, y: Real, z: Real) -> Self {
        match self {
            Self::TriangleMesh3D(mesh) => Self::TriangleMesh3D(mesh.translated(x, y, z)),
            // OpenSCAD retains the projected result as a 2D object. The
            // curve kernel applies the XY part of this exact transform.
            Self::CurveRegion2D(region) => match csgrs::curve::try_translated(&region, x, y) {
                Ok(region) => Self::CurveRegion2D(region),
                Err(error) => Self::Failed(format!("translate() failed: {error}")),
            },
            Self::Failed(e) => Self::Failed(e),
        }
    }

    #[must_use]
    pub fn rotate(self, x: Real, y: Real, z: Real) -> Self {
        match self {
            Self::TriangleMesh3D(mesh) => Self::TriangleMesh3D(solid::rotate(&mesh, x, y, z)),
            Self::CurveRegion2D(region) => {
                let matrix = Matrix4::rotation_z(z.to_radians())
                    * Matrix4::rotation_y(y.to_radians())
                    * Matrix4::rotation_x(x.to_radians());
                transform_curve_region(&region, &matrix, "rotate()")
            }
            Self::Failed(e) => Self::Failed(e),
        }
    }

    #[must_use]
    pub fn scale(self, sx: Real, sy: Real, sz: Real) -> Self {
        match self {
            Self::TriangleMesh3D(mesh) => Self::TriangleMesh3D(solid::scale(&mesh, sx, sy, sz)),
            Self::CurveRegion2D(region) => {
                let matrix = Matrix4::affine_nonuniform_scale([sx, sy, Real::one()]);
                transform_curve_region(&region, &matrix, "scale()")
            }
            Self::Failed(e) => Self::Failed(e),
        }
    }

    #[must_use]
    pub fn mirror(self, nx: Real, ny: Real, nz: Real) -> Self {
        if [&nx, &ny, &nz].into_iter().all(|coordinate| {
            hyperlimit::classify_real_sign(coordinate, crate::compiler::PREDICATE_POLICY).value()
                == Some(hyperlimit::Sign::Zero)
        }) {
            return self;
        }
        let plane = Plane::new(Point3::new(nx, ny, nz), Real::zero());
        let matrix = match plane.reflection_matrix(&crate::compiler::MESH_CONTEXT) {
            Ok(matrix) => matrix.into_value(),
            Err(error) => return Self::Failed(format!("mirror() failed: {error}")),
        };
        match self {
            Self::TriangleMesh3D(mesh) => Self::TriangleMesh3D(solid::transform(&mesh, &matrix)),
            Self::CurveRegion2D(region) => transform_curve_region(&region, &matrix, "mirror()"),
            Self::Failed(e) => Self::Failed(e),
        }
    }

    /// Rotates around an arbitrary exact axis without a primitive-float Euler conversion.
    #[must_use]
    pub fn rotate_axis_angle(self, axis: &Vector3, angle_degrees: &Real) -> Self {
        // Route exact coordinate axes through the retained csgrs rigid-rotation
        // object. This is geometrically identical to Rodrigues' formula, while
        // preserving its transformed support-plane certificates for HyperMesh.
        for coordinate in 0..3 {
            if (0..3)
                .filter(|&candidate| candidate != coordinate)
                .all(|candidate| axis.0[candidate].definitely_zero())
            {
                let angle = match hyperlimit::classify_real_sign(
                    &axis.0[coordinate],
                    crate::compiler::PREDICATE_POLICY,
                )
                .value()
                {
                    Some(hyperlimit::Sign::Positive) => angle_degrees.clone(),
                    Some(hyperlimit::Sign::Negative) => -angle_degrees.clone(),
                    Some(hyperlimit::Sign::Zero) | None => break,
                };
                let zero = Real::zero();
                return match coordinate {
                    0 => self.rotate(angle, zero.clone(), zero),
                    1 => self.rotate(zero.clone(), angle, zero),
                    2 => self.rotate(zero.clone(), zero, angle),
                    _ => unreachable!(),
                };
            }
        }
        let matrix = match Matrix4::rotation_axis_angle(axis, angle_degrees.to_radians()) {
            Ok(matrix) => matrix,
            Err(error) => return Self::Failed(format!("axis-angle rotation failed: {error:?}")),
        };
        match self {
            Self::TriangleMesh3D(mesh) => Self::TriangleMesh3D(solid::transform(&mesh, &matrix)),
            Self::CurveRegion2D(region) => {
                transform_curve_region(&region, &matrix, "axis-angle rotate()")
            }
            Self::Failed(error) => Self::Failed(error),
        }
    }

    #[must_use]
    #[allow(dead_code)]
    pub fn center(self) -> Self {
        match self {
            Self::TriangleMesh3D(mesh) => Self::TriangleMesh3D(solid::center(&mesh)),
            Self::CurveRegion2D(region) => {
                let bounds = match csgrs::curve::try_bounding_box(&region) {
                    Ok(bounds) => bounds,
                    Err(error) => return Self::Failed(format!("center() failed: {error}")),
                };
                let two = Real::from(2_u8);
                let Ok(x) = (&bounds.mins.x + &bounds.maxs.x) / &two else {
                    return Self::Failed("center() could not divide the x bounds by two".into());
                };
                let Ok(y) = (&bounds.mins.y + &bounds.maxs.y) / &two else {
                    return Self::Failed("center() could not divide the y bounds by two".into());
                };
                match csgrs::curve::try_translated(&region, -x, -y) {
                    Ok(region) => Self::CurveRegion2D(region),
                    Err(error) => Self::Failed(format!("center() failed: {error}")),
                }
            }
            Self::Failed(e) => Self::Failed(e),
        }
    }
}

fn transform_curve_region(region: &CurveRegion2, matrix: &Matrix4, operation: &str) -> Shape {
    match region.transform_affine(
        &matrix.0[0][0],
        &matrix.0[0][1],
        &matrix.0[1][0],
        &matrix.0[1][1],
        &matrix.0[0][3],
        &matrix.0[1][3],
        &CurvePolicy::STRICT,
    ) {
        Ok(region) => Shape::CurveRegion2D(region.into_value()),
        Err(error) => Shape::Failed(format!(
            "{operation} collapsed or invalidated a 2D curve region: {error}"
        )),
    }
}

#[allow(clippy::needless_pass_by_value)]
fn triangle_mesh_boolean(lhs: TriangleMesh, rhs: TriangleMesh, op: BoolOp) -> Shape {
    let result = match op {
        BoolOp::Union => lhs.try_union(&rhs),
        BoolOp::Difference => lhs.try_difference(&rhs),
        BoolOp::Intersection => lhs.try_intersection(&rhs),
    };
    match result {
        Ok(mesh) => Shape::TriangleMesh3D(mesh),
        Err(error) => Shape::Failed(format!("exact {op:?} failed: {error}")),
    }
}

pub mod conversions;

#[cfg(test)]
mod tests {
    use super::*;
    use hypercurve::{Curve2, CurvePath2, CurveRegion2, LineSeg2, Point2, QuadraticBezier2};

    fn point(x: i64, y: i64) -> Point2 {
        Point2::new(Real::from(x), Real::from(y))
    }

    #[test]
    fn shape_keeps_curved_regions_through_boolean_transform_and_meshing() {
        let boundary = CurvePath2::try_new(vec![
            Curve2::from(QuadraticBezier2::new(
                point(-2, 4),
                point(0, -4),
                point(2, 4),
            )),
            Curve2::from(LineSeg2::try_new(point(2, 4), point(-2, 4)).unwrap()),
        ])
        .unwrap();
        let curved =
            Shape::CurveRegion2D(CurveRegion2::try_from_boundary_paths(&[boundary]).unwrap());
        let cutter = Shape::CurveRegion2D(csgrs::curve::translated(
            &csgrs::curve::rectangle(Real::from(6), Real::from(3)),
            Real::from(-3),
            Real::from(2),
        ));

        let result = curved
            .difference(cutter)
            .scale(Real::from(2), Real::from(3), Real::one());
        let Shape::CurveRegion2D(region) = result else {
            panic!("higher-order 2D operations should remain a curve region");
        };

        assert!(region.has_algebraic_fragments());
        assert!(
            !csgrs::curve::try_extrude(&region, Real::one(), &csgrs::GeometryContext::STRICT,)
                .unwrap()
                .into_value()
                .triangles
                .is_empty()
        );
    }

    #[test]
    fn exact_2d_transforms_never_invent_3d_thickness() {
        let below_the_old_tolerance = (Real::one() / Real::from(10_000_000_000_000_u64))
            .expect("the exact denominator is nonzero");
        let square = || Shape::CurveRegion2D(csgrs::curve::rectangle(Real::one(), Real::one()));

        assert!(matches!(
            square().translate(Real::zero(), Real::zero(), below_the_old_tolerance.clone()),
            Shape::CurveRegion2D(_)
        ));
        assert!(matches!(
            square().rotate(below_the_old_tolerance, Real::zero(), Real::zero()),
            Shape::CurveRegion2D(_)
        ));
        assert!(matches!(
            square().scale(Real::one(), Real::one(), Real::from(2_u8)),
            Shape::CurveRegion2D(_)
        ));
    }

    #[test]
    fn implicit_2d_and_failed_shape_mesh_conversions_are_rejected() {
        let region = Shape::CurveRegion2D(csgrs::curve::rectangle(Real::one(), Real::one()));
        assert!(
            region
                .try_into_triangle_mesh()
                .unwrap_err()
                .contains("linear_extrude")
        );

        assert_eq!(
            Shape::Failed("retained failure".into())
                .try_into_triangle_mesh()
                .unwrap_err(),
            "retained failure"
        );
    }

    #[test]
    fn exact_boolean_failure_remains_explicit() {
        let points = [
            [Real::zero(), Real::zero(), Real::zero()],
            [Real::one(), Real::zero(), Real::zero()],
            [Real::zero(), Real::one(), Real::zero()],
        ];
        let face = [0_usize, 1, 2];
        let open = solid::polyhedron(&points, &[&face]).unwrap();
        let result = triangle_mesh_boolean(open, solid::cube(Real::from(2_u8)), BoolOp::Difference);
        let Shape::Failed(error) = result else {
            panic!("an open Boolean input must remain an explicit exact failure");
        };
        assert!(error.contains("exact Difference failed"));
        assert!(error.contains("boundary edges"));
    }
}
