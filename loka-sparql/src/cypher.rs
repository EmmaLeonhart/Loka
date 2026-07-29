//! Cypher → SPARQL transpiler.
//!
//! Translates a documented subset of Cypher into SPARQL text, which then goes
//! through the normal [`crate::parser`] and [`crate::executor`] path. Nothing
//! here touches the engine — a transpiled query is indistinguishable from a
//! hand-written one by the time it reaches the planner.
//!
//! # The property-graph → RDF mapping
//!
//! Cypher labels, relationship types and property keys are bare names; RDF
//! needs IRIs. Every bare name is placed in a single configurable namespace,
//! `http://loka.dev/` by default, emitted as the `loka:` prefix:
//!
//! | Cypher | SPARQL |
//! |---|---|
//! | `(a:Person)` | `?a rdf:type loka:Person .` |
//! | `(a {name: "Ada"})` | `?a loka:name "Ada" .` |
//! | `(a)-[:KNOWS]->(b)` | `?a loka:KNOWS ?b .` |
//! | `(a)<-[:KNOWS]-(b)` | `?b loka:KNOWS ?a .` |
//! | `WHERE a.age > 30` | `?a loka:age ?_w0 . FILTER(?_w0 > 30)` |
//! | `RETURN a.name` | `?a loka:name ?_r0 .` + `SELECT ?_r0` |
//!
//! `WHERE a.age > 30` emits a triple pattern *and* a filter because Cypher's
//! `WHERE` on a property requires that property to exist — a bare `FILTER` over
//! an unbound variable would not.
//!
//! # What is deliberately rejected
//!
//! The engine stores triples; it does not store a property graph. Constructs
//! with no faithful RDF reading are rejected with a reason rather than
//! silently given approximate semantics. See [`CypherError::Unsupported`].
//! Mutations (`CREATE`, `MERGE`, `SET`, `DELETE`, `REMOVE`) are rejected here
//! because this is a read path — SPARQL `INSERT DATA` / `DELETE DATA` are the
//! supported write surface.

use std::fmt;

/// Default namespace for Cypher labels, relationship types and property keys.
pub const DEFAULT_BASE: &str = "http://loka.dev/";

/// A transpilation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CypherError {
    /// The input is not valid Cypher (or not valid in the supported subset).
    Syntax { position: usize, message: String },
    /// The input is valid Cypher but has no faithful mapping onto RDF.
    Unsupported {
        construct: String,
        reason: &'static str,
    },
}

impl fmt::Display for CypherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CypherError::Syntax { position, message } => {
                write!(
                    f,
                    "cypher syntax error at position {}: {}",
                    position, message
                )
            }
            CypherError::Unsupported { construct, reason } => {
                write!(
                    f,
                    "unsupported Cypher construct `{}`: {}",
                    construct, reason
                )
            }
        }
    }
}

impl std::error::Error for CypherError {}

type TResult<T> = std::result::Result<T, CypherError>;

// ---------------------------------------------------------------- tokenizer

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Str(String),
    Num(String),
    /// `-[`, `]->`, `]-`, `<-[`, `->`, `<-`
    Sym(String),
    Punct(char),
}

#[derive(Debug, Clone)]
struct Spanned {
    tok: Tok,
    pos: usize,
}

fn tokenize(src: &str) -> TResult<Vec<Spanned>> {
    let b: Vec<char> = src.chars().collect();
    let mut i = 0usize;
    let mut out = Vec::new();

    while i < b.len() {
        let c = b[i];

        if c.is_whitespace() {
            i += 1;
            continue;
        }

        // Line comment: //
        if c == '/' && i + 1 < b.len() && b[i + 1] == '/' {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
            continue;
        }

        let start = i;

        // Multi-character symbols, longest first.
        let rest: String = b[i..b.len().min(i + 3)].iter().collect();
        let three = ["]->", "]-("];
        let two = ["<-", "->", "-[", "]-", "<=", ">=", "<>", "!="];
        if let Some(s) = three.iter().find(|s| rest.starts_with(**s)) {
            // "]-(" is "]-" followed by "(" — push "]-" and let "(" be re-read.
            if *s == "]-(" {
                out.push(Spanned {
                    tok: Tok::Sym("]-".into()),
                    pos: start,
                });
                i += 2;
                continue;
            }
            out.push(Spanned {
                tok: Tok::Sym((*s).to_string()),
                pos: start,
            });
            i += s.len();
            continue;
        }
        let rest2: String = b[i..b.len().min(i + 2)].iter().collect();
        if let Some(s) = two.iter().find(|s| rest2.starts_with(**s)) {
            out.push(Spanned {
                tok: Tok::Sym((*s).to_string()),
                pos: start,
            });
            i += 2;
            continue;
        }

        // Quoted string, single or double, with backslash escapes.
        if c == '"' || c == '\'' {
            let quote = c;
            i += 1;
            let mut s = String::new();
            let mut closed = false;
            while i < b.len() {
                if b[i] == '\\' && i + 1 < b.len() {
                    s.push(b[i + 1]);
                    i += 2;
                    continue;
                }
                if b[i] == quote {
                    i += 1;
                    closed = true;
                    break;
                }
                s.push(b[i]);
                i += 1;
            }
            if !closed {
                return Err(CypherError::Syntax {
                    position: start,
                    message: "unterminated string literal".into(),
                });
            }
            out.push(Spanned {
                tok: Tok::Str(s),
                pos: start,
            });
            continue;
        }

        // Backtick-quoted identifier.
        if c == '`' {
            i += 1;
            let mut s = String::new();
            let mut closed = false;
            while i < b.len() {
                if b[i] == '`' {
                    i += 1;
                    closed = true;
                    break;
                }
                s.push(b[i]);
                i += 1;
            }
            if !closed {
                return Err(CypherError::Syntax {
                    position: start,
                    message: "unterminated backtick identifier".into(),
                });
            }
            out.push(Spanned {
                tok: Tok::Ident(s),
                pos: start,
            });
            continue;
        }

        if c.is_ascii_digit()
            || (c == '-'
                && i + 1 < b.len()
                && b[i + 1].is_ascii_digit()
                && matches!(
                    out.last().map(|s| &s.tok),
                    None | Some(Tok::Punct('(')) | Some(Tok::Punct(','))
                ))
        {
            let mut s = String::new();
            if c == '-' {
                s.push('-');
                i += 1;
            }
            let mut seen_dot = false;
            while i < b.len() && (b[i].is_ascii_digit() || (b[i] == '.' && !seen_dot)) {
                if b[i] == '.' {
                    // Only consume the dot if a digit follows, so `a.b` is not a number.
                    if i + 1 >= b.len() || !b[i + 1].is_ascii_digit() {
                        break;
                    }
                    seen_dot = true;
                }
                s.push(b[i]);
                i += 1;
            }
            out.push(Spanned {
                tok: Tok::Num(s),
                pos: start,
            });
            continue;
        }

        if c.is_alphabetic() || c == '_' || c == '$' {
            let mut s = String::new();
            while i < b.len() && (b[i].is_alphanumeric() || b[i] == '_' || b[i] == '$') {
                s.push(b[i]);
                i += 1;
            }
            out.push(Spanned {
                tok: Tok::Ident(s),
                pos: start,
            });
            continue;
        }

        out.push(Spanned {
            tok: Tok::Punct(c),
            pos: start,
        });
        i += 1;
    }

    Ok(out)
}

