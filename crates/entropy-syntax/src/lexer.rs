use unicode_normalization::UnicodeNormalization;

use crate::diagnostic::{Diagnostic, Span, code};

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keyword {
    Program,
    Declare,
    Let,
    Print,
    If,
    While,
    Real,
    Char,
    String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    Keyword(Keyword),
    /// Normalised name (NFC, lowercase). The text as written is the token's span.
    Ident(String),
    Number(f32),
    Str(String),
    Char(char),
    LBracket,
    RBracket,
    LParen,
    RParen,
    Semi,
    Comma,
    Assign,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    PercentAssign,
    /// `&= |= ^= <<= >>=`: parsed so the parser can reject them clearly.
    BitwiseAssign,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Lt,
    Gt,
    Le,
    Ge,
    Eof,
}

/// Identifiers compare after NFC normalisation and Unicode lowercasing.
pub fn normalize_ident(text: &str) -> String {
    text.nfc().collect::<String>().to_lowercase()
}

fn keyword(name: &str) -> Option<Keyword> {
    Some(match name {
        "program" => Keyword::Program,
        "declare" => Keyword::Declare,
        "let" => Keyword::Let,
        "print" => Keyword::Print,
        "if" => Keyword::If,
        "while" => Keyword::While,
        "real" => Keyword::Real,
        "char" => Keyword::Char,
        "string" => Keyword::String,
        _ => return None,
    })
}

/// Lexes the whole source. Errors are reported and skipped; the token list always ends in `Eof`.
pub fn lex(src: &str) -> (Vec<Token>, Vec<Diagnostic>) {
    let mut lexer = Lexer {
        src,
        pos: if src.starts_with('\u{feff}') { 3 } else { 0 },
        tokens: Vec::new(),
        diagnostics: Vec::new(),
    };
    lexer.run();
    (lexer.tokens, lexer.diagnostics)
}

struct Lexer<'a> {
    src: &'a str,
    pos: usize,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
}

