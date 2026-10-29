use super::utc_minutes;

#[test]
fn known_moments_are_formatted_as_utc_to_the_minute() {
    // The expected strings are what `date -u -d @N '+%Y-%m-%d %H:%M UTC'` prints.
    for (secs, want) in [
        (0, "1970-01-01 00:00 UTC"),
        (86_399, "1970-01-01 23:59 UTC"),
        (951_782_400, "2000-02-29 00:00 UTC"),
        (1_700_000_000, "2023-11-14 22:13 UTC"),
        (1_782_039_780, "2026-06-21 11:03 UTC"),
        (4_102_444_800, "2100-01-01 00:00 UTC"),
        (-1, "1969-12-31 23:59 UTC"),
    ] {
        assert_eq!(utc_minutes(secs), want, "{secs}");
    }
}