// ------------------------------------------------------------------- parser

struct Transpiler {
    toks: Vec<Spanned>,
    i: usize,
    base: String,
    /// Emitted triple patterns, in order.
    patterns: Vec<String>,
    /// Emitted FILTER expressions.
    filters: Vec<String>,
    /// OPTIONAL groups, each a list of patterns.
    optionals: Vec<Vec<String>>,
    anon: usize,
    tmp: usize,
    /// Variables bound by patterns, so RETURN can validate against them.
    bound: Vec<String>,
}

/// Keywords that end a clause.
const CLAUSE_KWS: &[&str] = &[
    "MATCH", "OPTIONAL", "WHERE", "RETURN", "ORDER", "SKIP", "LIMIT", "WITH", "UNION", "CREATE",
    "MERGE", "SET", "DELETE", "DETACH", "REMOVE", "UNWIND", "FOREACH", "CALL",
];

/// A WHERE expression tree. Leaves are already-rendered SPARQL comparisons.
#[derive(Debug, Clone)]
enum Expr {
    Cmp(String),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
}

/// Split top-level `AND`s into separate conjuncts.
///
/// SPARQL conjoins multiple `FILTER` clauses in the same group, so each
/// conjunct can become its own `FILTER`. That sidesteps Loka's FILTER grammar,
/// which has no parenthesised grouping.
fn conjuncts(e: Expr, out: &mut Vec<Expr>) {
    match e {
        Expr::And(l, r) => {
            conjuncts(*l, out);
            conjuncts(*r, out);
        }
        other => out.push(other),
    }
}

/// Render one conjunct as the body of a single `FILTER(...)`.
///
/// Loka's FILTER grammar accepts a flat right-nested chain of comparisons
/// joined by `&&` / `||` and does **not** accept a parenthesised expression in
/// operand position (`parser.rs::parse_comparison_expr` expects a term). So a
/// conjunct is representable only if it is a single comparison or a pure `OR`
/// chain of comparisons. Anything else — an `OR` with a compound left side,
/// such as `(a AND b) OR c` — has no equivalent in that grammar and is
/// rejected rather than mis-emitted.
fn render_conjunct(e: &Expr) -> TResult<String> {
    fn or_chain(e: &Expr, out: &mut Vec<String>) -> TResult<()> {
        match e {
            Expr::Cmp(s) => {
                out.push(s.clone());
                Ok(())
            }
            Expr::Or(l, r) => {
                or_chain(l, out)?;
                or_chain(r, out)
            }
            Expr::And(_, _) => unsupported(
                "AND nested inside OR",
                "Loka's FILTER grammar is a flat chain of comparisons with no parenthesised \
                 grouping, so a disjunction with a conjunctive branch cannot be expressed. \
                 Rewrite it in disjunctive normal form, or write the FILTER directly in SPARQL.",
            ),
        }
    }

    match e {
        Expr::Cmp(s) => Ok(s.clone()),
        _ => {
            let mut parts = Vec::new();
            or_chain(e, &mut parts)?;
            Ok(parts.join(" || "))
        }
    }
}

/// The negation of a comparison operator, used to push `NOT` down to the leaves.
fn invert_op(op: &str) -> &'static str {
    match op {
        "=" => "!=",
        "!=" => "=",
        "<" => ">=",
        ">" => "<=",
        "<=" => ">",
        ">=" => "<",
        other => unreachable!("unknown comparison operator {}", other),
    }
}

fn unsupported<T>(construct: &str, reason: &'static str) -> TResult<T> {
    Err(CypherError::Unsupported {
        construct: construct.to_string(),
        reason,
    })
}

