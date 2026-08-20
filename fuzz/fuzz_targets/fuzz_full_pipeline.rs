#![no_main]

mod support;

use libfuzzer_sys::fuzz_target;
use synaps_cad::compiler::{CompilationResult, compile_scad_code};

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let mut cursor = support::ByteCursor::new(data);
    let source = match cursor.choice(4) {
        0 => format!(
            "cube([{}, {}, {}]);",
            cursor.positive(),
            cursor.positive(),
            cursor.positive()
        ),
        1 => format!(
            "sphere(r={}, $fn={});",
            cursor.positive(),
            cursor.positive() + 2
        ),
        2 => format!(
            "linear_extrude(height={}) square([{}, {}]);",
            cursor.positive(),
            cursor.positive(),
            cursor.positive()
        ),
        _ => {
            "difference() { cube(4, center=true); translate([1,1,1]) cube(4, center=true); }".into()
        }
    };
    match compile_scad_code(&source, u32::from(cursor.positive() + 2), None) {
        CompilationResult::Success {
            parts,
            views,
            warnings: _,
        } => {
            for part in &parts {
                support::assert_render_mesh(part);
            }
            if !parts.is_empty() {
                assert!(!views.is_empty());
                assert!(
                    views
                        .iter()
                        .all(|view| !view.label.is_empty() && !view.base64_png.is_empty())
                );
            }
        }
        CompilationResult::Error(error) => {
            panic!("generated full-pipeline source failed: {error}\n{source}")
        }
        CompilationResult::Canceled => panic!("uncanceled compilation returned Canceled"),
    }
});
