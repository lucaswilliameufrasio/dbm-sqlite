use dbm_sqlite::Migration;
use rusqlite::types::{ToSql, ToSqlOutput, Value, ValueRef};

#[derive(Debug)]
pub(crate) struct ParsedMigration {
    pub(crate) version: String,
    pub(crate) sql: String,
}

pub(crate) enum Arg {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl ToSql for Arg {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        match self {
            Self::Null => Ok(ToSqlOutput::Owned(Value::Null)),
            Self::Integer(value) => Ok(ToSqlOutput::Owned(Value::Integer(*value))),
            Self::Real(value) => Ok(ToSqlOutput::Owned(Value::Real(*value))),
            Self::Text(value) => Ok(ToSqlOutput::Owned(Value::Text(value.clone()))),
            Self::Blob(value) => Ok(ToSqlOutput::Owned(Value::Blob(value.clone()))),
        }
    }
}

pub(crate) fn migrations_from(bytes: &[u8]) -> Result<Vec<ParsedMigration>, String> {
    let mut offset = 0;
    let count = read_u32(bytes, &mut offset)?;
    let mut migrations = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let version = read_text(bytes, &mut offset)?;
        let sql = read_bytes(bytes, &mut offset)?;
        let sql = String::from_utf8(sql).map_err(|_| "migration SQL is not utf-8".to_string())?;
        migrations.push(ParsedMigration { version, sql });
    }
    if offset != bytes.len() {
        return Err("migration blob has trailing bytes".to_string());
    }
    Ok(migrations)
}

pub(crate) fn migration_refs(migrations: &[ParsedMigration]) -> Vec<Migration<'_>> {
    let mut refs = Vec::with_capacity(migrations.len());
    for migration in migrations {
        refs.push(Migration {
            version: &migration.version,
            sql: &migration.sql,
        });
    }
    refs
}

pub(crate) fn args_from(bytes: &[u8]) -> Result<Vec<Arg>, String> {
    let mut offset = 0;
    let count = read_u32(bytes, &mut offset)?;
    let mut args = Vec::with_capacity(count as usize);
    for _ in 0..count {
        if offset >= bytes.len() {
            return Err("argument blob is truncated".to_string());
        }
        let tag = bytes[offset];
        offset += 1;
        let arg = match tag {
            0 => Arg::Null,
            1 => Arg::Integer(read_i64(bytes, &mut offset)?),
            2 => Arg::Real(read_f64(bytes, &mut offset)?),
            3 => Arg::Text(read_text(bytes, &mut offset)?),
            4 => Arg::Blob(read_bytes(bytes, &mut offset)?),
            _ => return Err(format!("unknown argument tag {tag}")),
        };
        args.push(arg);
    }
    if offset != bytes.len() {
        return Err("argument blob has trailing bytes".to_string());
    }
    Ok(args)
}

pub(crate) fn encode_row_json(
    columns: &[String],
    rows: &[Vec<ValueRefOwned>],
) -> Result<String, String> {
    let mut out = String::from("{\"columns\":[");
    for (index, column) in columns.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        push_json_string(&mut out, column);
    }
    out.push_str("],\"rows\":[");
    for (row_index, row) in rows.iter().enumerate() {
        if row_index > 0 {
            out.push(',');
        }
        out.push('[');
        for (value_index, value) in row.iter().enumerate() {
            if value_index > 0 {
                out.push(',');
            }
            push_value(&mut out, value)?;
        }
        out.push(']');
    }
    out.push_str("]}");
    Ok(out)
}