impl Transpiler {
    fn new(toks: Vec<Spanned>, base: &str) -> Self {
        Transpiler {
            toks,
            i: 0,
            base: base.to_string(),
            patterns: Vec::new(),
            filters: Vec::new(),
            optionals: Vec::new(),
            anon: 0,
            tmp: 0,
            bound: Vec::new(),
        }
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i).map(|s| &s.tok)
    }

    fn pos(&self) -> usize {
        self.toks
            .get(self.i)
            .map(|s| s.pos)
            .unwrap_or_else(|| self.toks.last().map(|s| s.pos).unwrap_or(0))
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.i).map(|s| s.tok.clone());
        if t.is_some() {
            self.i += 1;
        }
        t
    }

    /// Case-insensitive keyword match without consuming.
    fn peek_kw(&self, kw: &str) -> bool {
        matches!(self.peek(), Some(Tok::Ident(s)) if s.eq_ignore_ascii_case(kw))
    }

    fn eat_kw(&mut self, kw: &str) -> bool {
        if self.peek_kw(kw) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn expect_punct(&mut self, c: char) -> TResult<()> {
        match self.next() {
            Some(Tok::Punct(g)) if g == c => Ok(()),
            other => Err(CypherError::Syntax {
                position: self.pos(),
                message: format!("expected `{}`, found {:?}", c, other),
            }),
        }
    }

    fn fresh_anon(&mut self) -> String {
        let v = format!("_n{}", self.anon);
        self.anon += 1;
        v
    }

    fn fresh_tmp(&mut self) -> String {
        let v = format!("_t{}", self.tmp);
        self.tmp += 1;
        v
    }

    /// `loka:Name`, escaping anything that is not a bare PN_LOCAL.
    fn iri(&self, name: &str) -> String {
        if name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            && !name.starts_with(|c: char| c.is_ascii_digit())
        {
            format!("loka:{}", name)
        } else {
            format!("<{}{}>", self.base, name)
        }
    }

    // -------------------------------------------------------------- patterns

    /// Parse a node pattern `( [var] [:Label]* [{k: v, ...}] )`, emit its
    /// triples, and return the variable name bound to it.
    fn node(&mut self, into: &mut Vec<String>) -> TResult<String> {
        self.expect_punct('(')?;

        let var = match self.peek() {
            Some(Tok::Ident(s)) => {
                let s = s.clone();
                self.i += 1;
                self.bound.push(s.clone());
                s
            }
            _ => self.fresh_anon(),
        };

        // Labels
        while matches!(self.peek(), Some(Tok::Punct(':'))) {
            self.i += 1;
            match self.next() {
                Some(Tok::Ident(label)) => {
                    into.push(format!("?{} rdf:type {} .", var, self.iri(&label)));
                }
                other => {
                    return Err(CypherError::Syntax {
                        position: self.pos(),
                        message: format!("expected label after `:`, found {:?}", other),
                    })
                }
            }
        }

        // Inline property map
        if matches!(self.peek(), Some(Tok::Punct('{'))) {
            self.i += 1;
            loop {
                let key = match self.next() {
                    Some(Tok::Ident(k)) => k,
                    Some(Tok::Str(k)) => k,
                    other => {
                        return Err(CypherError::Syntax {
                            position: self.pos(),
                            message: format!("expected property key, found {:?}", other),
                        })
                    }
                };
                self.expect_punct(':')?;
                let value = self.literal()?;
                into.push(format!("?{} {} {} .", var, self.iri(&key), value));

                match self.peek() {
                    Some(Tok::Punct(',')) => {
                        self.i += 1;
                    }
                    Some(Tok::Punct('}')) => {
                        self.i += 1;
                        break;
                    }
                    other => {
                        return Err(CypherError::Syntax {
                            position: self.pos(),
                            message: format!(
                                "expected `,` or `}}` in property map, found {:?}",
                                other
                            ),
                        })
                    }
                }
            }
        }

        self.expect_punct(')')?;
        Ok(var)
    }

    /// A scalar literal in value position.
    fn literal(&mut self) -> TResult<String> {
        match self.next() {
            Some(Tok::Str(s)) => Ok(format!(
                "\"{}\"",
                s.replace('\\', "\\\\").replace('"', "\\\"")
            )),
            Some(Tok::Num(n)) => Ok(n),
            Some(Tok::Ident(id)) if id.eq_ignore_ascii_case("true") => Ok("true".into()),
            Some(Tok::Ident(id)) if id.eq_ignore_ascii_case("false") => Ok("false".into()),
            Some(Tok::Ident(id)) if id.eq_ignore_ascii_case("null") => unsupported(
                "NULL",
                "RDF has no null; an absent property is an absent triple. Use OPTIONAL plus \
                 `FILTER(!BOUND(?x))` in SPARQL instead.",
            ),
            Some(Tok::Punct('[')) => unsupported(
                "list literal",
                "RDF has no native list-valued property; a Cypher list property has no faithful \
                 single-triple reading.",
            ),
            Some(Tok::Punct('{')) => unsupported(
                "map literal",
                "RDF has no native map-valued property; nest entities instead.",
            ),
            other => Err(CypherError::Syntax {
                position: self.pos(),
                message: format!("expected a literal value, found {:?}", other),
            }),
        }
    }

    /// Parse one `MATCH` path: node (rel node)*.
    fn path(&mut self, into: &mut Vec<String>) -> TResult<()> {
        let mut left = self.node(into)?;

        loop {
            // Relationship, in one of: -[..]->  <-[..]-  -[..]-
            let inbound = match self.peek() {
                Some(Tok::Sym(s)) if s == "-[" => false,
                Some(Tok::Sym(s)) if s == "<-" => {
                    // `<-[` tokenizes as `<-` then `[`
                    self.i += 1;
                    match self.peek() {
                        Some(Tok::Punct('[')) => {
                            self.i += 1;
                        }
                        other => {
                            return Err(CypherError::Syntax {
                                position: self.pos(),
                                message: format!("expected `[` after `<-`, found {:?}", other),
                            })
                        }
                    }
                    true
                }
                Some(Tok::Sym(s)) if s == "->" || s == "]-" || s == "]->" => {
                    return Err(CypherError::Syntax {
                        position: self.pos(),
                        message: format!("unexpected `{}`", s),
                    })
                }
                _ => break,
            };
            if !inbound {
                self.i += 1; // consume `-[`
            }

            // Optional relationship variable.
            if let Some(Tok::Ident(v)) = self.peek().cloned() {
                if !v.eq_ignore_ascii_case("null") {
                    return unsupported(
                        "relationship variable",
                        "a bound relationship variable implies edge properties, which need \
                         RDF-star quoted triples rather than a plain predicate. Query the quoted \
                         triple directly in SPARQL.",
                    );
                }
            }

            let rel_type = if matches!(self.peek(), Some(Tok::Punct(':'))) {
                self.i += 1;
                match self.next() {
                    Some(Tok::Ident(t)) => t,
                    other => {
                        return Err(CypherError::Syntax {
                            position: self.pos(),
                            message: format!("expected relationship type, found {:?}", other),
                        })
                    }
                }
            } else {
                return unsupported(
                    "untyped relationship",
                    "an untyped `-[]->` matches any predicate; write the predicate as a variable \
                     in SPARQL if that is what you want.",
                );
            };

            // Variable-length paths `*`, `*1..3`
            if matches!(self.peek(), Some(Tok::Punct('*'))) {
                return unsupported(
                    "variable-length relationship",
                    "Cypher's `*n..m` bounds do not map onto SPARQL property paths, which have \
                     only `*` and `+` with no length bounds. Use a property path directly.",
                );
            }

            // Relationship property map
            if matches!(self.peek(), Some(Tok::Punct('{'))) {
                return unsupported(
                    "relationship properties",
                    "edge properties need RDF-star quoted triples rather than a plain predicate.",
                );
            }

            // Closing bracket + direction
            let outbound = match self.next() {
                Some(Tok::Sym(s)) if s == "]->" => true,
                Some(Tok::Sym(s)) if s == "]-" => {
                    if inbound {
                        false
                    } else {
                        return unsupported(
                            "undirected relationship",
                            "RDF triples are directed; an undirected match would need a UNION of \
                             both directions. Write that UNION explicitly in SPARQL.",
                        );
                    }
                }
                Some(Tok::Punct(']')) => {
                    // `]` followed by `-` handled as Sym above; a bare `]` means undirected tail.
                    return unsupported(
                        "undirected relationship",
                        "RDF triples are directed; an undirected match would need a UNION of both \
                         directions. Write that UNION explicitly in SPARQL.",
                    );
                }
                other => {
                    return Err(CypherError::Syntax {
                        position: self.pos(),
                        message: format!("expected `]->` or `]-`, found {:?}", other),
                    })
                }
            };

            let right = self.node(into)?;

            if inbound && !outbound {
                into.push(format!("?{} {} ?{} .", right, self.iri(&rel_type), left));
            } else {
                into.push(format!("?{} {} ?{} .", left, self.iri(&rel_type), right));
            }

            left = right;
        }

        Ok(())
    }

    // ----------------------------------------------------------------- WHERE

    /// Parse a WHERE expression into a SPARQL filter expression string,
    /// emitting any property-access triple patterns it needs.
    ///
    /// Negation is pushed down to the comparison leaves (De Morgan on `AND`/`OR`,
    /// operator inversion on comparisons) rather than emitted as a `!` wrapper.
    /// This is not a stylistic choice: Loka's own FILTER grammar only accepts `!`
    /// in leading position (`parser.rs` consumes `!`, parses one inner
    /// expression, then expects the FILTER's closing paren), so a nested
    /// `(a && !(b))` fails to parse. Pushing negation to the leaves keeps the
    /// emitted query inside the grammar the engine actually implements, and is
    /// exactly equivalent over this subset, which has no three-valued logic.
    fn where_expr(&mut self) -> TResult<Expr> {
        self.or_expr(false)
    }

    fn or_expr(&mut self, neg: bool) -> TResult<Expr> {
        let lhs = self.and_expr(neg)?;
        if self.peek_kw("OR") {
            self.i += 1;
            let rhs = self.or_expr(neg)?;
            // De Morgan: !(a || b) == (!a && !b)
            return Ok(if neg {
                Expr::And(Box::new(lhs), Box::new(rhs))
            } else {
                Expr::Or(Box::new(lhs), Box::new(rhs))
            });
        }
        Ok(lhs)
    }

    fn and_expr(&mut self, neg: bool) -> TResult<Expr> {
        let lhs = self.not_expr(neg)?;
        if self.peek_kw("AND") {
            self.i += 1;
            let rhs = self.and_expr(neg)?;
            // De Morgan: !(a && b) == (!a || !b)
            return Ok(if neg {
                Expr::Or(Box::new(lhs), Box::new(rhs))
            } else {
                Expr::And(Box::new(lhs), Box::new(rhs))
            });
        }
        Ok(lhs)
    }

    fn not_expr(&mut self, neg: bool) -> TResult<Expr> {
        if self.eat_kw("NOT") {
            return self.not_expr(!neg);
        }
        self.cmp_expr(neg)
    }

    fn cmp_expr(&mut self, neg: bool) -> TResult<Expr> {
        if matches!(self.peek(), Some(Tok::Punct('('))) {
            self.i += 1;
            let inner = self.or_expr(neg)?;
            self.expect_punct(')')?;
            return Ok(inner);
        }

        let lhs = self.operand()?;

        let op = match self.peek() {
            Some(Tok::Punct('=')) => {
                self.i += 1;
                "="
            }
            Some(Tok::Punct('<')) => {
                self.i += 1;
                "<"
            }
            Some(Tok::Punct('>')) => {
                self.i += 1;
                ">"
            }
            Some(Tok::Sym(s)) if s == "<=" => {
                self.i += 1;
                "<="
            }
            Some(Tok::Sym(s)) if s == ">=" => {
                self.i += 1;
                ">="
            }
            Some(Tok::Sym(s)) if s == "<>" || s == "!=" => {
                self.i += 1;
                "!="
            }
            _ => {
                return Err(CypherError::Syntax {
                    position: self.pos(),
                    message: "expected a comparison operator in WHERE".into(),
                })
            }
        };

        let op = if neg { invert_op(op) } else { op };
        let rhs = self.operand()?;
        Ok(Expr::Cmp(format!("{} {} {}", lhs, op, rhs)))
    }

    /// A WHERE operand: `var.prop` (emits a pattern, yields a temp var) or a literal.
    fn operand(&mut self) -> TResult<String> {
        if let Some(Tok::Ident(name)) = self.peek().cloned() {
            // Reserved words that are not operands.
            if ["AND", "OR", "NOT"]
                .iter()
                .any(|k| name.eq_ignore_ascii_case(k))
            {
                return Err(CypherError::Syntax {
                    position: self.pos(),
                    message: format!("unexpected `{}` in operand position", name),
                });
            }
            if name.eq_ignore_ascii_case("true") || name.eq_ignore_ascii_case("false") {
                return self.literal();
            }
            if name.eq_ignore_ascii_case("null") {
                return self.literal();
            }
            self.i += 1;
            if matches!(self.peek(), Some(Tok::Punct('.'))) {
                self.i += 1;
                let prop = match self.next() {
                    Some(Tok::Ident(p)) => p,
                    other => {
                        return Err(CypherError::Syntax {
                            position: self.pos(),
                            message: format!("expected property name after `.`, found {:?}", other),
                        })
                    }
                };
                let t = self.fresh_tmp();
                self.patterns
                    .push(format!("?{} {} ?{} .", name, self.iri(&prop), t));
                return Ok(format!("?{}", t));
            }
            // A bare variable compared directly (node identity).
            return Ok(format!("?{}", name));
        }
        self.literal()
    }

    // ---------------------------------------------------------------- driver

    fn run(&mut self) -> TResult<String> {
        // Reject unsupported leading clauses up front with a precise reason.
        if let Some(Tok::Ident(kw)) = self.peek().cloned() {
            self.reject_unsupported_clause(&kw)?;
        }

        let mut saw_match = false;

        loop {
            if self.eat_kw("MATCH") {
                saw_match = true;
                let mut pats = Vec::new();
                loop {
                    self.path(&mut pats)?;
                    if matches!(self.peek(), Some(Tok::Punct(','))) {
                        self.i += 1;
                        continue;
                    }
                    break;
                }
                self.patterns.extend(pats);
                continue;
            }

            if self.peek_kw("OPTIONAL") {
                self.i += 1;
                if !self.eat_kw("MATCH") {
                    return Err(CypherError::Syntax {
                        position: self.pos(),
                        message: "expected MATCH after OPTIONAL".into(),
                    });
                }
                saw_match = true;
                let mut pats = Vec::new();
                loop {
                    self.path(&mut pats)?;
                    if matches!(self.peek(), Some(Tok::Punct(','))) {
                        self.i += 1;
                        continue;
                    }
                    break;
                }
                self.optionals.push(pats);
                continue;
            }

            if self.eat_kw("WHERE") {
                let e = self.where_expr()?;
                let mut parts = Vec::new();
                conjuncts(e, &mut parts);
                for c in &parts {
                    self.filters.push(render_conjunct(c)?);
                }
                continue;
            }

            break;
        }

        if !saw_match {
            return Err(CypherError::Syntax {
                position: self.pos(),
                message: "query has no MATCH clause".into(),
            });
        }

        if let Some(Tok::Ident(kw)) = self.peek().cloned() {
            self.reject_unsupported_clause(&kw)?;
        }

        if !self.eat_kw("RETURN") {
            return Err(CypherError::Syntax {
                position: self.pos(),
                message: "query has no RETURN clause".into(),
            });
        }

        let distinct = self.eat_kw("DISTINCT");

        let mut projection: Vec<String> = Vec::new();
        loop {
            match self.peek().cloned() {
                Some(Tok::Punct('*')) => {
                    self.i += 1;
                    projection.push("*".into());
                }
                Some(Tok::Ident(name)) => {
                    // Aggregates
                    let is_call = matches!(
                        self.toks.get(self.i + 1).map(|s| &s.tok),
                        Some(Tok::Punct('('))
                    );
                    if is_call {
                        return unsupported(
                            &format!("{}(...)", name),
                            "aggregate and function calls are not transpiled yet; write the \
                             aggregate directly in SPARQL.",
                        );
                    }
                    self.i += 1;
                    if matches!(self.peek(), Some(Tok::Punct('.'))) {
                        self.i += 1;
                        let prop = match self.next() {
                            Some(Tok::Ident(p)) => p,
                            other => {
                                return Err(CypherError::Syntax {
                                    position: self.pos(),
                                    message: format!(
                                        "expected property name after `.`, found {:?}",
                                        other
                                    ),
                                })
                            }
                        };
                        let t = self.fresh_tmp();
                        self.patterns
                            .push(format!("?{} {} ?{} .", name, self.iri(&prop), t));
                        projection.push(format!("?{}", t));
                    } else {
                        projection.push(format!("?{}", name));
                    }

                    // AS alias
                    if self.eat_kw("AS") {
                        match self.next() {
                            Some(Tok::Ident(_alias)) => {
                                return unsupported(
                                    "RETURN ... AS",
                                    "SPARQL SELECT aliasing needs an expression binding; write \
                                     `(expr AS ?name)` directly in SPARQL.",
                                );
                            }
                            other => {
                                return Err(CypherError::Syntax {
                                    position: self.pos(),
                                    message: format!("expected alias after AS, found {:?}", other),
                                })
                            }
                        }
                    }
                }
                other => {
                    return Err(CypherError::Syntax {
                        position: self.pos(),
                        message: format!("expected a RETURN item, found {:?}", other),
                    })
                }
            }

            if matches!(self.peek(), Some(Tok::Punct(','))) {
                self.i += 1;
                continue;
            }
            break;
        }

        // ORDER BY / SKIP / LIMIT
        let mut order_by: Option<String> = None;
        let mut skip: Option<String> = None;
        let mut limit: Option<String> = None;

        loop {
            if self.eat_kw("ORDER") {
                if !self.eat_kw("BY") {
                    return Err(CypherError::Syntax {
                        position: self.pos(),
                        message: "expected BY after ORDER".into(),
                    });
                }
                let mut items = Vec::new();
                loop {
                    let v = match self.next() {
                        Some(Tok::Ident(name)) => {
                            if matches!(self.peek(), Some(Tok::Punct('.'))) {
                                self.i += 1;
                                let prop = match self.next() {
                                    Some(Tok::Ident(p)) => p,
                                    other => {
                                        return Err(CypherError::Syntax {
                                            position: self.pos(),
                                            message: format!(
                                                "expected property after `.`, found {:?}",
                                                other
                                            ),
                                        })
                                    }
                                };
                                let t = self.fresh_tmp();
                                self.patterns.push(format!(
                                    "?{} {} ?{} .",
                                    name,
                                    self.iri(&prop),
                                    t
                                ));
                                format!("?{}", t)
                            } else {
                                format!("?{}", name)
                            }
                        }
                        other => {
                            return Err(CypherError::Syntax {
                                position: self.pos(),
                                message: format!("expected ORDER BY item, found {:?}", other),
                            })
                        }
                    };
                    let dir = if self.eat_kw("DESC") {
                        "DESC"
                    } else {
                        let _ = self.eat_kw("ASC");
                        "ASC"
                    };
                    items.push(format!("{}({})", dir, v));
                    if matches!(self.peek(), Some(Tok::Punct(','))) {
                        self.i += 1;
                        continue;
                    }
                    break;
                }
                order_by = Some(items.join(" "));
                continue;
            }
            if self.eat_kw("SKIP") {
                match self.next() {
                    Some(Tok::Num(n)) => skip = Some(n),
                    other => {
                        return Err(CypherError::Syntax {
                            position: self.pos(),
                            message: format!("expected a number after SKIP, found {:?}", other),
                        })
                    }
                }
                continue;
            }
            if self.eat_kw("LIMIT") {
                match self.next() {
                    Some(Tok::Num(n)) => limit = Some(n),
                    other => {
                        return Err(CypherError::Syntax {
                            position: self.pos(),
                            message: format!("expected a number after LIMIT, found {:?}", other),
                        })
                    }
                }
                continue;
            }
            break;
        }

        if let Some(Tok::Ident(kw)) = self.peek().cloned() {
            self.reject_unsupported_clause(&kw)?;
        }
        if self.i < self.toks.len() {
            return Err(CypherError::Syntax {
                position: self.pos(),
                message: format!("unexpected trailing input: {:?}", self.peek()),
            });
        }

        // ------------------------------------------------------------- emit
        let mut out = String::new();
        out.push_str("PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>\n");
        out.push_str(&format!("PREFIX loka: <{}>\n", self.base));

        let proj = if projection.iter().any(|p| p == "*") {
            "*".to_string()
        } else {
            projection.join(" ")
        };
        out.push_str(&format!(
            "SELECT {}{}\nWHERE {{\n",
            if distinct { "DISTINCT " } else { "" },
            proj
        ));

        for p in &self.patterns {
            out.push_str(&format!("  {}\n", p));
        }
        for group in &self.optionals {
            out.push_str("  OPTIONAL {\n");
            for p in group {
                out.push_str(&format!("    {}\n", p));
            }
            out.push_str("  }\n");
        }
        for f in &self.filters {
            out.push_str(&format!("  FILTER({})\n", f));
        }
        out.push('}');

        if let Some(o) = order_by {
            out.push_str(&format!("\nORDER BY {}", o));
        }
        if let Some(l) = limit {
            out.push_str(&format!("\nLIMIT {}", l));
        }
        if let Some(s) = skip {
            out.push_str(&format!("\nOFFSET {}", s));
        }
        out.push('\n');

        Ok(out)
    }

    fn reject_unsupported_clause(&self, kw: &str) -> TResult<()> {
        let upper = kw.to_ascii_uppercase();
        let reason: Option<&'static str> = match upper.as_str() {
            "CREATE" | "MERGE" | "SET" | "DELETE" | "DETACH" | "REMOVE" => Some(
                "this is a read-only transpiler; use SPARQL INSERT DATA / DELETE DATA for writes.",
            ),
            "WITH" => Some(
                "WITH chains query parts through an intermediate projection, which has no direct \
                 SPARQL equivalent short of a subquery. Write the subquery in SPARQL.",
            ),
            "UNWIND" => {
                Some("UNWIND expands a list into rows; RDF has no list-valued bindings to expand.")
            }
            "FOREACH" => Some("FOREACH is a mutation construct; this transpiler is read-only."),
            "CALL" => Some("procedure calls have no SPARQL equivalent."),
            "UNION" => Some(
                "Cypher UNION combines full result sets; SPARQL UNION combines graph patterns. \
                 Write the UNION directly in SPARQL.",
            ),
            _ => None,
        };
        if let Some(reason) = reason {
            if CLAUSE_KWS.contains(&upper.as_str()) {
                return unsupported(&upper, reason);
            }
        }
        Ok(())
    }
}

