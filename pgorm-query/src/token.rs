//! Tokenizer for processing SQL.

use std::fmt::Write;
use std::iter::Iterator;

// [spec:pgorm:def:sql.token]
// [spec:pgorm:sem:sql.token.limits+1]
#[derive(Debug, Default)]
pub struct Tokenizer {
    dollar_quotes: bool,
    pub chars: Vec<char>,
    pub p: usize,
}

// [spec:pgorm:def:sql.token] (the four token classes)
#[derive(Debug, PartialEq, Eq)]
pub enum Token {
    Quoted(String),
    Unquoted(String),
    Space(String),
    Punctuation(String),
}

impl Tokenizer {
    pub fn new(string: &str) -> Self {
        Self {
            dollar_quotes: true,
            chars: string.chars().collect(),
            p: 0,
        }
    }

    /// A tokenizer that does not read `$tag$ … $tag$` as a quoted body, for
    /// input where `$` spellings carry their own grammar — the placeholder
    /// template's `$$` escape (`sql.render.custom-expr`).
    pub fn new_without_dollar_quoting(string: &str) -> Self {
        Self {
            dollar_quotes: false,
            ..Self::new(string)
        }
    }

    pub fn iter(self) -> impl Iterator<Item = Token> {
        self
    }

    fn get(&self) -> char {
        self.chars[self.p]
    }

    fn inc(&mut self) {
        self.p += 1;
    }

    fn end(&self) -> bool {
        self.p == self.chars.len()
    }

