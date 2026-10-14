//! Shared by the CLI's snapshot tests.

/// A slow machine adds the run's duration to the footer (` in 0.1 s`);
/// snapshots compare the output without it.
pub fn without_run_time(text: &str) -> String {
    text.split_inclusive('\n')
        .map(|line| {
            let (body, end) = line
                .strip_suffix('\n')
                .map_or((line, ""), |body| (body, "\n"));
            match body.rsplit_once(" in ") {
                Some((before, time))
                    if time
                        .strip_suffix(" s")
                        .is_some_and(|secs| secs.parse::<f64>().is_ok()) =>
                {
                    format!("{before}{end}")
                }
                _ => line.to_string(),
            }
        })
        .collect()
}
