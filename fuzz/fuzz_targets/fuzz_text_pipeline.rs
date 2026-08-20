#![no_main]

mod support;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let mut cursor = support::ByteCursor::new(data);
    let length = cursor.choice(4) + 1;
    let mut text = String::new();
    for _ in 0..length {
        let character = char::from(cursor.byte() % 95 + 32);
        match character {
            '"' => text.push_str("\\\""),
            '\\' => text.push_str("\\\\"),
            _ => text.push(character),
        }
    }
    let directions = ["ltr", "rtl", "ttb", "btt"];
    let horizontal = ["left", "center", "right"];
    let vertical = ["baseline", "top", "center", "bottom"];
    let source = format!(
        "linear_extrude(height={}) text(\"{}\", size={}, spacing={}/{}, direction=\"{}\", halign=\"{}\", valign=\"{}\");",
        cursor.positive(),
        text,
        cursor.positive(),
        cursor.positive(),
        cursor.positive(),
        directions[cursor.choice(directions.len())],
        horizontal[cursor.choice(horizontal.len())],
        vertical[cursor.choice(vertical.len())],
    );
    support::evaluate_source(&source, 4);
});