    fn peek(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.p + ahead).copied()
    }

    /// A `--` line comment or a nested `/* */` block comment, lexed as one
    /// [`Token::Space`]: a comment separates tokens exactly as whitespace
    /// does, and folding it in keeps everything inside it — a `$N` spelling
    /// included — out of every other token form.
    // [spec:pgorm:req:sql.token.space+1]
    fn comment(&mut self) -> Option<Token> {
        let mut string = String::new();
        if self.peek(0) == Some('-') && self.peek(1) == Some('-') {
            while !self.end() {
                let c = self.get();
                write!(string, "{c}").unwrap();
                self.inc();
                if c == '\n' {
                    break;
                }
            }
            return Some(Token::Space(string));
        }
        if self.peek(0) == Some('/') && self.peek(1) == Some('*') {
            let mut depth = 0usize;
            while !self.end() {
                if self.peek(0) == Some('/') && self.peek(1) == Some('*') {
                    depth += 1;
                    string.push_str("/*");
                    self.inc();
                    self.inc();
                } else if self.peek(0) == Some('*') && self.peek(1) == Some('/') {
                    depth = depth.saturating_sub(1);
                    string.push_str("*/");
                    self.inc();
                    self.inc();
                    if depth == 0 {
                        break;
                    }
                } else {
                    write!(string, "{}", self.get()).unwrap();
                    self.inc();
                }
            }
            return Some(Token::Space(string));
        }
        None
    }

    /// A dollar-quoted string, `$tag$ … $tag$`, lexed whole as one
    /// [`Token::Quoted`]. A digit after the opening `$` is a placeholder
    /// spelling, never a tag, so `$1` stays punctuation; an unclosed body
    /// runs to the end of the input, reproduced verbatim.
    // [spec:pgorm:req:sql.token.quoted+1]
    fn dollar_quoted(&mut self) -> Option<Token> {
        if !self.dollar_quotes || self.peek(0) != Some('$') {
            return None;
        }
        let mut tag_len = 0usize;
        loop {
            match self.peek(1 + tag_len) {
                Some('$') => break,
                Some(c) if c.is_alphabetic() || c == '_' || (tag_len > 0 && c.is_ascii_digit()) => {
                    tag_len += 1;
                }
                _ => return None,
            }
        }
        let opener: String = self.chars[self.p..=self.p + tag_len + 1].iter().collect();
        let mut string = opener.clone();
        for _ in 0..opener.len() {
            self.inc();
        }
        while !self.end() {
            let closes =
                (0..opener.len()).all(|ahead| self.peek(ahead) == opener.chars().nth(ahead));
            if closes {
                string.push_str(&opener);
                for _ in 0..opener.len() {
                    self.inc();
                }
                break;
            }
            write!(string, "{}", self.get()).unwrap();
            self.inc();
        }
        Some(Token::Quoted(string))
    }

    /// An escape string, `E'…'`, lexed whole as one [`Token::Quoted`]: inside
    /// it a backslash escapes the next character — the closing quote included
    /// — and `''` doubling still continues the body.
    // [spec:pgorm:req:sql.token.quoted+1]
    fn e_string(&mut self) -> Option<Token> {
        if !matches!(self.peek(0), Some('E' | 'e')) || self.peek(1) != Some('\'') {
            return None;
        }
        let mut string = String::new();
        write!(string, "{}", self.get()).unwrap();
        self.inc();
        write!(string, "{}", self.get()).unwrap();
        self.inc();
        let mut escape = false;
        while !self.end() {
            let c = self.get();
            write!(string, "{c}").unwrap();
            self.inc();
            if escape {
                escape = false;
                continue;
            }
            if c == '\\' {
                escape = true;
                continue;
            }
            if c == '\'' {
                if self.peek(0) == Some('\'') {
                    write!(string, "'").unwrap();
                    self.inc();
                    continue;
                }
                break;
            }
        }
        Some(Token::Quoted(string))
    }

    // [spec:pgorm:req:sql.token.space+1]
    fn space(&mut self) -> Option<Token> {
        let mut string = String::new();
        while !self.end() {
            let c = self.get();
            if Self::is_space(c) {
                write!(string, "{c}").unwrap();
            } else {
                break;
            }
            self.inc();
        }
        if !string.is_empty() {
            Some(Token::Space(string))
        } else {
            None
        }
    }

    // [spec:pgorm:req:sql.token.word]
    fn unquoted(&mut self) -> Option<Token> {
        let mut string = String::new();
        let mut first = true;
        while !self.end() {
            let c = self.get();
            if Self::is_alphanumeric(c) {
                write!(string, "{c}").unwrap();
                first = false;
                self.inc();
            } else if !first && Self::is_identifier(c) {
                write!(string, "{c}").unwrap();
                self.inc();
            } else {
                break;
            }
        }
        if !string.is_empty() {
            Some(Token::Unquoted(string))
        } else {
            None
        }
    }

    // [spec:pgorm:req:sql.token.quoted+1]
    fn quoted(&mut self) -> Option<Token> {
        let mut string = String::new();
        let mut first = true;
        let mut escape = false;
        let mut start = ' ';
        while !self.end() {
            let c = self.get();
            if first && Self::is_string_delimiter_start(c) {
                write!(string, "{c}").unwrap();
                first = false;
                start = c;
                self.inc();
            } else if !first && !escape && Self::is_string_delimiter_end_for(start, c) {
                write!(string, "{c}").unwrap();
                self.inc();
                if self.end() {
                    break;
                }
                if !Self::is_string_escape_for(start, self.get()) {
                    break;
                } else {
                    write!(string, "{}", self.get()).unwrap();
                    self.inc();
                }
            } else if !first {
                escape = !escape && Self::is_escape_char(c);
                write!(string, "{c}").unwrap();
                self.inc();
            } else {
                break;
            }
        }
        if !string.is_empty() {
            Some(Token::Quoted(string))
        } else {
            None
        }
    }

    /// unquote a quoted string
    // [spec:pgorm:sem:sql.token.unquote]
    fn unquote(mut self) -> String {
        let mut string = String::new();
        let mut first = true;
        let mut escape = false;
        let mut start = ' ';
        while !self.end() {
            let c = self.get();
            if first && Self::is_string_delimiter_start(c) {
                first = false;
                start = c;
                self.inc();
            } else if !first && !escape && Self::is_string_delimiter_end_for(start, c) {
                self.inc();
                if self.end() {
                    break;
                }
                if !Self::is_string_escape_for(start, self.get()) {
                    break;
                } else {
                    write!(string, "{c}").unwrap();
                    self.inc();
                }
            } else if !first {
                escape = !escape && Self::is_escape_char(c);
                write!(string, "{c}").unwrap();
                self.inc();
            } else {
                break;
            }
        }
        string
    }

    // [spec:pgorm:req:sql.token.scan] (single-character punctuation fallback)
    fn punctuation(&mut self) -> Option<Token> {
        let mut string = String::new();
        if !self.end() {
            let c = self.get();
            if !Self::is_space(c) && !Self::is_alphanumeric(c) {
                write!(string, "{c}").unwrap();
                self.inc();
            }
        }
        if !string.is_empty() {
            Some(Token::Punctuation(string))
        } else {
            None
        }
    }

    fn is_space(c: char) -> bool {
        matches!(c, ' ' | '\t' | '\r' | '\n')
    }

    fn is_identifier(c: char) -> bool {
        matches!(c, '_' | '$')
    }

    fn is_alphanumeric(c: char) -> bool {
        c.is_alphabetic() || c.is_ascii_digit()
    }

    fn is_string_delimiter_start(c: char) -> bool {
        matches!(c, '`' | '[' | '\'' | '"')
    }

    fn is_string_escape_for(start: char, c: char) -> bool {
        match start {
            '`' => c == '`',
            '\'' => c == '\'',
            '"' => c == '"',
            _ => false,
        }
    }

    fn is_string_delimiter_end_for(start: char, c: char) -> bool {
        match start {
            '`' => c == '`',
            '[' => c == ']',
            '\'' => c == '\'',
            '"' => c == '"',
            _ => false,
        }
    }

    fn is_escape_char(c: char) -> bool {
        c == '\\'
    }
}

