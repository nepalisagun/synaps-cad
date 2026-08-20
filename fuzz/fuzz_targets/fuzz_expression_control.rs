#![no_main]

mod support;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let mut cursor = support::ByteCursor::new(data);
    let depth = usize::from(cursor.byte() % 4) + 1;
    let expression = support::expression(&mut cursor, depth);
    let source = format!(
        "function twice(v) = v * 2;\n\
         x = {};\n\
         values = [for (i = [0:3]) let (v = twice(i)) if (v != 4) each [v, v + 1]];\n\
         assertion = assert(len(values) >= 0, \"bounded\") true;\n\
         if (assertion && bool({expression})) cube([1, 1, 1]); else sphere(1, $fn=4);\n",
        cursor.integer()
    );
    openscad_rs::parse(&source).unwrap_or_else(|error| {
        panic!("generated expression source did not parse: {error}\n{source}")
    });
    support::evaluate_source(&source, 6);
});
