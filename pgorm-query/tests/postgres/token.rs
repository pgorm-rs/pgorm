use super::*;
use crate::oracle::assert_eq;

// [spec:pgorm:def:sql.token/test]    input is held as a `Vec<char>` plus a cursor
#[test]
fn tokenizer_holds_input_chars_and_cursor() {
    let tokenizer = Tokenizer::new("a b");
    assert_eq!(tokenizer.chars, vec!['a', ' ', 'b']);
    assert_eq!(tokenizer.p, 0);

    let mut tokenizer = Tokenizer::new("a b");
    assert_eq!(tokenizer.next(), Some(Token::Unquoted("a".to_owned())));
    assert_eq!(tokenizer.p, 1);
    assert_eq!(tokenizer.next(), Some(Token::Space(" ".to_owned())));
    assert_eq!(tokenizer.next(), Some(Token::Unquoted("b".to_owned())));
    assert_eq!(tokenizer.next(), None);
}

// [spec:pgorm:def:sql.token/test]    the four classes, each carrying its exact source text
#[test]
fn token_classes_carry_their_source_text_verbatim() {
    let input = r#"SELECT 'a' , "b""#;
    let tokens: Vec<Token> = Tokenizer::new(input).iter().collect();

    assert_eq!(
        tokens,
        vec![
            Token::Unquoted("SELECT".to_owned()),
            Token::Space(" ".to_owned()),
            Token::Quoted("'a'".to_owned()),
            Token::Space(" ".to_owned()),
            Token::Punctuation(",".to_owned()),
            Token::Space(" ".to_owned()),
            Token::Quoted(r#""b""#.to_owned()),
        ]
    );

    // `as_str` and `Display` both hand back the carried text unmodified: quoted
    // tokens keep their delimiters, spaces keep their exact run.
    let strs: Vec<&str> = tokens.iter().map(|t| t.as_str()).collect();
    assert_eq!(strs, vec!["SELECT", " ", "'a'", " ", ",", " ", r#""b""#]);
    for token in &tokens {
        assert_eq!(token.as_str(), token.to_string());
    }
    assert_eq!(strs.concat(), input);

    assert!(tokens[0].is_unquoted());
    assert!(tokens[1].is_space());
    assert!(tokens[2].is_quoted());
    assert!(tokens[4].is_punctuation());
}

// [spec:pgorm:sem:sql.token.limits+1/test]    a line comment is one space
// token, ending at the newline it includes
#[test]
fn line_comments_lex_as_one_space_token() {
    let tokens: Vec<Token> = Tokenizer::new("1 -- two").iter().collect();

    assert_eq!(
        tokens,
        vec![
            Token::Unquoted("1".to_owned()),
            Token::Space(" ".to_owned()),
            Token::Space("-- two".to_owned()),
        ]
    );
}

// [spec:pgorm:sem:sql.token.limits+1/test]    a block comment is one space
// token, and a quote character inside a comment stays inside it
#[test]
fn block_comments_lex_as_one_space_token() {
    let tokens: Vec<Token> = Tokenizer::new("/* a */").iter().collect();
    assert_eq!(tokens, vec![Token::Space("/* a */".to_owned())]);

    let tokens: Vec<Token> = Tokenizer::new("-- it's fine\nSELECT").iter().collect();
    assert_eq!(
        tokens,
        vec![
            Token::Space("-- it's fine\n".to_owned()),
            Token::Unquoted("SELECT".to_owned()),
        ]
    );
}

// [spec:pgorm:sem:sql.token.limits+1/test]    a dollar-quoted body — bare or
// tagged — is one quoted token
#[test]
fn dollar_quoting_lexes_as_one_quoted_token() {
    let tokens: Vec<Token> = Tokenizer::new("$$body$$").iter().collect();
    assert_eq!(tokens, vec![Token::Quoted("$$body$$".to_owned())]);

    let tokens: Vec<Token> = Tokenizer::new("$tag$body$tag$").iter().collect();
    assert_eq!(tokens, vec![Token::Quoted("$tag$body$tag$".to_owned())]);
}

// [spec:pgorm:sem:sql.token.limits+1/test]    an E-string is one quoted token,
// its backslash escapes honoured
#[test]
fn e_strings_lex_as_one_quoted_token() {
    let tokens: Vec<Token> = Tokenizer::new(r#"E'a\nb'"#).iter().collect();
    assert_eq!(tokens, vec![Token::Quoted(r#"E'a\nb'"#.to_owned())]);
}

// [spec:pgorm:sem:sql.token.limits+1/test]    the delimiter set is wider than PostgreSQL's
#[test]
fn backtick_and_bracket_strings_are_quoted_tokens_too() {
    let tokens: Vec<Token> = Tokenizer::new("`a` [b] \"c\" 'd'").iter().collect();
    assert_eq!(
        tokens
            .iter()
            .filter(|t| t.is_quoted())
            .map(|t| t.as_str())
            .collect::<Vec<_>>(),
        vec!["`a`", "[b]", "\"c\"", "'d'"]
    );
}