// [spec:pgorm:req:sql.token.scan]
impl Iterator for Tokenizer {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(comment) = self.comment() {
            return Some(comment);
        }
        if let Some(dollar) = self.dollar_quoted() {
            return Some(dollar);
        }
        if let Some(quoted) = self.e_string() {
            return Some(quoted);
        }
        if let Some(space) = self.space() {
            return Some(space);
        }
        if let Some(unquoted) = self.unquoted() {
            return Some(unquoted);
        }
        if let Some(quoted) = self.quoted() {
            return Some(quoted);
        }
        if let Some(punctuation) = self.punctuation() {
            return Some(punctuation);
        }
        None
    }
}

impl Token {
    pub fn is_quoted(&self) -> bool {
        matches!(self, Self::Quoted(_))
    }

    pub fn is_unquoted(&self) -> bool {
        matches!(self, Self::Unquoted(_))
    }

    pub fn is_space(&self) -> bool {
        matches!(self, Self::Space(_))
    }

    pub fn is_punctuation(&self) -> bool {
        matches!(self, Self::Punctuation(_))
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Quoted(string) => string,
            Self::Unquoted(string) => string,
            Self::Space(string) => string,
            Self::Punctuation(string) => string,
        }
    }

    // [spec:pgorm:sem:sql.token.unquote] (public entry; Quoted tokens only)
    pub fn unquote(&self) -> Option<String> {
        if self.is_quoted() {
            let tokenizer = Tokenizer::new(self.as_str());
            Some(tokenizer.unquote())
        } else {
            None
        }
    }
}

// [spec:pgorm:thm:sql.token.roundtrip] (verbatim reproduction of carried text)
impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Token::Unquoted(string) => string,
                Token::Space(string) => string,
                Token::Quoted(string) => string,
                Token::Punctuation(string) => string,
            }
        )
    }
}