/// Transpile Cypher to SPARQL using the default namespace.
pub fn transpile(cypher: &str) -> TResult<String> {
    transpile_with_base(cypher, DEFAULT_BASE)
}

/// Transpile Cypher to SPARQL, placing bare Cypher names in `base`.
pub fn transpile_with_base(cypher: &str, base: &str) -> TResult<String> {
    let toks = tokenize(cypher)?;
    if toks.is_empty() {
        return Err(CypherError::Syntax {
            position: 0,
            message: "empty query".into(),
        });
    }
    Transpiler::new(toks, base).run()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(c: &str) -> String {
        transpile(c).unwrap_or_else(|e| panic!("transpile failed for {:?}: {}", c, e))
    }

    /// Every emitted query must survive the real SPARQL parser — the point of
    /// the transpiler is that downstream sees an ordinary query.
    fn parses(sparql: &str) {
        crate::parser::parse(sparql)
            .unwrap_or_else(|e| panic!("emitted SPARQL did not parse: {}\n---\n{}", e, sparql));
    }

    #[test]
    fn label_match() {
        let out = t("MATCH (a:Person) RETURN a");
        assert!(out.contains("?a rdf:type loka:Person ."), "{}", out);
        assert!(out.contains("SELECT ?a"), "{}", out);
        parses(&out);
    }

    #[test]
    fn relationship_direction() {
        let out = t("MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a, b");
        assert!(out.contains("?a loka:KNOWS ?b ."), "{}", out);
        parses(&out);
    }

    #[test]
    fn inbound_relationship_is_reversed() {
        let out = t("MATCH (a)<-[:KNOWS]-(b) RETURN a, b");
        // b KNOWS a
        assert!(out.contains("?b loka:KNOWS ?a ."), "{}", out);
        parses(&out);
    }

    #[test]
    fn inline_properties() {
        let out = t("MATCH (a:Person {name: \"Ada\"}) RETURN a");
        assert!(out.contains("?a loka:name \"Ada\" ."), "{}", out);
        parses(&out);
    }

    #[test]
    fn where_emits_pattern_and_filter() {
        let out = t("MATCH (a:Person) WHERE a.age > 30 RETURN a");
        // The property must be bound by a pattern, not filtered while unbound.
        assert!(out.contains("?a loka:age ?_t0 ."), "{}", out);
        assert!(out.contains("FILTER(?_t0 > 30)"), "{}", out);
        parses(&out);
    }

    #[test]
    fn where_and_or_not() {
        let out = t("MATCH (a) WHERE a.x = 1 AND (a.y = 2 OR NOT a.z = 3) RETURN a");
        // Top-level ANDs become separate FILTER clauses (SPARQL conjoins them),
        // which is how a grouped filter is expressed without parenthesised
        // grouping in Loka's FILTER grammar.
        assert!(out.contains("FILTER(?_t0 = 1)"), "{}", out);
        assert!(out.contains("FILTER(?_t1 = 2 || ?_t2 != 3)"), "{}", out);
        // NOT is pushed into the operator, not emitted as a nested `!`.
        assert!(!out.contains('!') || out.contains("!="), "{}", out);
        parses(&out);
    }

    #[test]
    fn top_level_and_becomes_separate_filters() {
        let out = t("MATCH (a) WHERE a.x = 1 AND a.y = 2 AND a.z = 3 RETURN a");
        assert_eq!(out.matches("FILTER(").count(), 3, "{}", out);
        assert!(!out.contains("&&"), "{}", out);
        parses(&out);
    }

    #[test]
    fn not_inverts_each_comparison_operator() {
        for (cy, sparql) in [
            ("=", "!="),
            ("<>", "="),
            ("<", ">="),
            (">", "<="),
            ("<=", ">"),
            (">=", "<"),
        ] {
            let q = format!("MATCH (a) WHERE NOT a.x {} 1 RETURN a", cy);
            let out = t(&q);
            assert!(
                out.contains(&format!("?_t0 {} 1", sparql)),
                "NOT {} should emit {}: {}",
                cy,
                sparql,
                out
            );
            parses(&out);
        }
    }

    #[test]
    fn not_over_group_applies_de_morgan() {
        // !(x = 1 AND y = 2) == (x != 1 OR y != 2) — one FILTER, disjunctive.
        let out = t("MATCH (a) WHERE NOT (a.x = 1 AND a.y = 2) RETURN a");
        assert!(
            out.contains("FILTER(?_t0 != 1 || ?_t1 != 2)"),
            "expected a single disjunctive FILTER after De Morgan: {}",
            out
        );
        parses(&out);

        // !(x = 1 OR y = 2) == (x != 1 AND y != 2) — a conjunction, so it
        // becomes two separate FILTER clauses.
        let out = t("MATCH (a) WHERE NOT (a.x = 1 OR a.y = 2) RETURN a");
        assert!(out.contains("FILTER(?_t0 != 1)"), "{}", out);
        assert!(out.contains("FILTER(?_t1 != 2)"), "{}", out);
        parses(&out);
    }

    #[test]
    fn rejects_conjunction_nested_in_disjunction() {
        // (a AND b) OR c has no reading in a grammar without grouping.
        rejects(
            "MATCH (a) WHERE (a.x = 1 AND a.y = 2) OR a.z = 3 RETURN a",
            "disjunctive normal form",
        );
    }

    #[test]
    fn double_negation_cancels() {
        let out = t("MATCH (a) WHERE NOT NOT a.x = 1 RETURN a");
        assert!(out.contains("?_t0 = 1"), "{}", out);
        parses(&out);
    }

    #[test]
    fn return_property_projects_temp() {
        let out = t("MATCH (a:Person) RETURN a.name");
        assert!(out.contains("?a loka:name ?_t0 ."), "{}", out);
        assert!(out.contains("SELECT ?_t0"), "{}", out);
        parses(&out);
    }

    #[test]
    fn distinct_order_limit_skip() {
        let out = t("MATCH (a:Person) RETURN DISTINCT a ORDER BY a.name DESC SKIP 5 LIMIT 10");
        assert!(out.contains("SELECT DISTINCT"), "{}", out);
        assert!(out.contains("ORDER BY DESC(?_t0)"), "{}", out);
        assert!(out.contains("LIMIT 10"), "{}", out);
        assert!(out.contains("OFFSET 5"), "{}", out);
        parses(&out);
    }

    #[test]
    fn optional_match_becomes_optional_block() {
        let out = t("MATCH (a:Person) OPTIONAL MATCH (a)-[:KNOWS]->(b) RETURN a, b");
        assert!(out.contains("OPTIONAL {"), "{}", out);
        assert!(out.contains("?a loka:KNOWS ?b ."), "{}", out);
        parses(&out);
    }

    #[test]
    fn comma_separated_paths() {
        let out = t("MATCH (a:Person), (b:City) RETURN a, b");
        assert!(out.contains("?a rdf:type loka:Person ."), "{}", out);
        assert!(out.contains("?b rdf:type loka:City ."), "{}", out);
        parses(&out);
    }

    #[test]
    fn multi_hop_chain() {
        let out = t("MATCH (a)-[:R1]->(b)-[:R2]->(c) RETURN a, c");
        assert!(out.contains("?a loka:R1 ?b ."), "{}", out);
        assert!(out.contains("?b loka:R2 ?c ."), "{}", out);
        parses(&out);
    }

    #[test]
    fn anonymous_nodes_get_fresh_vars() {
        let out = t("MATCH (a:Person)-[:LIVES_IN]->(:City) RETURN a");
        assert!(out.contains("?a loka:LIVES_IN ?_n0 ."), "{}", out);
        parses(&out);
    }

    #[test]
    fn custom_base_namespace() {
        let out = transpile_with_base("MATCH (a:Person) RETURN a", "http://example.org/").unwrap();
        assert!(
            out.contains("PREFIX loka: <http://example.org/>"),
            "{}",
            out
        );
    }

    // ---- rejections: each must name the construct and give a reason ----

    fn rejects(c: &str, needle: &str) {
        match transpile(c) {
            Err(CypherError::Unsupported { construct, reason }) => {
                assert!(
                    construct.to_lowercase().contains(needle) || reason.contains(needle),
                    "wrong rejection for {:?}: {} / {}",
                    c,
                    construct,
                    reason
                );
            }
            other => panic!("expected Unsupported for {:?}, got {:?}", c, other),
        }
    }

    #[test]
    fn rejects_mutations() {
        rejects("CREATE (a:Person) RETURN a", "read-only");
        rejects("MERGE (a:Person) RETURN a", "read-only");
    }

    #[test]
    fn rejects_variable_length_paths() {
        rejects("MATCH (a)-[:KNOWS*1..3]->(b) RETURN a", "property path");
    }

    #[test]
    fn rejects_undirected() {
        rejects("MATCH (a)-[:KNOWS]-(b) RETURN a", "directed");
    }

    #[test]
    fn rejects_relationship_variable() {
        rejects("MATCH (a)-[r:KNOWS]->(b) RETURN a", "RDF-star");
    }

    #[test]
    fn rejects_untyped_relationship() {
        rejects("MATCH (a)-[]->(b) RETURN a", "any predicate");
    }

    #[test]
    fn rejects_with_and_unwind() {
        rejects("MATCH (a) WITH a RETURN a", "subquery");
        rejects("UNWIND [1,2] AS x RETURN x", "list");
    }

    #[test]
    fn rejects_aggregates_explicitly() {
        rejects("MATCH (a:Person) RETURN count(a)", "aggregate");
    }

    #[test]
    fn rejects_null_literal() {
        rejects("MATCH (a) WHERE a.x = null RETURN a", "BOUND");
    }

    #[test]
    fn syntax_errors_carry_position() {
        match transpile("MATCH (a:Person RETURN a") {
            Err(CypherError::Syntax { .. }) => {}
            other => panic!("expected Syntax error, got {:?}", other),
        }
        match transpile("MATCH (a:Person) RETURN") {
            Err(CypherError::Syntax { .. }) => {}
            other => panic!("expected Syntax error, got {:?}", other),
        }
        assert!(transpile("RETURN a").is_err());
    }
}
