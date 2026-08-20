#![no_main]

mod support;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some(source) = support::bounded_arbitrary_source(data) else {
        return;
    };
    support::evaluate_source(&source, 8);
});