// [spec:pgorm:req:sql.token.scan/test]
// [spec:pgorm:req:sql.token.space+1/test]
// [spec:pgorm:req:sql.token.word/test]
// [spec:pgorm:req:sql.token.quoted+1/test]
// [spec:pgorm:sem:sql.token.unquote/test]
// [spec:pgorm:thm:sql.token.roundtrip/test]
#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_0() {
        let tokenizer = Tokenizer::new("");
        let tokens: Vec<Token> = tokenizer.iter().collect();
        assert_eq!(tokens, vec![]);
    }

    #[test]
    fn test_1() {
        let string = "SELECT * FROM `character`";
        let tokenizer = Tokenizer::new(string);
        let tokens: Vec<Token> = tokenizer.iter().collect();
        assert_eq!(
            tokens,
            vec![
                Token::Unquoted("SELECT".to_string()),
                Token::Space(" ".to_string()),
                Token::Punctuation("*".to_string()),
                Token::Space(" ".to_string()),
                Token::Unquoted("FROM".to_string()),
                Token::Space(" ".to_string()),
                Token::Quoted("`character`".to_string()),
            ]
        );
        assert_eq!(
            string,
            tokens.iter().map(|x| x.to_string()).collect::<String>()
        );
    }

    #[test]
    fn test_2() {
        let string = "SELECT * FROM `character` WHERE id = ?";
        let tokenizer = Tokenizer::new(string);
        let tokens: Vec<Token> = tokenizer.iter().collect();
        assert_eq!(
            tokens,
            vec![
                Token::Unquoted("SELECT".to_string()),
                Token::Space(" ".to_string()),
                Token::Punctuation("*".to_string()),
                Token::Space(" ".to_string()),
                Token::Unquoted("FROM".to_string()),
                Token::Space(" ".to_string()),
                Token::Quoted("`character`".to_string()),
                Token::Space(" ".to_string()),
                Token::Unquoted("WHERE".to_string()),
                Token::Space(" ".to_string()),
                Token::Unquoted("id".to_string()),
                Token::Space(" ".to_string()),
                Token::Punctuation("=".to_string()),
                Token::Space(" ".to_string()),
                Token::Punctuation("?".to_string()),
            ]
        );
        assert_eq!(
            string,
            tokens.iter().map(|x| x.to_string()).collect::<String>()
        );
    }

    #[test]
    fn test_3() {
        let string = r#"? = "?" "#;
        let tokenizer = Tokenizer::new(string);
        let tokens: Vec<Token> = tokenizer.iter().collect();
        assert_eq!(
            tokens,
            vec![
                Token::Punctuation("?".to_string()),
                Token::Space(" ".to_string()),
                Token::Punctuation("=".to_string()),
                Token::Space(" ".to_string()),
                Token::Quoted(r#""?""#.to_string()),
                Token::Space(" ".to_string()),
            ]
        );
        assert_eq!(
            string,
            tokens.iter().map(|x| x.to_string()).collect::<String>()
        );
    }

    #[test]
    fn test_4() {
        let string = r#""a\"bc""#;
        let tokenizer = Tokenizer::new(string);
        let tokens: Vec<Token> = tokenizer.iter().collect();
        assert_eq!(tokens, vec![Token::Quoted("\"a\\\"bc\"".to_string())]);
        assert_eq!(
            string,
            tokens.iter().map(|x| x.to_string()).collect::<String>()
        );
    }

    #[test]
    fn test_5() {
        let string = "abc123";
        let tokenizer = Tokenizer::new(string);
        let tokens: Vec<Token> = tokenizer.iter().collect();
        assert_eq!(tokens, vec![Token::Unquoted(string.to_string())]);
        assert_eq!(
            string,
            tokens.iter().map(|x| x.to_string()).collect::<String>()
        );
    }

    #[test]
    fn test_6() {
        let string = "2.3*4";
        let tokenizer = Tokenizer::new(string);
        let tokens: Vec<Token> = tokenizer.iter().collect();
        assert_eq!(
            tokens,
            vec![
                Token::Unquoted("2".to_string()),
                Token::Punctuation(".".to_string()),
                Token::Unquoted("3".to_string()),
                Token::Punctuation("*".to_string()),
                Token::Unquoted("4".to_string()),
            ]
        );
        assert_eq!(
            string,
            tokens.iter().map(|x| x.to_string()).collect::<String>()
        );
    }

    #[test]
    fn test_7() {
        let string = r#""a\\" B"#;
        let tokenizer = Tokenizer::new(string);
        let tokens: Vec<Token> = tokenizer.iter().collect();
        assert_eq!(
            tokens,
            vec![
                Token::Quoted("\"a\\\\\"".to_string()),
                Token::Space(" ".to_string()),
                Token::Unquoted("B".to_string()),
            ]
        );
        assert_eq!(
            string,
            tokens.iter().map(|x| x.to_string()).collect::<String>()
        );
    }

    #[test]
    fn test_8() {
        let string = r#"`a"b` "#;
        let tokenizer = Tokenizer::new(string);
        let tokens: Vec<Token> = tokenizer.iter().collect();
        assert_eq!(
            tokens,
            vec![
                Token::Quoted("`a\"b`".to_string()),
                Token::Space(" ".to_string()),
            ]
        );
        assert_eq!(
            string,
            tokens.iter().map(|x| x.to_string()).collect::<String>()
        );
    }

    #[test]
    fn test_9() {
        let string = r#"[ab] "#;
        let tokenizer = Tokenizer::new(string);
        let tokens: Vec<Token> = tokenizer.iter().collect();
        assert_eq!(
            tokens,
            vec![
                Token::Quoted("[ab]".to_string()),
                Token::Space(" ".to_string()),
            ]
        );
        assert_eq!(
            string,
            tokens.iter().map(|x| x.to_string()).collect::<String>()
        );
    }

    #[test]
    fn test_10() {
        let string = r#" 'a"b' "#;
        let tokenizer = Tokenizer::new(string);
        let tokens: Vec<Token> = tokenizer.iter().collect();
        assert_eq!(
            tokens,
            vec![
                Token::Space(" ".to_string()),
                Token::Quoted("'a\"b'".to_string()),
                Token::Space(" ".to_string()),
            ]
        );
        assert_eq!(
            string,
            tokens.iter().map(|x| x.to_string()).collect::<String>()
        );
    }

    #[test]
    fn test_11() {
        let string = r#" `a``b` "#;
        let tokenizer = Tokenizer::new(string);
        let tokens: Vec<Token> = tokenizer.iter().collect();
        assert_eq!(
            tokens,
            vec![
                Token::Space(" ".to_string()),
                Token::Quoted("`a``b`".to_string()),
                Token::Space(" ".to_string()),
            ]
        );
        assert_eq!(
            string,
            tokens.iter().map(|x| x.to_string()).collect::<String>()
        );
    }

    #[test]
    fn test_12() {
        let string = r#" 'a''b' "#;
        let tokenizer = Tokenizer::new(string);
        let tokens: Vec<Token> = tokenizer.iter().collect();
        assert_eq!(
            tokens,
            vec![
                Token::Space(" ".to_string()),
                Token::Quoted("'a''b'".to_string()),
                Token::Space(" ".to_string()),
            ]
        );
        assert_eq!(
            string,
            tokens.iter().map(|x| x.to_string()).collect::<String>()
        );
    }

    #[test]
    fn test_13() {
        let string = r#"(?)"#;
        let tokenizer = Tokenizer::new(string);
        let tokens: Vec<Token> = tokenizer.iter().collect();
        assert_eq!(
            tokens,
            vec![
                Token::Punctuation("(".to_string()),
                Token::Punctuation("?".to_string()),
                Token::Punctuation(")".to_string()),
            ]
        );
        assert_eq!(
            string,
            tokens.iter().map(|x| x.to_string()).collect::<String>()
        );
    }

    #[test]
    fn test_14() {
        let string = r#"($1 = $2)"#;
        let tokenizer = Tokenizer::new(string);
        let tokens: Vec<Token> = tokenizer.iter().collect();
        assert_eq!(
            tokens,
            vec![
                Token::Punctuation("(".to_string()),
                Token::Punctuation("$".to_string()),
                Token::Unquoted("1".to_string()),
                Token::Space(" ".to_string()),
                Token::Punctuation("=".to_string()),
                Token::Space(" ".to_string()),
                Token::Punctuation("$".to_string()),
                Token::Unquoted("2".to_string()),
                Token::Punctuation(")".to_string()),
            ]
        );
        assert_eq!(
            string,
            tokens.iter().map(|x| x.to_string()).collect::<String>()
        );
    }

    #[test]
    fn test_15() {
        let string = r#" "Hello World" "#;
        let tokenizer = Tokenizer::new(string);
        let tokens: Vec<Token> = tokenizer.iter().collect();
        assert_eq!(
            tokens,
            vec![
                Token::Space(" ".to_string()),
                Token::Quoted("\"Hello World\"".to_string()),
                Token::Space(" ".to_string()),
            ]
        );
        assert_eq!(
            string,
            tokens.iter().map(|x| x.to_string()).collect::<String>()
        );
    }

    #[test]
    fn test_16() {
        let string = "abc_$123";
        let tokenizer = Tokenizer::new(string);
        let tokens: Vec<Token> = tokenizer.iter().collect();
        assert_eq!(tokens, vec![Token::Unquoted(string.to_string())]);
        assert_eq!(
            string,
            tokens.iter().map(|x| x.to_string()).collect::<String>()
        );
    }

    #[test]
    fn test_17() {
        // `$abc$` opens a tagged dollar quote; without a closer the body runs
        // to the end of the input, verbatim.
        let string = "$abc$123";
        let tokenizer = Tokenizer::new(string);
        let tokens: Vec<Token> = tokenizer.iter().collect();
        assert_eq!(tokens, vec![Token::Quoted("$abc$123".to_string())]);
        let template = Tokenizer::new_without_dollar_quoting(string);
        assert_eq!(
            template.iter().collect::<Vec<_>>(),
            vec![
                Token::Punctuation("$".to_string()),
                Token::Unquoted("abc$123".to_string()),
            ]
        );
        assert_eq!(
            string,
            tokens.iter().map(|x| x.to_string()).collect::<String>()
        );
    }

    #[test]
    fn test_18() {
        let string = "_$abc_123$";
        let tokenizer = Tokenizer::new(string);
        let tokens: Vec<Token> = tokenizer.iter().collect();
        assert_eq!(
            tokens,
            vec![
                Token::Punctuation("_".to_string()),
                Token::Quoted("$abc_123$".to_string()),
            ]
        );
        assert_eq!(
            string,
            tokens.iter().map(|x| x.to_string()).collect::<String>()
        );
    }

    #[test]
    fn test_19() {
        let string = r#""a\"bc""#;
        let tokenizer = Tokenizer::new(string);
        assert_eq!(tokenizer.unquote(), "a\\\"bc".to_owned());
    }

    #[test]
    fn test_20() {
        let string = r#""a""bc""#;
        let tokenizer = Tokenizer::new(string);
        assert_eq!(tokenizer.unquote(), "a\"bc".to_owned());
    }

    #[test]
    fn test_21() {
        assert_eq!(
            Token::Quoted("'a\\nb'".to_owned()).unquote().unwrap(),
            "a\\nb".to_owned()
        );
    }

    #[test]
    fn test_22() {
        let string = r#" "Hello\nWorld" "#;
        let tokenizer = Tokenizer::new(string);
        let tokens: Vec<Token> = tokenizer.iter().collect();
        assert_eq!(
            tokens,
            vec![
                Token::Space(" ".to_string()),
                Token::Quoted("\"Hello\\nWorld\"".to_string()),
                Token::Space(" ".to_string()),
            ]
        );
        assert_eq!(
            string,
            tokens.iter().map(|x| x.to_string()).collect::<String>()
        );
    }
}