impl Lexer<'_> {
    fn rest(&self) -> &str {
        self.src.get(self.pos..).unwrap_or("")
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn peek2(&self) -> Option<char> {
        self.rest().chars().nth(1)
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn eat(&mut self, c: char) -> bool {
        let found = self.peek() == Some(c);
        if found {
            self.pos += c.len_utf8();
        }
        found
    }

    fn text(&self, start: usize) -> &str {
        self.src.get(start..self.pos).unwrap_or("")
    }

    fn push(&mut self, kind: TokenKind, start: usize) {
        self.tokens.push(Token {
            kind,
            span: Span::new(start, self.pos),
        });
    }

    fn error(&mut self, code: crate::diagnostic::Code, span: Span, message: impl Into<String>) {
        self.diagnostics
            .push(Diagnostic::error(code, span, message));
    }

    fn run(&mut self) {
        while let Some(c) = self.peek() {
            let start = self.pos;
            if c.is_whitespace() {
                self.bump();
            } else if c == '#' || self.rest().starts_with("//") {
                self.line_comment();
            } else if self.rest().starts_with("/*") {
                self.block_comment(start);
            } else if c.is_ascii_digit()
                || (c == '.' && self.peek2().is_some_and(|d| d.is_ascii_digit()))
            {
                self.number(start);
            } else if c == '_' || unicode_ident::is_xid_start(c) {
                self.ident(start);
            } else if c == '"' {
                self.string(start);
            } else if c == '\'' {
                self.char_literal(start);
            } else {
                self.punct(c, start);
            }
        }
        let end = self.src.len();
        self.tokens.push(Token {
            kind: TokenKind::Eof,
            span: Span::new(end, end),
        });
    }

    fn line_comment(&mut self) {
        while let Some(c) = self.bump() {
            if c == '\n' {
                break;
            }
        }
    }

    fn block_comment(&mut self, start: usize) {
        self.pos += 2;
        match self.rest().find("*/") {
            Some(i) => self.pos += i + 2,
            None => {
                self.pos = self.src.len();
                self.error(
                    code::UNTERMINATED_COMMENT,
                    Span::new(start, start + 2),
                    "unterminated block comment",
                );
            }
        }
    }

    fn digits(&mut self) {
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
        }
    }

    fn number(&mut self, start: usize) {
        let leading_dot = self.peek() == Some('.');
        self.digits();
        let mut trailing_dot = false;
        if self.peek() == Some('.') {
            self.bump();
            trailing_dot = !self.peek().is_some_and(|c| c.is_ascii_digit());
            self.digits();
        }
        // An `e` only starts an exponent when digits follow; otherwise it's the next token.
        let sign = matches!(self.peek2(), Some('+' | '-'));
        let after = self.rest().chars().nth(if sign { 2 } else { 1 });
        if matches!(self.peek(), Some('e' | 'E')) && after.is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
            if sign {
                self.bump();
            }
            self.digits();
        }

        let span = Span::new(start, self.pos);
        let text = self.text(start).to_string();
        if leading_dot {
            self.error(
                code::MALFORMED_NUMBER,
                span,
                format!("a number must start with a digit; write `0{text}`"),
            );
        } else if trailing_dot {
            self.error(
                code::MALFORMED_NUMBER,
                span,
                format!("expected a digit after `.`; write `{text}0`"),
            );
        }
        let mut value = if leading_dot || trailing_dot {
            0.0
        } else {
            text.parse::<f32>().unwrap_or(0.0)
        };
        if value.is_infinite() {
            self.error(
                code::NUMBER_TOO_LARGE,
                span,
                "number is too large for a real",
            );
            value = 0.0;
        }
        self.push(TokenKind::Number(value), start);
    }

    fn ident(&mut self, start: usize) {
        self.bump();
        while self.peek().is_some_and(unicode_ident::is_xid_continue) {
            self.bump();
        }
        let name = normalize_ident(self.text(start));
        let kind = match keyword(&name) {
            Some(k) => TokenKind::Keyword(k),
            None => TokenKind::Ident(name),
        };
        self.push(kind, start);
    }

    /// Reads one character of a string or char literal, handling escapes.
    /// Returns `None` at end of input.
    fn literal_char(&mut self) -> Option<char> {
        let start = self.pos;
        let c = self.bump()?;
        if c != '\\' {
            return Some(c);
        }
        let escaped = match self.bump() {
            Some('n') => '\n',
            Some('t') => '\t',
            Some('r') => '\r',
            Some('\\') => '\\',
            Some('"') => '"',
            Some('\'') => '\'',
            Some('0') => '\0',
            Some('u') => return Some(self.unicode_escape(start)),
            Some(other) => {
                self.error(
                    code::INVALID_ESCAPE,
                    Span::new(start, self.pos),
                    format!("unknown escape `\\{other}`"),
                );
                '\u{fffd}'
            }
            None => return None,
        };
        Some(escaped)
    }

    fn unicode_escape(&mut self, start: usize) -> char {
        let hex: String = self.rest().chars().take(4).collect();
        let valid = hex.len() == 4 && hex.chars().all(|c| c.is_ascii_hexdigit());
        if valid {
            self.pos += 4;
        }
        let c = valid
            .then(|| u32::from_str_radix(&hex, 16).ok())
            .flatten()
            .and_then(char::from_u32);
        c.unwrap_or_else(|| {
            self.error(
                code::INVALID_ESCAPE,
                Span::new(start, self.pos),
                "`\\u` needs 4 hex digits naming a Unicode scalar value (not a surrogate)",
            );
            '\u{fffd}'
        })
    }

    fn string(&mut self, start: usize) {
        self.bump();
        let mut value = String::new();
        loop {
            if self.eat('"') {
                self.push(TokenKind::Str(value), start);
                return;
            }
            match self.literal_char() {
                Some(c) => value.push(c),
                None => {
                    self.error(
                        code::UNTERMINATED_STRING,
                        Span::new(start, start + 1),
                        "unterminated string literal",
                    );
                    self.push(TokenKind::Str(value), start);
                    return;
                }
            }
        }
    }

    fn char_literal(&mut self, start: usize) {
        self.bump();
        if self.eat('\'') {
            self.error(
                code::CHAR_LITERAL_LENGTH,
                Span::new(start, self.pos),
                "empty char literal",
            );
            self.push(TokenKind::Char('\0'), start);
            return;
        }
        let first = self.literal_char();
        if self.eat('\'') {
            self.push(TokenKind::Char(first.unwrap_or('\0')), start);
            return;
        }
        // More than one character: find the closing quote on this line, if any.
        let line = self.rest().split('\n').next().unwrap_or("");
        match line.find('\'') {
            Some(i) => {
                self.pos += i + 1;
                self.error(
                    code::CHAR_LITERAL_LENGTH,
                    Span::new(start, self.pos),
                    "a char literal must contain exactly one character; use \"...\" for a string",
                );
            }
            None => self.error(
                code::UNTERMINATED_CHAR,
                Span::new(start, start + 1),
                "unterminated char literal",
            ),
        }
        self.push(TokenKind::Char(first.unwrap_or('\0')), start);
    }

    fn punct(&mut self, c: char, start: usize) {
        use TokenKind::*;
        self.bump();
        let with_eq = |plain, assign| (plain, Some(assign));
        let (plain, assign) = match c {
            '[' => (Some(LBracket), None),
            ']' => (Some(RBracket), None),
            '(' => (Some(LParen), None),
            ')' => (Some(RParen), None),
            ';' => (Some(Semi), None),
            ',' => (Some(Comma), None),
            '=' => (Some(Assign), None),
            '+' => with_eq(Some(Plus), PlusAssign),
            '-' => with_eq(Some(Minus), MinusAssign),
            '*' => with_eq(Some(Star), StarAssign),
            '/' => with_eq(Some(Slash), SlashAssign),
            '%' => with_eq(Some(Percent), PercentAssign),
            '<' if self.rest().starts_with("<=") => {
                self.pos += 2;
                (Some(BitwiseAssign), None)
            }
            '>' if self.rest().starts_with(">=") => {
                self.pos += 2;
                (Some(BitwiseAssign), None)
            }
            '<' => with_eq(Some(Lt), Le),
            '>' => with_eq(Some(Gt), Ge),
            '&' | '|' | '^' => (None, Some(BitwiseAssign)),
            _ => (None, None),
        };
        let kind = match assign {
            Some(assign) if self.eat('=') => Some(assign),
            _ => plain,
        };
        match kind {
            Some(kind) => self.push(kind, start),
            None => self.error(
                code::UNEXPECTED_CHARACTER,
                Span::new(start, self.pos),
                format!("unexpected character `{}`", c.escape_debug()),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Keyword as K;
    use super::*;
    use TokenKind::*;

    fn kinds(src: &str) -> Vec<TokenKind> {
        let (tokens, diagnostics) = lex(src);
        assert_eq!(diagnostics, [], "unexpected diagnostics for {src:?}");
        tokens.into_iter().map(|t| t.kind).collect()
    }

    /// Lexes source that must fail, returning the error codes and the token kinds.
    fn errors(src: &str) -> (Vec<&'static str>, Vec<TokenKind>) {
        let (tokens, diagnostics) = lex(src);
        let codes = diagnostics.iter().map(|d| d.code.0).collect();
        (codes, tokens.into_iter().map(|t| t.kind).collect())
    }

    fn ident(s: &str) -> TokenKind {
        Ident(s.to_string())
    }

    #[test]
    fn keywords_are_case_insensitive() {
        assert_eq!(
            kinds("Program PROGRAM declare Let print IF while real Char STRING"),
            [
                Keyword(K::Program),
                Keyword(K::Program),
                Keyword(K::Declare),
                Keyword(K::Let),
                Keyword(K::Print),
                Keyword(K::If),
                Keyword(K::While),
                Keyword(K::Real),
                Keyword(K::Char),
                Keyword(K::String),
                Eof
            ]
        );
    }

    #[test]
    fn contextual_keywords_are_identifiers() {
        assert_eq!(
            kinds("extern returns"),
            [ident("extern"), ident("returns"), Eof]
        );
    }

    #[test]
    fn identifiers_are_normalised() {
        // Composed and decomposed "é", in any case, are the same name.
        assert_eq!(
            kinds("Count _x1 caf\u{e9} CAFE\u{301} \u{3bb}"),
            [
                ident("count"),
                ident("_x1"),
                ident("caf\u{e9}"),
                ident("caf\u{e9}"),
                ident("\u{3bb}"),
                Eof
            ]
        );
    }

    #[test]
    fn identifier_span_keeps_the_written_text() {
        let src = "  Sqrt";
        let (tokens, _) = lex(src);
        assert_eq!(tokens[0].span, Span::new(2, 6));
        assert_eq!(&src[2..6], "Sqrt");
    }

    #[test]
    fn numbers() {
        assert_eq!(
            kinds("3 1.1 2e3 2E-3 1.5e+2 0.1"),
            [
                Number(3.0),
                Number(1.1),
                Number(2000.0),
                Number(0.002),
                Number(150.0),
                Number(0.1),
                Eof
            ]
        );
    }

    #[test]
    fn e_without_digits_is_not_an_exponent() {
        assert_eq!(kinds("2e"), [Number(2.0), ident("e"), Eof]);
        assert_eq!(kinds("2e+"), [Number(2.0), ident("e"), Plus, Eof]);
    }

    #[test]
    fn numbers_round_to_nearest_f32() {
        assert_eq!(kinds("16777217"), [Number(16777216.0), Eof]);
    }

    #[test]
    fn malformed_numbers() {
        assert_eq!(errors(".5").0, ["E0007"]);
        assert_eq!(errors("1.").0, ["E0007"]);
        assert_eq!(errors("1.;").1, [Number(0.0), Semi, Eof]);
    }

    #[test]
    fn number_too_large() {
        assert_eq!(errors("1e39").0, ["E0008"]);
        assert_eq!(kinds("1e-50"), [Number(0.0), Eof]);
    }

    #[test]
    fn strings_and_escapes() {
        assert_eq!(
            kinds(
                r#""hi" "a\n\t\r\\\"\'\0b" "é中" "" "line
break""#
            ),
            [
                Str("hi".into()),
                Str("a\n\t\r\\\"'\0b".into()),
                Str("\u{e9}\u{4e2d}".into()),
                Str(String::new()),
                Str("line\nbreak".into()),
                Eof
            ]
        );
    }

    #[test]
    fn bad_escapes() {
        assert_eq!(
            errors(r#""\q""#),
            (vec!["E0005"], vec![Str("\u{fffd}".into()), Eof])
        );
        assert_eq!(errors(r#""\u12""#).0, ["E0005"]);
        assert_eq!(errors(r#""\uD800""#).0, ["E0005"]);
    }

    #[test]
    fn unterminated_string() {
        assert_eq!(errors("\"abc").0, ["E0002"]);
        assert_eq!(errors("\"abc\\").0, ["E0002"]);
    }

    #[test]
    fn chars() {
        assert_eq!(
            kinds(r"'a' '\n' '\'' 'é' '😀'"),
            [
                Char('a'),
                Char('\n'),
                Char('\''),
                Char('\u{e9}'),
                Char('😀'),
                Eof
            ]
        );
    }

    #[test]
    fn bad_chars() {
        assert_eq!(errors("''").0, ["E0006"]);
        assert_eq!(errors("'ab';").1, [Char('a'), Semi, Eof]);
        assert_eq!(errors("'ab'").0, ["E0006"]);
        assert_eq!(errors("'a").0, ["E0003"]);
        assert_eq!(errors("'").0, ["E0003"]);
    }

    #[test]
    fn punctuation_and_operators() {
        assert_eq!(
            kinds("[ ] ( ) ; , = += -= *= /= %= + - * / % < > <= >="),
            [
                LBracket,
                RBracket,
                LParen,
                RParen,
                Semi,
                Comma,
                Assign,
                PlusAssign,
                MinusAssign,
                StarAssign,
                SlashAssign,
                PercentAssign,
                Plus,
                Minus,
                Star,
                Slash,
                Percent,
                Lt,
                Gt,
                Le,
                Ge,
                Eof
            ]
        );
    }

    #[test]
    fn bitwise_compound_assignment() {
        assert_eq!(
            kinds("&= |= ^= <<= >>="),
            [
                BitwiseAssign,
                BitwiseAssign,
                BitwiseAssign,
                BitwiseAssign,
                BitwiseAssign,
                Eof
            ]
        );
        assert_eq!(kinds("<<"), [Lt, Lt, Eof]);
    }

    #[test]
    fn unexpected_characters() {
        assert_eq!(errors("a @ b").0, ["E0001"]);
        assert_eq!(errors("a @ b").1, [ident("a"), ident("b"), Eof]);
        assert_eq!(errors("&").0, ["E0001"]);
        assert_eq!(errors(".").0, ["E0001"]);
    }

    #[test]
    fn comments() {
        assert_eq!(
            kinds("a // one\nb # two\nc /* three\n */ d /**/"),
            [ident("a"), ident("b"), ident("c"), ident("d"), Eof]
        );
        assert_eq!(kinds("/* a /* b */"), [Eof]);
    }

    #[test]
    fn unterminated_comment() {
        assert_eq!(errors("a /* b").0, ["E0004"]);
    }

    #[test]
    fn bom_is_skipped() {
        let (tokens, diagnostics) = lex("\u{feff}print");
        assert_eq!(diagnostics, []);
        assert_eq!(tokens[0].kind, Keyword(K::Print));
        assert_eq!(tokens[0].span, Span::new(3, 8));
    }

    #[test]
    fn eof_span_is_at_the_end() {
        let (tokens, _) = lex("ab ");
        assert_eq!(tokens.last().map(|t| t.span), Some(Span::new(3, 3)));
    }

    /// Each diagnostic as `(code, start, end)`.
    fn error_spans(src: &str) -> Vec<(&'static str, usize, usize)> {
        let (_, diagnostics) = lex(src);
        diagnostics
            .iter()
            .map(|d| (d.code.0, d.span.start, d.span.end))
            .collect()
    }

    #[test]
    fn diagnostic_spans() {
        assert_eq!(error_spans("a @"), [("E0001", 2, 3)]);
        assert_eq!(error_spans("x \"abc"), [("E0002", 2, 3)]);
        assert_eq!(error_spans("x 'a"), [("E0003", 2, 3)]);
        assert_eq!(error_spans("a /* b"), [("E0004", 2, 4)]);
        assert_eq!(error_spans(r#""a\qb""#), [("E0005", 2, 4)]);
        assert_eq!(error_spans(r#""\u12""#), [("E0005", 1, 3)]);
        assert_eq!(error_spans("x 'ab'"), [("E0006", 2, 6)]);
        assert_eq!(error_spans("x .5"), [("E0007", 2, 4)]);
        assert_eq!(error_spans("1."), [("E0007", 0, 2)]);
        assert_eq!(error_spans("x 1e39"), [("E0008", 2, 6)]);
    }

    #[test]
    fn token_spans() {
        let (tokens, _) = lex("let x += 1.5;");
        let spans: Vec<_> = tokens.iter().map(|t| (t.span.start, t.span.end)).collect();
        assert_eq!(spans, [(0, 3), (4, 5), (6, 8), (9, 12), (12, 13), (13, 13)]);
    }

    #[test]
    fn comment_markers_inside_strings() {
        assert_eq!(
            kinds(r##""// no" "# no" "/* no */""##),
            [
                Str("// no".into()),
                Str("# no".into()),
                Str("/* no */".into()),
                Eof
            ]
        );
    }

    #[test]
    fn comment_at_end_of_file() {
        assert_eq!(kinds("a // end"), [ident("a"), Eof]);
        assert_eq!(kinds("a #"), [ident("a"), Eof]);
    }

    #[test]
    fn unicode_whitespace() {
        assert_eq!(
            kinds("a\u{a0}b\u{2003}c\u{3000}d"),
            [ident("a"), ident("b"), ident("c"), ident("d"), Eof]
        );
    }

    #[test]
    fn numbers_with_extra_dots() {
        assert_eq!(
            errors("1.5.3"),
            (vec!["E0007"], vec![Number(1.5), Number(0.0), Eof])
        );
        assert_eq!(errors("1..2").0, ["E0007", "E0007"]);
    }

    #[test]
    fn raw_newline_char() {
        assert_eq!(kinds("'\n'"), [Char('\n'), Eof]);
    }

    #[test]
    fn escapes_cut_off_by_end_of_file() {
        assert_eq!(errors("'\\").0, ["E0003"]);
        assert_eq!(errors("\"\\").0, ["E0002"]);
        assert_eq!(errors("\"\\u").0, ["E0005", "E0002"]);
    }

    /// Strings built mostly from characters the lexer treats specially.
    fn lexer_soup() -> impl proptest::strategy::Strategy<Value = String> {
        proptest::string::string_regex(
            r#"([a-zA-Z_0-9.eE+\-*/%<>=&|^\[\]();,#'"\\u \n\t]|\u{feff}|é|\u{301}|😀|\u{a0}){0,40}"#,
        )
        .unwrap_or_else(|e| panic!("bad regex: {e}"))
    }

    fn check_invariants(src: &str) {
        let (tokens, diagnostics) = lex(src);
        let valid = |s: Span| {
            s.start <= s.end
                && s.end <= src.len()
                && src.is_char_boundary(s.start)
                && src.is_char_boundary(s.end)
        };
        for t in &tokens {
            assert!(valid(t.span), "bad token span {t:?} in {src:?}");
        }
        for d in &diagnostics {
            assert!(valid(d.span), "bad diagnostic span {d:?} in {src:?}");
        }
        for pair in tokens.windows(2) {
            assert!(
                pair[0].span.end <= pair[1].span.start,
                "overlap {pair:?} in {src:?}"
            );
        }
        let eofs = tokens.iter().filter(|t| t.kind == Eof).count();
        assert_eq!(eofs, 1);
        assert_eq!(
            tokens.last().map(|t| (&t.kind, t.span)),
            Some((&Eof, Span::new(src.len(), src.len())))
        );
    }

    proptest::proptest! {
        #[test]
        fn invariants_hold_for_any_string(src in proptest::prelude::any::<String>()) {
            check_invariants(&src);
        }

        #[test]
        fn invariants_hold_for_lexer_soup(src in lexer_soup()) {
            check_invariants(&src);
        }
    }
}
