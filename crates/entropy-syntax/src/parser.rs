use crate::ast::*;
use crate::diagnostic::{Code, Diagnostic, Span, code};
use crate::lexer::{Keyword, Token, TokenKind, lex};

/// Maximum nesting of blocks, parentheses and operator chains together.
pub const MAX_DEPTH: usize = 256;

/// Lexes and parses a whole program. The program is always returned, with placeholders where
/// input was bad. It's only meaningful when there are no error diagnostics.
pub fn parse(src: &str) -> (Program, Vec<Diagnostic>) {
    let (tokens, diagnostics) = lex(src);
    let eof = tokens.last().cloned().unwrap_or(Token {
        kind: TokenKind::Eof,
        span: Span::new(src.len(), src.len()),
    });
    // An unterminated string or comment swallows the rest of the file, so any error at the end of
    // the file is a consequence of it.
    let lexed_to_eof = diagnostics
        .iter()
        .any(|d| d.code == code::UNTERMINATED_STRING || d.code == code::UNTERMINATED_COMMENT);
    let mut parser = Parser {
        src,
        lexed_to_eof,
        tokens,
        pos: 0,
        eof,
        prev_end: 0,
        depth: 0,
        too_deep: false,
        diagnostics,
    };
    let program = parser.program();
    (program, parser.diagnostics)
}

/// A syntax error was reported. The caller recovers.
struct Bail;

type PResult<T> = Result<T, Bail>;

struct Parser<'a> {
    src: &'a str,
    tokens: Vec<Token>,
    pos: usize,
    eof: Token,
    /// End of the last consumed token, for building spans.
    prev_end: usize,
    depth: usize,
    /// Set once the depth limit is hit. Parsing then stops.
    too_deep: bool,
    lexed_to_eof: bool,
    diagnostics: Vec<Diagnostic>,
}

fn describe(kind: &TokenKind) -> String {
    use TokenKind::*;
    let symbol = match kind {
        Keyword(k) => return format!("keyword `{}`", keyword_text(*k)),
        Ident(name) => return format!("identifier `{name}`"),
        Number(_) => return "number".into(),
        Str(_) => return "string".into(),
        Char(_) => return "char".into(),
        Eof => return "end of file".into(),
        LBracket => "[",
        RBracket => "]",
        LParen => "(",
        RParen => ")",
        Semi => ";",
        Comma => ",",
        Assign => "=",
        PlusAssign => "+=",
        MinusAssign => "-=",
        StarAssign => "*=",
        SlashAssign => "/=",
        PercentAssign => "%=",
        BitwiseAssign => "bitwise assignment",
        Plus => "+",
        Minus => "-",
        Star => "*",
        Slash => "/",
        Percent => "%",
        Lt => "<",
        Gt => ">",
        Le => "<=",
        Ge => ">=",
    };
    format!("`{symbol}`")
}

fn keyword_text(k: Keyword) -> &'static str {
    match k {
        Keyword::Program => "program",
        Keyword::Declare => "declare",
        Keyword::Let => "let",
        Keyword::Print => "print",
        Keyword::If => "if",
        Keyword::While => "while",
        Keyword::Real => "real",
        Keyword::Char => "char",
        Keyword::String => "string",
    }
}

fn type_keyword(kind: &TokenKind) -> Option<Type> {
    match kind {
        TokenKind::Keyword(Keyword::Real) => Some(Type::Real),
        TokenKind::Keyword(Keyword::Char) => Some(Type::Char),
        TokenKind::Keyword(Keyword::String) => Some(Type::String),
        _ => None,
    }
}

fn binop(kind: &TokenKind) -> Option<BinOp> {
    Some(match kind {
        TokenKind::Plus => BinOp::Add,
        TokenKind::Minus => BinOp::Sub,
        TokenKind::Star => BinOp::Mul,
        TokenKind::Slash => BinOp::Div,
        TokenKind::Percent => BinOp::Rem,
        TokenKind::Lt => BinOp::Lt,
        TokenKind::Gt => BinOp::Gt,
        TokenKind::Le => BinOp::Le,
        TokenKind::Ge => BinOp::Ge,
        _ => return None,
    })
}

fn is_comparison(op: BinOp) -> bool {
    matches!(op, BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge)
}

fn placeholder_ident(span: Span) -> Ident {
    Ident {
        name: String::new(),
        span,
    }
}

