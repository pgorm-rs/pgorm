//! Tokenizer for processing SQL.

use std::fmt::Write;
use std::iter::Iterator;

// [spec:pgorm:def:sql.token+1]
// [spec:pgorm:sem:sql.token.limits+3]
#[derive(Debug, Default)]
pub struct Tokenizer {
    dollar_quotes: bool,
    chars: Vec<char>,
    p: usize,
}

/// A quoted form as one scan read it: the text it consumed, verbatim, and
/// its body — what lies between the delimiters, with each doubled delimiter
/// collapsed to one and every backslash sequence left as written. Lexing
/// keeps the text and [`Token::unquote`] keeps the body, so the two share a
/// single reading of where the form ends.
#[derive(Debug, Default)]
struct Delimited {
    text: String,
    body: String,
}

// [spec:pgorm:def:sql.token+1] (the four token classes)
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
        self.p >= self.chars.len()
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
    // [spec:pgorm:req:sql.token.quoted+3]
    fn dollar_quoted(&mut self) -> Option<Token> {
        self.dollar_form().map(|form| Token::Quoted(form.text))
    }

    /// The `$tag$ … $tag$` form at the cursor. The tag is counted in
    /// characters, not bytes: a tag may be any alphabetic character, and
    /// stepping the cursor by the opener's byte length would skip body
    /// characters after a multi-byte one.
    // [spec:pgorm:req:sql.token.quoted+3]
    fn dollar_form(&mut self) -> Option<Delimited> {
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
        let opener: Vec<char> = self.chars[self.p..=self.p + tag_len + 1].to_vec();
        let mut form = Delimited::default();
        form.text.extend(&opener);
        self.p += opener.len();
        while !self.end() {
            let closes = opener
                .iter()
                .enumerate()
                .all(|(ahead, &c)| self.peek(ahead) == Some(c));
            if closes {
                form.text.extend(&opener);
                self.p += opener.len();
                break;
            }
            form.text.push(self.get());
            form.body.push(self.get());
            self.inc();
        }
        Some(form)
    }

    /// An escape string, `E'…'`, lexed whole as one [`Token::Quoted`]. It is
    /// tried ahead of words, which would otherwise take the `E` and leave a
    /// plain literal behind it.
    // [spec:pgorm:req:sql.token.quoted+3]
    fn e_string(&mut self) -> Option<Token> {
        if !self.at_escape_string() {
            return None;
        }
        self.delimited().map(|form| Token::Quoted(form.text))
    }

    fn at_escape_string(&self) -> bool {
        matches!(self.peek(0), Some('E' | 'e')) && self.peek(1) == Some('\'')
    }

    /// A `'…'` literal, a `"…"` delimited identifier, or an `E'…'` escape
    /// string at the cursor.
    ///
    /// Only the escape string gives a backslash meaning: inside it `\`
    /// escapes the next character, the closing quote included. In the other
    /// two forms a backslash is an ordinary character — PostgreSQL's
    /// standard-conforming reading, the server default since 9.1 — so
    /// `'C:\'` is complete, and reading its backslash as an escape would
    /// carry the literal on over whatever follows it. Every form continues
    /// past a doubled delimiter.
    ///
    /// An escape string also runs on through each continuation segment: a
    /// `'` reached across whitespace that holds a newline. PostgreSQL scans a
    /// continuation in the state of the string it continues, so a backslash
    /// there is still an escape, and lexing the segment as a plain literal
    /// would end it at a `\'` the server reads as part of the body.
    // [spec:pgorm:req:sql.token.quoted+3]
    fn delimited(&mut self) -> Option<Delimited> {
        let escapes = self.at_escape_string();
        let mut form = Delimited::default();
        if escapes {
            form.text.push(self.get());
            self.inc();
        }
        let delimiter = self
            .peek(0)
            .filter(|&c| Self::is_string_delimiter_start(c))?;
        loop {
            form.text.push(delimiter);
            self.inc();
            let closed = self.delimited_segment(delimiter, escapes, &mut form);
            if !(closed && escapes) {
                break;
            }
            let Some(gap) = self.continuation() else {
                break;
            };
            form.text.extend(&self.chars[self.p..self.p + gap]);
            self.p += gap;
        }
        Some(form)
    }

    /// One segment of a delimited form, from just past its opening delimiter
    /// through its closing one. `false` when the input ends first.
    fn delimited_segment(&mut self, delimiter: char, escapes: bool, form: &mut Delimited) -> bool {
        while !self.end() {
            let c = self.get();
            self.inc();
            form.text.push(c);
            if escapes && c == '\\' {
                form.body.push(c);
                if !self.end() {
                    let escaped = self.get();
                    self.inc();
                    form.text.push(escaped);
                    form.body.push(escaped);
                }
            } else if c != delimiter {
                form.body.push(c);
            } else if self.peek(0) == Some(delimiter) {
                self.inc();
                form.text.push(delimiter);
                form.body.push(delimiter);
            } else {
                return true;
            }
        }
        false
    }

    /// The length of a string continuation at the cursor: PostgreSQL's
    /// whitespace — `--` comments included, block comments not — holding at
    /// least one newline, up to the `'` that continues the string before it.
    /// `None` when the cursor is at anything else.
    fn continuation(&self) -> Option<usize> {
        let mut ahead = 0usize;
        let mut newline = false;
        loop {
            match self.peek(ahead)? {
                '\n' | '\r' => newline = true,
                ' ' | '\t' | '\x0b' | '\x0c' => {}
                '-' if self.peek(ahead + 1) == Some('-') => {
                    while !matches!(self.peek(ahead + 1), None | Some('\n' | '\r')) {
                        ahead += 1;
                    }
                }
                '\'' if newline => return Some(ahead),
                _ => return None,
            }
            ahead += 1;
        }
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

    // [spec:pgorm:req:sql.token.quoted+3]
    fn quoted(&mut self) -> Option<Token> {
        self.delimited().map(|form| Token::Quoted(form.text))
    }

    /// The body of the quoted form at the start of the input. It is read by
    /// the very scan that lexed the form, so where a token ends and what its
    /// body holds cannot disagree.
    // [spec:pgorm:sem:sql.token.unquote+1]
    fn unquote(mut self) -> String {
        self.dollar_form()
            .or_else(|| self.delimited())
            .map(|form| form.body)
            .unwrap_or_default()
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

    /// PostgreSQL's two string delimiters: `'` for a literal, `"` for a
    /// delimited identifier. Each closes with itself and doubles itself, so
    /// the closing and doubling tests both compare against the opening
    /// delimiter and need no table. The backtick and `[bracket]` forms the
    /// delimiter set used to carry are MySQL and SQL Server syntax:
    /// PostgreSQL never opens a string with either, so treating them as
    /// quoted made the tokenizer read plain SQL as an opaque body and skip
    /// the placeholders inside it.
    // [spec:pgorm:req:sql.token.quoted+3]
    fn is_string_delimiter_start(c: char) -> bool {
        matches!(c, '\'' | '"')
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

    /// The body of a [`Token::Quoted`] — its delimiters dropped, each doubled
    /// delimiter collapsed to one, backslash sequences left as written — or
    /// `None` for any other class.
    // [spec:pgorm:sem:sql.token.unquote+1] (public entry; Quoted tokens only)
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
// [spec:pgorm:req:sql.token.quoted+3/test]
// [spec:pgorm:sem:sql.token.unquote+1/test]
// [spec:pgorm:thm:sql.token.roundtrip/test]
#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// Assert the token stream `input` lexes to, and that the stream puts the
    /// input back together — the losslessness property
    /// (`[spec:pgorm:thm:sql.token.roundtrip]`) holds per case, so every case
    /// checks it.
    fn lexes(input: &str, expected: Vec<Token>) {
        let tokens: Vec<Token> = Tokenizer::new(input).iter().collect();
        assert_eq!(tokens, expected);
        assert_eq!(
            input,
            tokens.iter().map(|t| t.to_string()).collect::<String>()
        );
    }

    fn quoted(text: &str) -> Token {
        Token::Quoted(text.to_owned())
    }

    fn word(text: &str) -> Token {
        Token::Unquoted(text.to_owned())
    }

    fn space(text: &str) -> Token {
        Token::Space(text.to_owned())
    }

    fn mark(text: &str) -> Token {
        Token::Punctuation(text.to_owned())
    }

    #[test]
    fn test_0() {
        lexes("", vec![]);
    }

    #[test]
    fn test_1() {
        lexes(
            r#"SELECT * FROM "character""#,
            vec![
                word("SELECT"),
                space(" "),
                mark("*"),
                space(" "),
                word("FROM"),
                space(" "),
                quoted(r#""character""#),
            ],
        );
    }

    #[test]
    fn test_2() {
        lexes(
            r#"SELECT * FROM "character" WHERE id = ?"#,
            vec![
                word("SELECT"),
                space(" "),
                mark("*"),
                space(" "),
                word("FROM"),
                space(" "),
                quoted(r#""character""#),
                space(" "),
                word("WHERE"),
                space(" "),
                word("id"),
                space(" "),
                mark("="),
                space(" "),
                mark("?"),
            ],
        );
    }

    #[test]
    fn test_3() {
        lexes(
            r#"? = "?" "#,
            vec![
                mark("?"),
                space(" "),
                mark("="),
                space(" "),
                quoted(r#""?""#),
                space(" "),
            ],
        );
    }

    #[test]
    fn test_4() {
        // A backslash is an ordinary character in a delimited identifier, so
        // `\"` is a backslash and then the closing quote.
        lexes(
            r#""a\"bc""#,
            vec![quoted(r#""a\""#), word("bc"), quoted(r#"""#)],
        );
    }

    #[test]
    fn test_5() {
        lexes("abc123", vec![word("abc123")]);
    }

    #[test]
    fn test_6() {
        lexes(
            "2.3*4",
            vec![word("2"), mark("."), word("3"), mark("*"), word("4")],
        );
    }

    #[test]
    fn test_7() {
        lexes(
            r#""a\\" B"#,
            vec![quoted(r#""a\\""#), space(" "), word("B")],
        );
    }

    #[test]
    fn test_8() {
        // A backtick is MySQL's quote, not PostgreSQL's: it is punctuation,
        // and the `"` inside opens the only string here.
        lexes(r#"`a"b` "#, vec![mark("`"), word("a"), quoted(r#""b` "#)]);
    }

    #[test]
    fn test_9() {
        // `[ab]` is SQL Server's quote; to PostgreSQL it is subscript
        // punctuation around a word, and it lexes that way.
        lexes("[ab] ", vec![mark("["), word("ab"), mark("]"), space(" ")]);
    }

    #[test]
    fn test_10() {
        lexes(
            r#" 'a"b' "#,
            vec![space(" "), quoted(r#"'a"b'"#), space(" ")],
        );
    }

    #[test]
    fn test_11() {
        lexes(
            " `a``b` ",
            vec![
                space(" "),
                mark("`"),
                word("a"),
                mark("`"),
                mark("`"),
                word("b"),
                mark("`"),
                space(" "),
            ],
        );
    }

    #[test]
    fn test_12() {
        lexes(" 'a''b' ", vec![space(" "), quoted("'a''b'"), space(" ")]);
    }

    #[test]
    fn test_13() {
        lexes("(?)", vec![mark("("), mark("?"), mark(")")]);
    }

    #[test]
    fn test_14() {
        lexes(
            "($1 = $2)",
            vec![
                mark("("),
                mark("$"),
                word("1"),
                space(" "),
                mark("="),
                space(" "),
                mark("$"),
                word("2"),
                mark(")"),
            ],
        );
    }

    #[test]
    fn test_15() {
        lexes(
            r#" "Hello World" "#,
            vec![space(" "), quoted(r#""Hello World""#), space(" ")],
        );
    }

    #[test]
    fn test_16() {
        lexes("abc_$123", vec![word("abc_$123")]);
    }

    #[test]
    fn test_17() {
        // `$abc$` opens a tagged dollar quote; without a closer the body runs
        // to the end of the input, verbatim.
        lexes("$abc$123", vec![quoted("$abc$123")]);

        let template = Tokenizer::new_without_dollar_quoting("$abc$123");
        assert_eq!(
            template.iter().collect::<Vec<_>>(),
            vec![mark("$"), word("abc$123")]
        );
    }

    #[test]
    fn test_18() {
        lexes("_$abc_123$", vec![mark("_"), quoted("$abc_123$")]);
    }

    #[test]
    fn test_19() {
        assert_eq!(Tokenizer::new(r#""a\"bc""#).unquote(), r"a\");
        assert_eq!(Tokenizer::new(r"E'a\'bc'").unquote(), r"a\'bc");
    }

    #[test]
    fn test_20() {
        assert_eq!(Tokenizer::new(r#""a""bc""#).unquote(), r#"a"bc"#);
    }

    #[test]
    fn test_21() {
        assert_eq!(
            Token::Quoted(r"'a\nb'".to_owned()).unquote().unwrap(),
            r"a\nb"
        );
    }

    #[test]
    fn test_22() {
        lexes(
            r#" "Hello\nWorld" "#,
            vec![space(" "), quoted(r#""Hello\nWorld""#), space(" ")],
        );
    }

    #[test]
    fn a_plain_literal_ends_at_its_backslash_quote() {
        // Under standard-conforming strings `\'` is a backslash and then the
        // closing quote: `'C:\'` is complete.
        lexes(r"'C:\' x", vec![quoted(r"'C:\'"), space(" "), word("x")]);
        lexes(r"'a\'b'", vec![quoted(r"'a\'"), word("b"), quoted("'")]);
        lexes(r"'\\' x", vec![quoted(r"'\\'"), space(" "), word("x")]);
    }

    #[test]
    fn a_backslash_escapes_inside_an_escape_string() {
        lexes(
            r"E'C:\\' x",
            vec![quoted(r"E'C:\\'"), space(" "), word("x")],
        );
        lexes(r"e'\'' x", vec![quoted(r"e'\''"), space(" "), word("x")]);
        lexes(
            r"E'a''\'b' x",
            vec![quoted(r"E'a''\'b'"), space(" "), word("x")],
        );
        // `E` inside a word is not a prefix: the literal after it is plain.
        lexes(
            r"abcE'\' x",
            vec![word("abcE"), quoted(r"'\'"), space(" "), word("x")],
        );
    }

    #[test]
    fn an_escape_string_runs_through_its_continuations() {
        // PostgreSQL scans a continuation in the escape string's own state,
        // so the backslash there still escapes the quote after it.
        lexes(
            "E'a'\n'b\\' $1 ' x",
            vec![quoted("E'a'\n'b\\' $1 '"), space(" "), word("x")],
        );
        lexes(
            "E'a' -- note\n\n  -- more\n '\\'' x",
            vec![
                quoted("E'a' -- note\n\n  -- more\n '\\''"),
                space(" "),
                word("x"),
            ],
        );
        // No newline, or a block comment in the gap: no continuation.
        lexes(
            "E'a' '\\'",
            vec![quoted("E'a'"), space(" "), quoted("'\\'")],
        );
        lexes(
            "E'a' /* c */\n'\\'",
            vec![
                quoted("E'a'"),
                space(" "),
                space("/* c */"),
                space("\n"),
                quoted("'\\'"),
            ],
        );
        // A plain literal's continuation is plain too, so it lexes apart.
        lexes(
            "'a'\n'b\\'",
            vec![quoted("'a'"), space("\n"), quoted("'b\\'")],
        );
    }

    #[test]
    fn dollar_bodies_keep_their_backslashes_verbatim() {
        lexes(
            r"$$C:\$$ x",
            vec![quoted(r"$$C:\$$"), space(" "), word("x")],
        );
        lexes(
            r"$t$ \' $1 $t$ x",
            vec![quoted(r"$t$ \' $1 $t$"), space(" "), word("x")],
        );
    }

    #[test]
    fn a_multibyte_dollar_tag_keeps_its_body() {
        lexes(
            "$é$x$é$ $1",
            vec![quoted("$é$x$é$"), space(" "), mark("$"), word("1")],
        );
    }

    #[test]
    fn every_quoted_form_unquotes_to_its_body() {
        let cases = [
            (r"'C:\'", r"C:\"),
            (r#""a\""#, r"a\"),
            (r"'a''b'", "a'b"),
            (r"E'C:\\'", r"C:\\"),
            (r"e'a''\'b'", r"a'\'b"),
            ("E'x'\n'y\\'z'", r"xy\'z"),
            ("$$a $1$$", "a $1"),
            ("$é$x$é$", "x"),
            ("'unclosed", "unclosed"),
        ];
        for (text, body) in cases {
            assert_eq!(
                Token::Quoted(text.to_owned()).unquote().as_deref(),
                Some(body),
                "unquoting {text}"
            );
        }
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

    // [spec:pgorm:req:sql.token.quoted+3/test]    a dollar-quoted body is one
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

    // [spec:pgorm:req:sql.token.quoted+3/test]    an escape string honours the
    // backslash, so an escaped quote does not end the body
    #[test]
    fn escape_strings_lex_as_quoted() {
        roundtrip(r"SELECT E'a\'b $1' AND $1");
        let toks = tokens(r"E'a\'b $1' x");
        assert!(matches!(&toks[0], Token::Quoted(s) if s == r"E'a\'b $1'"));
    }
}
