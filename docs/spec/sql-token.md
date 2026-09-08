# SQL tokenizer (pgorm-query)

This spec covers `pgorm-query/src/token.rs`: a small, lossless SQL tokenizer
used by the rendering layer (`inject_parameters` and
`SimpleExpr::CustomWithExpr` in `sql.render`) to locate placeholder markers
without being fooled by quoted text. It is not a SQL parser; it has no notion
of keywords, statements, or expressions.

## Artifact and token classes

> [spec:pgorm:def:sql.token]
> The tokenizer is `Tokenizer` in `pgorm-query/src/token.rs`: it holds the
> input as a `Vec<char>` plus a cursor and implements `Iterator<Item = Token>`.
> `Token` has exactly four classes, each carrying the exact source text it
> consumed: `Space` (whitespace run), `Unquoted` (word), `Quoted` (delimited
> string including its delimiters), and `Punctuation` (a single symbol
> character). `Token::as_str` and the `Display` impl both return the carried
> text unmodified.

## Scanning

> [spec:pgorm:req:sql.token.scan]
> At each position the tokenizer MUST try token classes in this fixed order:
> space, unquoted, quoted, punctuation; the first class whose start condition
> matches consumes the token. `Space` and `Unquoted` use maximal munch.
> `Punctuation` always consumes exactly one character — any character that is
> neither whitespace nor alphanumeric (including `_` and `$` when they cannot
> start a word, and any quote-delimiter character not consumed by the quoted
> rule). Every input character therefore belongs to exactly one token, and
> iteration yields `None` exactly when the input is exhausted.

> [spec:pgorm:req:sql.token.space+1]
> A `Space` token is a maximal run of the whitespace characters space
> (`' '`), tab (`\t`), carriage return (`\r`), and newline (`\n`) — or one
> whole comment, `--` through the newline it ends at or a nested `/* */`
> block, which separates tokens exactly as whitespace does
> (`sql.token.limits`). Consecutive whitespace characters of different kinds
> are grouped into a single token; a comment is its own token. No other
> characters (including other Unicode whitespace) count as whitespace.

> [spec:pgorm:req:sql.token.word]
> An `Unquoted` token MUST start with an alphanumeric character — Unicode
> alphabetic per `char::is_alphabetic`, or an ASCII digit — and continues
> through subsequent characters that are alphanumeric or one of `_` and `$`.
> `_` and `$` MUST NOT start a word (a leading `_` or `$` becomes a
> `Punctuation` token instead): `abc_$123` is one word, while `$abc$123`
> tokenizes as punctuation `$` followed by word `abc$123`. Numbers are not
> special: `2.3` tokenizes as word `2`, punctuation `.`, word `3` — which is
> exactly what makes `$1` scan as punctuation `$` + word `1` for placeholder
> detection.

> [spec:pgorm:req:sql.token.quoted+1]
> A `Quoted` token MUST begin with one of the delimiter characters `` ` ``,
> `[`, `'`, `"` — or spell one of the two PostgreSQL forms of
> `sql.token.limits`: a dollar-quoted body `$tag$…$tag$`, or an escape
> string `E'…'` — and otherwise runs under this state machine: the closing character is
> the paired delimiter (`[` closes with `]`; the others close with
> themselves). A backslash escapes the immediately following character
> (`escape` toggles, so `\\` does not escape the character after it). On
> reaching an unescaped closing delimiter, the tokenizer peeks one character:
> if it equals the doubling continuation character for the start delimiter
> (`` ` `` → `` ` ``, `'` → `'`, `"` → `"`; bracket strings have none), both
> characters are consumed and scanning continues inside the token — so
> `'a''b'` and `"a""b"` are each a single token. The token text includes the
> delimiters and all interior characters verbatim. An unterminated quote
> consumes to end of input without error.

## Unquoting

> [spec:pgorm:sem:sql.token.unquote]
> `Token::unquote` returns `Some(text)` only for `Quoted` tokens (else
> `None`). It re-runs the quote state machine over the token text, dropping
> the outer delimiters and collapsing each doubled delimiter to a single
> occurrence (`"a""bc"` → `a"bc`), while leaving backslash escape sequences
> intact rather than decoding them (`"a\"bc"` → `a\"bc`, `'a\nb'` → `a\nb`).

## Properties and limitations

> [spec:pgorm:thm:sql.token.roundtrip]
> Tokenization is lossless: for any input string, concatenating the `Display`
> output of the token stream in order reproduces the input exactly. This holds
> because every character is consumed into exactly one token and every token
> stores its source text verbatim; the module's unit tests assert it for each
> case.

> [spec:pgorm:sem:sql.token.limits+1]
> The tokenizer understands PostgreSQL's non-plain lexical regions: `--`
> line comments (through the newline they end at) and nested `/* */` block
> comments each lex as ONE `Space` token — a comment separates tokens
> exactly as whitespace does — while `$$…$$` / `$tag$…$tag$` dollar-quoted
> bodies and `E'…'` escape strings (backslash escapes honoured) each lex as
> ONE `Quoted` token. A `$` followed by a digit is a placeholder spelling,
> never a dollar-quote tag, and an unclosed dollar body runs to the end of
> the input verbatim. `Tokenizer::new_without_dollar_quoting` disables only
> the dollar-quote form, for input whose `$` spellings carry their own
> grammar — the placeholder template's `$$` escape
> (`sql.render.custom-expr`). Remaining limitations: the accepted delimiter
> set is wider than PostgreSQL's — backtick and square-bracket strings
> (MySQL / SQL Server identifier syntax) are treated as quoted tokens too —
> and there is no `U&'…'` awareness. Consumers relying on the tokenizer to
> skip quoted regions (see `sql.render.inject`) inherit exactly this
> contract.