impl Parser<'_> {
    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&self.eof)
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    fn peek2_kind(&self) -> &TokenKind {
        &self.tokens.get(self.pos + 1).unwrap_or(&self.eof).kind
    }

    fn at(&self, kind: &TokenKind) -> bool {
        self.peek_kind() == kind
    }

    fn at_ident(&self, name: &str) -> bool {
        matches!(self.peek_kind(), TokenKind::Ident(n) if n == name)
    }

    fn bump(&mut self) -> Token {
        let token = self.peek().clone();
        if token.kind != TokenKind::Eof {
            self.pos += 1;
            self.prev_end = token.span.end;
        }
        token
    }

    fn span_from(&self, start: usize) -> Span {
        Span::new(start, self.prev_end.max(start))
    }

    fn error(&mut self, code: Code, span: Span, message: impl Into<String>) {
        self.diagnostics
            .push(Diagnostic::error(code, span, message));
    }

    fn unexpected(&mut self, expected: &str) -> Bail {
        if self.too_deep {
            // Parsing stopped at the depth limit, so everything after is noise.
            return Bail;
        }
        let token = self.peek().clone();
        if self.lexed_to_eof && token.kind == TokenKind::Eof {
            return Bail;
        }
        self.error(
            code::UNEXPECTED_TOKEN,
            token.span,
            format!("expected {expected}, found {}", describe(&token.kind)),
        );
        Bail
    }

    fn expect(&mut self, kind: TokenKind) -> PResult<Token> {
        if self.at(&kind) {
            Ok(self.bump())
        } else {
            Err(self.unexpected(&describe(&kind)))
        }
    }

    fn ident(&mut self) -> PResult<Ident> {
        match self.peek_kind().clone() {
            TokenKind::Ident(name) => {
                let span = self.bump().span;
                Ok(Ident { name, span })
            }
            _ => Err(self.unexpected("an identifier")),
        }
    }

    fn ty(&mut self) -> PResult<TypeRef> {
        match type_keyword(self.peek_kind()) {
            Some(ty) => Ok(TypeRef {
                ty,
                span: self.bump().span,
            }),
            None => Err(self.unexpected("a type (`real`, `char` or `string`)")),
        }
    }

    /// Enters one level of nesting. Fails, and stops all parsing, past [`MAX_DEPTH`].
    fn enter(&mut self) -> PResult<()> {
        self.depth += 1;
        if self.depth <= MAX_DEPTH {
            return Ok(());
        }
        if !self.too_deep {
            self.too_deep = true;
            let span = self.peek().span;
            self.error(
                code::NESTING_TOO_DEEP,
                span,
                format!("nesting too deep (the limit is {MAX_DEPTH} levels)"),
            );
        }
        self.pos = self.tokens.len();
        Err(Bail)
    }

    /// Skips to just after the next `;` or skipped block, or to just before the `]` closing the
    /// current block.
    fn recover(&mut self) {
        let mut depth = 0usize;
        loop {
            match self.peek_kind() {
                TokenKind::Eof => return,
                TokenKind::Semi if depth == 0 => {
                    self.bump();
                    return;
                }
                TokenKind::RBracket if depth == 0 => return,
                TokenKind::LBracket => depth += 1,
                TokenKind::RBracket => {
                    depth -= 1;
                    if depth == 0 {
                        // A whole block was skipped. Resume after it.
                        self.bump();
                        return;
                    }
                }
                _ => {}
            }
            self.bump();
        }
    }

    fn program(&mut self) -> Program {
        let start = self.peek().span.start;
        let header = self.header();
        let header_ok = header.is_ok();
        let (namespace, name) = match header {
            Ok(names) => names,
            Err(Bail) => {
                let span = self.peek().span;
                (placeholder_ident(span), placeholder_ident(span))
            }
        };
        let mut items = Vec::new();
        if self.at(&TokenKind::LBracket) {
            self.bump();
            while !self.at(&TokenKind::RBracket) && !self.at(&TokenKind::Eof) {
                if let Some(item) = self.item() {
                    items.push(item);
                }
            }
            let closed = self.expect(TokenKind::RBracket).is_ok();
            if closed && !self.at(&TokenKind::Eof) {
                self.unexpected("end of file after the program's closing `]`");
            }
        } else if header_ok {
            self.unexpected("`[`");
        }
        Program {
            namespace,
            name,
            items,
            span: self.span_from(start),
        }
    }

    fn header(&mut self) -> PResult<(Ident, Ident)> {
        if !self.at(&TokenKind::Keyword(Keyword::Program)) {
            return Err(self.unexpected("`Program <namespace> <name>`"));
        }
        self.bump();
        Ok((self.ident()?, self.ident()?))
    }

    fn item(&mut self) -> Option<Item> {
        let is_extern = self.at_ident("extern") && matches!(self.peek2_kind(), TokenKind::Ident(_));
        let result = if is_extern {
            self.extern_decl().map(Item::Extern)
        } else {
            self.stmt().map(Item::Stmt)
        };
        match result {
            Ok(item) => Some(item),
            Err(Bail) => {
                self.recover();
                None
            }
        }
    }

    fn extern_decl(&mut self) -> PResult<Extern> {
        let start = self.bump().span.start;
        let name = self.ident()?;
        let symbol = self
            .src
            .get(name.span.start..name.span.end)
            .unwrap_or_default()
            .to_string();
        self.expect(TokenKind::LParen)?;
        let mut params = Vec::new();
        if !self.at(&TokenKind::RParen) {
            params.push(self.ty()?);
            while self.at(&TokenKind::Comma) {
                self.bump();
                params.push(self.ty()?);
            }
        }
        self.expect(TokenKind::RParen)?;
        let returns = if self.at_ident("returns") {
            self.bump();
            Some(self.ty()?)
        } else {
            None
        };
        self.expect(TokenKind::Semi)?;
        Ok(Extern {
            name,
            symbol,
            params,
            returns,
            span: self.span_from(start),
        })
    }

    fn block(&mut self) -> PResult<Vec<Stmt>> {
        self.expect(TokenKind::LBracket)?;
        self.enter()?;
        let mut body = Vec::new();
        while !self.at(&TokenKind::RBracket) && !self.at(&TokenKind::Eof) {
            match self.stmt() {
                Ok(stmt) => body.push(stmt),
                Err(Bail) => self.recover(),
            }
        }
        self.depth -= 1;
        self.expect(TokenKind::RBracket)?;
        Ok(body)
    }

    fn stmt(&mut self) -> PResult<Stmt> {
        let start = self.peek().span.start;
        let kind = match self.peek_kind().clone() {
            TokenKind::Semi => {
                self.bump();
                return Ok(Stmt {
                    kind: StmtKind::Empty,
                    span: self.span_from(start),
                });
            }
            TokenKind::Keyword(Keyword::Declare) => {
                self.bump();
                let name = self.ident()?;
                let ty = self.ty()?;
                StmtKind::Declare { name, ty }
            }
            TokenKind::Keyword(Keyword::Real | Keyword::Char | Keyword::String) => {
                let ty = self.ty()?;
                let name = self.ident()?;
                self.expect(TokenKind::Assign)?;
                let value = self.expr()?;
                StmtKind::DeclareInit { ty, name, value }
            }
            TokenKind::Keyword(Keyword::Let) => {
                self.bump();
                let name = self.ident()?;
                let op = self.assign_op()?;
                let value = self.expr()?;
                StmtKind::Assign { name, op, value }
            }
            TokenKind::Keyword(Keyword::Print) => {
                self.bump();
                StmtKind::Print(self.expr()?)
            }
            TokenKind::Keyword(Keyword::If) => {
                self.bump();
                let cond = self.expr()?;
                let body = self.block()?;
                return Ok(Stmt {
                    kind: StmtKind::If { cond, body },
                    span: self.span_from(start),
                });
            }
            TokenKind::Keyword(Keyword::While) => {
                self.bump();
                let cond = self.expr()?;
                let body = self.block()?;
                return Ok(Stmt {
                    kind: StmtKind::While { cond, body },
                    span: self.span_from(start),
                });
            }
            TokenKind::Ident(name)
                if name == "extern" && matches!(self.peek2_kind(), TokenKind::Ident(_)) =>
            {
                let span = self.peek().span;
                self.error(
                    code::EXTERN_NOT_AT_TOP_LEVEL,
                    span,
                    "`extern` declarations are only allowed at the top level of the program",
                );
                return Err(Bail);
            }
            TokenKind::Ident(_) if self.peek2_kind() == &TokenKind::LParen => {
                StmtKind::Call(self.call()?)
            }
            _ => return Err(self.unexpected("a statement")),
        };
        self.expect(TokenKind::Semi)?;
        Ok(Stmt {
            kind,
            span: self.span_from(start),
        })
    }

    fn assign_op(&mut self) -> PResult<AssignOp> {
        let op = match self.peek_kind() {
            TokenKind::Assign => AssignOp::Set,
            TokenKind::PlusAssign => AssignOp::Add,
            TokenKind::MinusAssign => AssignOp::Sub,
            TokenKind::StarAssign => AssignOp::Mul,
            TokenKind::SlashAssign => AssignOp::Div,
            TokenKind::PercentAssign => AssignOp::Rem,
            TokenKind::BitwiseAssign => {
                let span = self.peek().span;
                self.error(
                    code::BITWISE_ASSIGNMENT,
                    span,
                    "bitwise compound assignment is not supported",
                );
                return Err(Bail);
            }
            _ => return Err(self.unexpected("an assignment operator")),
        };
        self.bump();
        Ok(op)
    }

    fn expr(&mut self) -> PResult<Expr> {
        self.enter()?;
        let result = self.comparison();
        self.depth -= 1;
        result
    }

    fn comparison(&mut self) -> PResult<Expr> {
        let lhs = self.binary_chain(1)?;
        let Some(op) = binop(self.peek_kind()).filter(|&op| is_comparison(op)) else {
            return Ok(lhs);
        };
        self.bump();
        let rhs = self.binary_chain(1)?;
        let expr = binary(op, lhs, rhs);
        if binop(self.peek_kind()).is_some_and(is_comparison) {
            let span = self.peek().span;
            self.error(
                code::CHAINED_COMPARISON,
                span,
                "comparisons cannot be chained",
            );
            return Err(Bail);
        }
        Ok(expr)
    }

    /// Level 1 is `+ -` and level 2 is `* / %`. Both are left-associative.
    fn binary_chain(&mut self, level: u8) -> PResult<Expr> {
        let operand = |p: &mut Self| {
            if level == 1 {
                p.binary_chain(2)
            } else {
                p.atom()
            }
        };
        let entered = self.depth;
        let mut lhs = operand(self)?;
        let result = loop {
            let op = match binop(self.peek_kind()) {
                Some(op @ (BinOp::Add | BinOp::Sub)) if level == 1 => op,
                Some(op @ (BinOp::Mul | BinOp::Div | BinOp::Rem)) if level == 2 => op,
                _ => break Ok(lhs),
            };
            // Each operator deepens the (left-leaning) tree by one.
            if let Err(bail) = self.enter() {
                break Err(bail);
            }
            self.bump();
            match operand(self) {
                Ok(rhs) => lhs = binary(op, lhs, rhs),
                Err(bail) => break Err(bail),
            }
        };
        self.depth = entered;
        result
    }

    fn atom(&mut self) -> PResult<Expr> {
        let start = self.peek().span.start;
        let kind = match self.peek_kind().clone() {
            TokenKind::Number(v) => {
                self.bump();
                ExprKind::Number(v)
            }
            TokenKind::Str(s) => {
                self.bump();
                ExprKind::Str(s)
            }
            TokenKind::Char(c) => {
                self.bump();
                ExprKind::Char(c)
            }
            TokenKind::Ident(_) if self.peek2_kind() == &TokenKind::LParen => {
                ExprKind::Call(self.call()?)
            }
            TokenKind::Ident(_) => ExprKind::Var(self.ident()?),
            TokenKind::LParen => {
                self.bump();
                let inner = self.expr()?;
                self.expect(TokenKind::RParen)?;
                return Ok(Expr {
                    span: self.span_from(start),
                    ..inner
                });
            }
            TokenKind::Minus => {
                let span = self.peek().span;
                self.error(
                    code::UNARY_MINUS,
                    span,
                    "Entropy has no unary minus (write `0 - x`)",
                );
                return Err(Bail);
            }
            _ => return Err(self.unexpected("an expression")),
        };
        Ok(Expr {
            kind,
            span: self.span_from(start),
        })
    }

    fn call(&mut self) -> PResult<Call> {
        let name = self.ident()?;
        let start = name.span.start;
        self.expect(TokenKind::LParen)?;
        let mut args = Vec::new();
        if !self.at(&TokenKind::RParen) {
            args.push(self.expr()?);
            while self.at(&TokenKind::Comma) {
                self.bump();
                args.push(self.expr()?);
            }
        }
        self.expect(TokenKind::RParen)?;
        Ok(Call {
            name,
            args,
            span: self.span_from(start),
        })
    }
}

fn binary(op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
    let span = Span::new(lhs.span.start, rhs.span.end);
    Expr {
        kind: ExprKind::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
        span,
    }
}

#[cfg(test)]
mod tests;
