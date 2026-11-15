#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SqlReject {
    Empty,
    Unterminated,
    Forbidden { keyword: String },
}

pub(crate) fn check_migration_sql(sql: &str) -> Result<(), SqlReject> {
    if sql.as_bytes().contains(&0) {
        return Err(SqlReject::Forbidden {
            keyword: "nul".to_owned(),
        });
    }

    let keywords = statement_keywords(sql)?;
    for keyword in keywords {
        if is_forbidden(&keyword) {
            return Err(SqlReject::Forbidden { keyword });
        }
    }
    Ok(())
}

fn is_forbidden(keyword: &str) -> bool {
    matches!(
        keyword,
        "begin"
            | "commit"
            | "rollback"
            | "end"
            | "savepoint"
            | "release"
            | "attach"
            | "detach"
            | "vacuum"
    )
}

enum Mode {
    Normal,
    LineComment,
    BlockComment,
    SingleQuote,
    DoubleQuote,
    Backtick,
    Bracket,
}

fn statement_keywords(sql: &str) -> Result<Vec<String>, SqlReject> {
    let chars: Vec<char> = sql.chars().collect();
    let mut mode = Mode::Normal;
    let mut keywords = Vec::new();
    let mut keyword = String::new();
    let mut taking_keyword = true;
    let mut statement_has_content = false;
    let mut saw_statement = false;
    let mut paren_depth = 0u32;
    let mut block_depth = 0u32;
    let mut index = 0;

    while index < chars.len() {
        let ch = chars[index];
        match mode {
            Mode::LineComment => {
                if ch == '\n' {
                    mode = Mode::Normal;
                }
                index += 1;
            }
            Mode::BlockComment => {
                if ch == '*' && chars.get(index + 1) == Some(&'/') {
                    mode = Mode::Normal;
                    index += 2;
                } else {
                    index += 1;
                }
            }
            Mode::SingleQuote => {
                statement_has_content = true;
                taking_keyword = false;
                if ch == '\'' {
                    if chars.get(index + 1) == Some(&'\'') {
                        index += 2;
                    } else {
                        mode = Mode::Normal;
                        index += 1;
                    }
                } else {
                    index += 1;
                }
            }
            Mode::DoubleQuote => {
                statement_has_content = true;
                taking_keyword = false;
                if ch == '"' {
                    if chars.get(index + 1) == Some(&'"') {
                        index += 2;
                    } else {
                        mode = Mode::Normal;
                        index += 1;
                    }
                } else {
                    index += 1;
                }
            }
            Mode::Backtick => {
                statement_has_content = true;
                taking_keyword = false;
                if ch == '`' {
                    mode = Mode::Normal;
                }
                index += 1;
            }
            Mode::Bracket => {
                statement_has_content = true;
                taking_keyword = false;
                if ch == ']' {
                    mode = Mode::Normal;
                }
                index += 1;
            }
            Mode::Normal => {
                if ch == '-' && chars.get(index + 1) == Some(&'-') {
                    mode = Mode::LineComment;
                    index += 2;
                    continue;
                }
                if ch == '/' && chars.get(index + 1) == Some(&'*') {
                    mode = Mode::BlockComment;
                    index += 2;
                    continue;
                }
                if ch == '\'' {
                    mode = Mode::SingleQuote;
                    statement_has_content = true;
                    taking_keyword = false;
                    index += 1;
                    continue;
                }
                if ch == '"' {
                    mode = Mode::DoubleQuote;
                    statement_has_content = true;
                    taking_keyword = false;
                    index += 1;
                    continue;
                }
                if ch == '`' {
                    mode = Mode::Backtick;
                    statement_has_content = true;
                    taking_keyword = false;
                    index += 1;
                    continue;
                }
                if ch == '[' {
                    mode = Mode::Bracket;
                    statement_has_content = true;
                    taking_keyword = false;
                    index += 1;
                    continue;
                }
                if ch == '(' {
                    paren_depth += 1;
                    statement_has_content = true;
                    taking_keyword = false;
                    index += 1;
                    continue;
                }
                if ch == ')' {
                    paren_depth = paren_depth.saturating_sub(1);
                    statement_has_content = true;
                    taking_keyword = false;
                    index += 1;
                    continue;
                }
                if ch == ';' && paren_depth == 0 && block_depth == 0 {
                    finish_statement(
                        &mut keyword,
                        &mut keywords,
                        &mut taking_keyword,
                        &mut statement_has_content,
                        &mut saw_statement,
                    );
                    index += 1;
                    continue;
                }
                if ch.is_ascii_whitespace() {
                    if !keyword.is_empty() {
                        taking_keyword = false;
                    }
                    index += 1;
                    continue;
                }
                statement_has_content = true;
                if taking_keyword && paren_depth == 0 && (ch.is_ascii_alphanumeric() || ch == '_') {
                    keyword.push(ch);
                    index += 1;
                    continue;
                }
                let at_word = is_word_boundary(chars.get(index.wrapping_sub(1)).copied());
                if at_word
                    && (starts_word(&chars, index, "begin") || starts_word(&chars, index, "case"))
                {
                    block_depth += 1;
                } else if at_word && starts_word(&chars, index, "end") && block_depth > 0 {
                    block_depth = block_depth.saturating_sub(1);
                }
                taking_keyword = false;
                index += 1;
            }
        }
    }

    if !matches!(mode, Mode::Normal | Mode::LineComment) {
        return Err(SqlReject::Unterminated);
    }
    if paren_depth == 0 && block_depth == 0 {
        finish_statement(
            &mut keyword,
            &mut keywords,
            &mut taking_keyword,
            &mut statement_has_content,
            &mut saw_statement,
        );
    } else if statement_has_content || !keyword.is_empty() {
        return Err(SqlReject::Unterminated);
    }
    if !saw_statement {
        return Err(SqlReject::Empty);
    }
    Ok(keywords)
}