#[cfg(test)]
mod context_tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn tokens(input: &str) -> Vec<Token> {
        Tokenizer::new(input).iter().collect()
    }

    fn roundtrip(input: &str) {
        let joined: String = tokens(input).iter().map(Token::as_str).collect();
        assert_eq!(joined, input);
    }

    // [spec:pgorm:req:sql.token.space+1/test]    comments lex as one space
    // token, so nothing inside them reaches any other token form
    #[test]
    fn comments_lex_as_space() {
        roundtrip("SELECT 1 -- $1 trailing\nFROM t");
        roundtrip("SELECT 1 /* $1 /* nested $2 */ still */ FROM t");
        let toks = tokens("a /* $1 */ b");
        assert!(matches!(&toks[2], Token::Space(s) if s == "/* $1 */"));
    }

    // [spec:pgorm:req:sql.token.quoted+1/test]    a dollar-quoted body is one
    // quoted token — tagged, unclosed and placeholder-adjacent forms included
    // — while `$1` stays punctuation
    #[test]
    fn dollar_quotes_lex_as_quoted() {
        roundtrip("SELECT $$ $1 $$ WHERE $1 IS NOT NULL");
        roundtrip("SELECT $tag$ body $$ inner $tag$ AND $2");
        let toks = tokens("$$ $1 $$ $1");
        assert!(matches!(&toks[0], Token::Quoted(s) if s == "$$ $1 $$"));
        assert!(matches!(&toks[2], Token::Punctuation(p) if p == "$"));
        let unclosed = tokens("$$ runs to the end");
        assert_eq!(unclosed.len(), 1);
    }

    // [spec:pgorm:req:sql.token.quoted+1/test]    an escape string honours the
    // backslash, so an escaped quote does not end the body
    #[test]
    fn escape_strings_lex_as_quoted() {
        roundtrip(r"SELECT E'a\'b $1' AND $1");
        let toks = tokens(r"E'a\'b $1' x");
        assert!(matches!(&toks[0], Token::Quoted(s) if s == r"E'a\'b $1'"));
    }
}