pub(crate) enum ValueRefOwned {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl ValueRefOwned {
    pub(crate) fn from_sql(value: ValueRef<'_>) -> Result<Self, String> {
        match value {
            ValueRef::Null => Ok(Self::Null),
            ValueRef::Integer(value) => Ok(Self::Integer(value)),
            ValueRef::Real(value) => {
                if !value.is_finite() {
                    return Err("query value is not a finite number".to_string());
                }
                Ok(Self::Real(value))
            }
            ValueRef::Text(bytes) => {
                let text = std::str::from_utf8(bytes)
                    .map_err(|_| "query text is not utf-8".to_string())?;
                Ok(Self::Text(text.to_string()))
            }
            ValueRef::Blob(bytes) => Ok(Self::Blob(bytes.to_vec())),
        }
    }
}

fn push_value(out: &mut String, value: &ValueRefOwned) -> Result<(), String> {
    match value {
        ValueRefOwned::Null => out.push_str("{\"t\":\"n\"}"),
        ValueRefOwned::Integer(value) => {
            out.push_str("{\"t\":\"i\",\"v\":");
            out.push_str(&value.to_string());
            out.push('}');
        }
        ValueRefOwned::Real(value) => {
            out.push_str("{\"t\":\"f\",\"v\":");
            out.push_str(&value.to_string());
            out.push('}');
        }
        ValueRefOwned::Text(value) => {
            out.push_str("{\"t\":\"s\",\"v\":");
            push_json_string(out, value);
            out.push('}');
        }
        ValueRefOwned::Blob(value) => {
            out.push_str("{\"t\":\"b\",\"v\":");
            push_json_string(out, &base64(value));
            out.push('}');
        }
    }
    Ok(())
}

fn push_json_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if (ch as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", ch as u32));
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut index = 0;
    while index + 3 <= bytes.len() {
        let chunk = ((bytes[index] as u32) << 16)
            | ((bytes[index + 1] as u32) << 8)
            | (bytes[index + 2] as u32);
        out.push(TABLE[((chunk >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((chunk >> 12) & 0x3f) as usize] as char);
        out.push(TABLE[((chunk >> 6) & 0x3f) as usize] as char);
        out.push(TABLE[(chunk & 0x3f) as usize] as char);
        index += 3;
    }
    let rest = bytes.len() - index;
    if rest == 1 {
        let chunk = (bytes[index] as u32) << 16;
        out.push(TABLE[((chunk >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((chunk >> 12) & 0x3f) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rest == 2 {
        let chunk = ((bytes[index] as u32) << 16) | ((bytes[index + 1] as u32) << 8);
        out.push(TABLE[((chunk >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((chunk >> 12) & 0x3f) as usize] as char);
        out.push(TABLE[((chunk >> 6) & 0x3f) as usize] as char);
        out.push('=');
    }
    out
}

fn read_u32(bytes: &[u8], offset: &mut usize) -> Result<u32, String> {
    let mut buf = [0u8; 4];
    copy_exact(bytes, offset, &mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_i64(bytes: &[u8], offset: &mut usize) -> Result<i64, String> {
    let mut buf = [0u8; 8];
    copy_exact(bytes, offset, &mut buf)?;
    Ok(i64::from_le_bytes(buf))
}

fn read_f64(bytes: &[u8], offset: &mut usize) -> Result<f64, String> {
    let mut buf = [0u8; 8];
    copy_exact(bytes, offset, &mut buf)?;
    Ok(f64::from_le_bytes(buf))
}

fn read_text(bytes: &[u8], offset: &mut usize) -> Result<String, String> {
    let raw = read_bytes(bytes, offset)?;
    String::from_utf8(raw).map_err(|_| "blob text is not utf-8".to_string())
}

fn read_bytes(bytes: &[u8], offset: &mut usize) -> Result<Vec<u8>, String> {
    let len = read_u32(bytes, offset)? as usize;
    let end = offset
        .checked_add(len)
        .ok_or_else(|| "blob length overflows".to_string())?;
    if end > bytes.len() {
        return Err("blob is truncated".to_string());
    }
    let slice = bytes[*offset..end].to_vec();
    *offset = end;
    Ok(slice)
}

fn copy_exact(bytes: &[u8], offset: &mut usize, dest: &mut [u8]) -> Result<(), String> {
    let end = offset
        .checked_add(dest.len())
        .ok_or_else(|| "blob offset overflows".to_string())?;
    if end > bytes.len() {
        return Err("blob is truncated".to_string());
    }
    dest.copy_from_slice(&bytes[*offset..end]);
    *offset = end;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Arg, args_from, migrations_from};

    #[test]
    fn migration_fixture_matches_the_dart_encoder() {
        let bytes = [
            1, 0, 0, 0, 20, 0, 0, 0, b'2', b'0', b'2', b'6', b'1', b'1', b'1', b'5', b'0', b'0',
            b'0', b'0', b'0', b'0', b'0', b'0', b'0', b'0', b'0', b'1', 9, 0, 0, 0, b'S', b'E',
            b'L', b'E', b'C', b'T', b' ', b'1', b';',
        ];
        let parsed = migrations_from(&bytes).expect("parse fixture");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].version, "20261115000000000001");
        assert_eq!(parsed[0].sql, "SELECT 1;");
    }

    #[test]
    fn args_round_trip_tags() {
        let mut bytes = vec![2, 0, 0, 0];
        bytes.push(1);
        bytes.extend_from_slice(&7i64.to_le_bytes());
        bytes.push(0);
        let parsed = args_from(&bytes).expect("parse args");
        assert!(matches!(parsed[0], Arg::Integer(7)));
        assert!(matches!(parsed[1], Arg::Null));
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut bytes = vec![0, 0, 0, 0];
        bytes.push(1);
        let error = migrations_from(&bytes).expect_err("trailing");
        assert!(error.contains("trailing"));
    }
}
