use ariadne::{Config, IndexType, Label as AriadneLabel, Report, ReportKind, Source};

/// A byte range in the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }
}

/// A stable diagnostic code such as `E0001`. All codes are listed in [`code`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Code(pub &'static str);

/// Every diagnostic code. Codes are never reused or renumbered.
pub mod code {
    use super::Code;

    pub const UNEXPECTED_CHARACTER: Code = Code("E0001");
    pub const UNTERMINATED_STRING: Code = Code("E0002");
    pub const UNTERMINATED_CHAR: Code = Code("E0003");
    pub const UNTERMINATED_COMMENT: Code = Code("E0004");
    pub const INVALID_ESCAPE: Code = Code("E0005");
    pub const CHAR_LITERAL_LENGTH: Code = Code("E0006");
    pub const MALFORMED_NUMBER: Code = Code("E0007");
    pub const NUMBER_TOO_LARGE: Code = Code("E0008");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub span: Span,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: Code,
    pub severity: Severity,
    pub span: Span,
    pub message: String,
    pub labels: Vec<Label>,
}

impl Diagnostic {
    pub fn error(code: Code, span: Span, message: impl Into<String>) -> Self {
        Diagnostic {
            code,
            severity: Severity::Error,
            span,
            message: message.into(),
            labels: Vec::new(),
        }
    }

    pub fn warning(code: Code, span: Span, message: impl Into<String>) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            ..Diagnostic::error(code, span, message)
        }
    }

    pub fn with_label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.labels.push(Label {
            span,
            message: message.into(),
        });
        self
    }
}

/// Renders diagnostics sorted by position, as ariadne reports.
pub fn render(diagnostics: &[Diagnostic], file: &str, source: &str, color: bool) -> String {
    let mut sorted: Vec<&Diagnostic> = diagnostics.iter().collect();
    sorted.sort_by_key(|d| d.span);

    let mut out = Vec::new();
    for d in sorted {
        let kind = match d.severity {
            Severity::Error => ReportKind::Error,
            Severity::Warning => ReportKind::Warning,
        };
        let range = |s: Span| (file, s.start..s.end);
        let report = Report::build(kind, range(d.span))
            .with_config(
                Config::default()
                    .with_color(color)
                    .with_index_type(IndexType::Byte),
            )
            .with_code(d.code.0)
            .with_message(&d.message)
            .with_labels(
                d.labels
                    .iter()
                    .map(|l| AriadneLabel::new(range(l.span)).with_message(&l.message)),
            )
            .finish();
        if report
            .write((file, Source::from(source)), &mut out)
            .is_err()
        {
            // Writing to a Vec can't fail, but never lose a diagnostic.
            out.extend(format!("[{}] {}\n", d.code.0, d.message).bytes());
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST: Code = Code("E9999");

    #[test]
    fn renders_sorted_with_labels() {
        let source = "declare x real;\nprint y;\n";
        let diagnostics = [
            Diagnostic::error(TEST, Span::new(22, 23), "undeclared variable `y`")
                .with_label(Span::new(22, 23), "not declared"),
            Diagnostic::warning(TEST, Span::new(8, 9), "unused variable `x`"),
        ];
        insta::assert_snapshot!(render(&diagnostics, "t.en", source, false));
    }
}