fn finish_statement(
    keyword: &mut String,
    keywords: &mut Vec<String>,
    taking_keyword: &mut bool,
    statement_has_content: &mut bool,
    saw_statement: &mut bool,
) {
    if !keyword.is_empty() {
        keywords.push(keyword.to_ascii_lowercase());
        keyword.clear();
    }
    if *statement_has_content {
        *saw_statement = true;
    }
    *taking_keyword = true;
    *statement_has_content = false;
}

fn is_word_boundary(previous: Option<char>) -> bool {
    !matches!(previous, Some(ch) if ch.is_ascii_alphanumeric() || ch == '_')
}

fn starts_word(chars: &[char], index: usize, word: &str) -> bool {
    let letters: Vec<char> = word.chars().collect();
    if index + letters.len() > chars.len() {
        return false;
    }
    for (offset, expected) in letters.iter().enumerate() {
        if chars[index + offset].to_ascii_lowercase() != *expected {
            return false;
        }
    }
    is_word_boundary(chars.get(index + letters.len()).copied())
        || chars.get(index + letters.len()).is_none()
}

#[cfg(test)]
mod tests {
    use super::{SqlReject, check_migration_sql};

    #[test]
    fn accepts_ordinary_sql_and_a_trigger_body() {
        let sql = "CREATE TABLE note (id INTEGER PRIMARY KEY, body TEXT DEFAULT 'commit');\n\
                   CREATE TRIGGER note_touch AFTER UPDATE ON note BEGIN SELECT 1; END;";
        assert!(check_migration_sql(sql).is_ok());
    }

    #[test]
    fn rejects_transaction_control_outside_strings_and_comments() {
        let rejected = check_migration_sql("CREATE TABLE note (id INTEGER); COMMIT;").unwrap_err();
        assert_eq!(
            rejected,
            SqlReject::Forbidden {
                keyword: "commit".to_owned(),
            }
        );
        assert!(check_migration_sql("-- commit\nCREATE TABLE note (id INTEGER);").is_ok());
        assert!(check_migration_sql("/* begin */ CREATE TABLE note (id INTEGER);").is_ok());
    }

    #[test]
    fn rejects_empty_and_attach() {
        assert_eq!(
            check_migration_sql("  -- nothing\n").unwrap_err(),
            SqlReject::Empty
        );
        let rejected = check_migration_sql("ATTACH DATABASE 'other.sqlite' AS other;").unwrap_err();
        assert_eq!(
            rejected,
            SqlReject::Forbidden {
                keyword: "attach".to_owned(),
            }
        );
    }
}
