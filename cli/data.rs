use std::fs;
use std::path::Path;

use palmscript::Bar;

pub fn load_bars_csv(path: &Path) -> Result<Vec<Bar>, String> {
    let raw = fs::read_to_string(path)
        .map_err(|err| format!("failed to read `{}`: {err}", path.display()))?;
    let mut lines = raw.lines();
    let Some(header) = lines.next() else {
        return Err(format!("`{}` is empty", path.display()));
    };
    if header.trim() != "time,open,high,low,close,volume" {
        return Err(format!(
            "`{}` must have header `time,open,high,low,close,volume`",
            path.display()
        ));
    }

    let mut bars = Vec::new();
    for (line_index, line) in lines.enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let fields: Vec<&str> = trimmed.split(',').collect();
        if fields.len() != 6 {
            return Err(format!(
                "`{}` line {} must contain 6 comma-separated fields",
                path.display(),
                line_index + 2
            ));
        }
        let time = parse_lossless_ms(fields[0], path, line_index + 2)?;
        let open = parse_f64(fields[1], path, line_index + 2, "open")?;
        let high = parse_f64(fields[2], path, line_index + 2, "high")?;
        let low = parse_f64(fields[3], path, line_index + 2, "low")?;
        let close = parse_f64(fields[4], path, line_index + 2, "close")?;
        let volume = parse_f64(fields[5], path, line_index + 2, "volume")?;
        bars.push(Bar {
            open,
            high,
            low,
            close,
            volume,
            time: time as f64,
        });
    }

    Ok(bars)
}

fn parse_lossless_ms(raw: &str, path: &Path, line: usize) -> Result<i64, String> {
    raw.parse::<i64>().map_err(|err| {
        format!(
            "`{}` line {} has invalid `time` value `{raw}`: {err}",
            path.display(),
            line
        )
    })
}

fn parse_f64(raw: &str, path: &Path, line: usize, field: &str) -> Result<f64, String> {
    raw.parse::<f64>().map_err(|err| {
        format!(
            "`{}` line {} has invalid `{field}` value `{raw}`: {err}",
            path.display(),
            line
        )
    })
}

#[cfg(test)]
mod tests {
    use super::load_bars_csv;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn load_bars_csv_rejects_bad_header_and_invalid_values() {
        let dir = tempdir().expect("tempdir");

        let bad_header = dir.path().join("bad-header.csv");
        fs::write(&bad_header, "foo,bar\n").expect("write header");
        assert!(load_bars_csv(&bad_header)
            .expect_err("header should fail")
            .contains("must have header"));

        let bad_value = dir.path().join("bad-value.csv");
        fs::write(
            &bad_value,
            "time,open,high,low,close,volume\n1704067200000,1,2,0.5,nope,10\n",
        )
        .expect("write value");
        let err = load_bars_csv(&bad_value).expect_err("invalid close should fail");
        assert!(err.contains("invalid `close` value `nope`"));
    }

    #[test]
    fn load_bars_csv_parses_valid_rows() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("bars.csv");
        fs::write(
            &path,
            "time,open,high,low,close,volume\n1704067200000,1,2,0.5,1.5,10\n",
        )
        .expect("write csv");
        let bars = load_bars_csv(&path).expect("bars load");
        assert_eq!(bars.len(), 1);
        assert_eq!(bars[0].time, 1704067200000.0);
        assert_eq!(bars[0].close, 1.5);
    }
}
