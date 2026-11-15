pub(crate) fn validate_version(version: &str) -> bool {
    if version.len() != 20 {
        return false;
    }
    for byte in version.bytes() {
        if !byte.is_ascii_digit() {
            return false;
        }
    }

    let year = i32::try_from(parse_digits(&version[0..4])).unwrap_or(0);
    let month = parse_digits(&version[4..6]);
    let day = parse_digits(&version[6..8]);
    let hour = parse_digits(&version[8..10]);
    let minute = parse_digits(&version[10..12]);
    let second = parse_digits(&version[12..14]);
    if year < 1 || !(1..=12).contains(&month) || hour > 23 || minute > 59 || second > 59 {
        return false;
    }
    day >= 1 && day <= days_in_month(year, month)
}

fn parse_digits(text: &str) -> u32 {
    let mut value = 0u32;
    for byte in text.bytes() {
        value = value * 10 + u32::from(byte - b'0');
    }
    value
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

fn is_leap(year: i32) -> bool {
    let divisible_by_4 = year % 4 == 0;
    let divisible_by_100 = year % 100 == 0;
    let divisible_by_400 = year % 400 == 0;
    (divisible_by_4 && !divisible_by_100) || divisible_by_400
}

#[cfg(test)]
mod tests {
    use super::validate_version;

    #[test]
    fn accepts_a_real_utc_timestamp() {
        assert!(validate_version("20261115000000000000"));
        assert!(validate_version("20240229010101000001"));
    }

    #[test]
    fn rejects_a_timestamp_that_is_not_a_real_utc_moment() {
        assert!(!validate_version("20260230000000000000"));
        assert!(!validate_version("20230229000000000000"));
        assert!(!validate_version("2026111500000000000"));
        assert!(!validate_version("20261115000060000000"));
        assert!(!validate_version("20261115250000000000"));
        assert!(!validate_version("not-a-timestamp-value"));
    }
}
