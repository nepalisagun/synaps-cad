#![no_main]

mod support;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some(source) = support::structured_geometry_program(data) else {
        return;
    };
    openscad_rs::parse(&source).unwrap_or_else(|error| {
        panic!("generated geometry source did not parse: {error}\n{source}")
    });
    support::evaluate_source(&source, 8);
});
