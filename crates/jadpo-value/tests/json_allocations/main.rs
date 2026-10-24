//! Large ordinary JSON strings need one output allocation, independent of length.

use std::alloc::System;

use jadpo_value::json::write_string;
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static COUNTED: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

fn main() {
    // Build inputs outside the region; count only the public writer's output.
    for text in ["x".repeat(65_536), "é🙂".repeat(16_384)] {
        for prefix in ["", "already written: "] {
            let mut out = prefix.to_owned();
            let region = Region::new(COUNTED);
            write_string(&text, &mut out);
            let change = region.change();
            assert_eq!(&out[..prefix.len()], prefix);
            assert_eq!(&out[prefix.len() + 1..out.len() - 1], text);
            assert_eq!(out.len(), prefix.len() + text.len() + 2);
            assert!(
                change.allocations + change.reallocations <= 1,
                "a large ordinary JSON string grew its output repeatedly: {change:?}"
            );
        }
    }
    println!("large ordinary JSON strings use at most one output allocation");
}
