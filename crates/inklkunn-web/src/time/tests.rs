use super::{http_date, rfc3339, utc_minutes};

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

#[test]
fn feed_and_header_dates_are_the_standard_forms() {
    // `date -u -d @N '+%Y-%m-%dT%H:%M:%SZ'` and `date -u -d @N '+%a, %d %b %Y %H:%M:%S GMT'`.
    for (secs, iso, http) in [
        (0, "1970-01-01T00:00:00Z", "Thu, 01 Jan 1970 00:00:00 GMT"),
        (
            784_111_777,
            "1994-11-06T08:49:37Z",
            "Sun, 06 Nov 1994 08:49:37 GMT",
        ),
        (
            951_782_400,
            "2000-02-29T00:00:00Z",
            "Tue, 29 Feb 2000 00:00:00 GMT",
        ),
        (
            1_700_000_000,
            "2023-11-14T22:13:20Z",
            "Tue, 14 Nov 2023 22:13:20 GMT",
        ),
        (
            4_102_444_800,
            "2100-01-01T00:00:00Z",
            "Fri, 01 Jan 2100 00:00:00 GMT",
        ),
    ] {
        assert_eq!(rfc3339(secs), iso, "{secs}");
        assert_eq!(http_date(secs), http, "{secs}");
    }
}
