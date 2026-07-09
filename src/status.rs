use core::fmt::{self, Write};

/// Writes formatted output into a fixed-size byte buffer, tracking how many
/// bytes have been written so far.
struct BufWriter<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl Write for BufWriter<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let bytes = s.as_bytes();
        let end = self.pos + bytes.len();
        if end > self.buf.len() {
            return Err(fmt::Error);
        }
        self.buf[self.pos..end].copy_from_slice(bytes);
        self.pos = end;
        Ok(())
    }
}

/// Writes `s` into `w`, escaping the characters that would otherwise break
/// JSON string syntax: quotes, backslashes, and ASCII control characters.
fn write_json_escaped(w: &mut impl Write, s: &str) -> fmt::Result {
    for c in s.chars() {
        match c {
            '"' => w.write_str("\\\"")?,
            '\\' => w.write_str("\\\\")?,
            c if (c as u32) < 0x20 => write!(w, "\\u{:04x}", c as u32)?,
            c => w.write_char(c)?,
        }
    }
    Ok(())
}

fn write_status(
    w: &mut BufWriter,
    blink_ms: u32,
    uptime_secs: u64,
    friendly_name: &str,
    unique_name: &str,
) -> fmt::Result {
    write!(
        w,
        r#"{{"blink_ms":{blink_ms},"uptime_secs":{uptime_secs},"friendly_name":""#
    )?;
    write_json_escaped(w, friendly_name)?;
    w.write_str(r#"","unique_name":""#)?;
    write_json_escaped(w, unique_name)?;
    w.write_str(r#""}"#)
}

/// Formats the `/api/status` JSON body into `buf`, escaping `friendly_name`
/// and `unique_name` so the result is always well-formed JSON regardless of
/// their contents.
///
/// Returns the number of bytes written, or `None` if `buf` is too small.
pub fn format_status_json(
    buf: &mut [u8],
    blink_ms: u32,
    uptime_secs: u64,
    friendly_name: &str,
    unique_name: &str,
) -> Option<usize> {
    let mut w = BufWriter { buf, pos: 0 };
    write_status(&mut w, blink_ms, uptime_secs, friendly_name, unique_name).ok()?;
    Some(w.pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn format(blink_ms: u32, uptime_secs: u64, friendly_name: &str, unique_name: &str) -> String {
        let mut buf = [0u8; 512];
        let n = format_status_json(&mut buf, blink_ms, uptime_secs, friendly_name, unique_name)
            .expect("buffer should be large enough");
        String::from_utf8(buf[..n].to_vec()).unwrap()
    }

    #[test]
    fn formats_basic_status() {
        let json = format(500, 42, "helicopter", "helicopter-a1b2c3");
        assert_eq!(
            json,
            r#"{"blink_ms":500,"uptime_secs":42,"friendly_name":"helicopter","unique_name":"helicopter-a1b2c3"}"#
        );
    }

    #[test]
    fn formats_zero_values() {
        let json = format(0, 0, "", "");
        assert_eq!(
            json,
            r#"{"blink_ms":0,"uptime_secs":0,"friendly_name":"","unique_name":""}"#
        );
    }

    #[test]
    fn escapes_double_quote_in_friendly_name() {
        let json = format(500, 1, r#"my"device"#, "unique");
        assert_eq!(
            json,
            r#"{"blink_ms":500,"uptime_secs":1,"friendly_name":"my\"device","unique_name":"unique"}"#
        );
    }

    #[test]
    fn escapes_backslash_in_friendly_name() {
        let json = format(500, 1, r"back\slash", "unique");
        assert_eq!(
            json,
            r#"{"blink_ms":500,"uptime_secs":1,"friendly_name":"back\\slash","unique_name":"unique"}"#
        );
    }

    #[test]
    fn escapes_control_characters() {
        let json = format(500, 1, "a\tb\nc", "unique");
        assert_eq!(
            json,
            "{\"blink_ms\":500,\"uptime_secs\":1,\"friendly_name\":\"a\\u0009b\\u000ac\",\"unique_name\":\"unique\"}"
        );
    }

    #[test]
    fn escapes_quote_in_unique_name() {
        let json = format(500, 1, "friendly", r#"uni"que"#);
        assert_eq!(
            json,
            r#"{"blink_ms":500,"uptime_secs":1,"friendly_name":"friendly","unique_name":"uni\"que"}"#
        );
    }

    #[test]
    fn passes_through_non_ascii_unescaped() {
        let json = format(500, 1, "héli", "unique");
        assert_eq!(
            json,
            r#"{"blink_ms":500,"uptime_secs":1,"friendly_name":"héli","unique_name":"unique"}"#
        );
    }

    #[test]
    fn returns_none_when_buffer_too_small() {
        let mut buf = [0u8; 10];
        assert!(format_status_json(&mut buf, 500, 1, "helicopter", "unique").is_none());
    }

    #[test]
    fn returns_none_without_partial_write_beyond_buffer() {
        // A buffer just one byte short of fitting should still fail cleanly.
        let full_len = format_status_json(&mut [0u8; 512], 500, 1, "helicopter", "unique").unwrap();
        let mut buf = vec![0u8; full_len - 1];
        assert!(format_status_json(&mut buf, 500, 1, "helicopter", "unique").is_none());
    }

    #[test]
    fn max_u32_and_u64_values_format_correctly() {
        let json = format(u32::MAX, u64::MAX, "helicopter", "unique");
        assert_eq!(
            json,
            format!(
                r#"{{"blink_ms":{},"uptime_secs":{},"friendly_name":"helicopter","unique_name":"unique"}}"#,
                u32::MAX,
                u64::MAX
            )
        );
    }
}
