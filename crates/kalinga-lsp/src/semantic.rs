//! Semantic highlighting: every name the checker resolved, coloured by what it is
//! (parameter or local, read-only or not, property, method, class or struct, ...), which
//! the grammar cannot tell from the text alone.

use std::collections::{HashMap, HashSet};

use kalinga_sema::index::Symbol;
use kalinga_syntax::ast::*;
use kalinga_syntax::error::Span;
use kalinga_syntax::visit::{self, Visitor};
use lsp_types::*;

use crate::snapshot::{DeclKind, Snapshot};
use crate::text;
use crate::world::World;

/// The token types, in the order of their numbers.
const TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::TYPE,
    SemanticTokenType::CLASS,
    SemanticTokenType::STRUCT,
    SemanticTokenType::INTERFACE,
    SemanticTokenType::ENUM,
    SemanticTokenType::ENUM_MEMBER,
    SemanticTokenType::PARAMETER,
    SemanticTokenType::VARIABLE,
    SemanticTokenType::PROPERTY,
    SemanticTokenType::FUNCTION,
    SemanticTokenType::METHOD,
];

/// The modifiers, in the order of their bits.
const MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,
    SemanticTokenModifier::READONLY,
    SemanticTokenModifier::STATIC,
    SemanticTokenModifier::DEPRECATED,
    SemanticTokenModifier::DEFAULT_LIBRARY,
];

#[derive(Clone, Copy)]
enum Kind {
    Type,
    Class,
    Struct,
    Interface,
    Enum,
    EnumMember,
    Parameter,
    Variable,
    Property,
    Function,
    Method,
}

const DECLARATION: u32 = 1;
const READONLY: u32 = 2;
const STATIC: u32 = 4;
const DEPRECATED: u32 = 8;
const LIBRARY: u32 = 16;

/// What the server tells the editor about its semantic tokens.
pub fn legend() -> SemanticTokensLegend {
    SemanticTokensLegend { token_types: TYPES.to_vec(), token_modifiers: MODIFIERS.to_vec() }
}

pub fn full(world: &mut World, params: SemanticTokensParams) -> Option<SemanticTokensResult> {
    let snap = world.good_snapshot(&params.text_document.uri)?;
    Some(SemanticTokensResult::Tokens(SemanticTokens { result_id: None, data: tokens(&snap) }))
}

/// The tokens of the analysed file, delta-encoded as the protocol wants them.
fn tokens(snap: &Snapshot) -> Vec<SemanticToken> {
    let Some(model) = snap.model() else { return vec![] };
    let params = snap.analysis.program.as_ref().map(parameters).unwrap_or_default();
    let kinds: HashMap<&Symbol, DeclKind> = snap.decls.iter().map(|d| (&d.symbol, d.kind)).collect();
    let src = snap.source(0);
    let mut out = vec![];
    let (mut line, mut col) = (0, 0);
    // `names` is sorted by position and has one entry per place.
    for n in snap.names.iter().filter(|n| n.span.file == 0) {
        let (kind, mut mods) = match &n.symbol {
            Symbol::Local { decl, mutable, .. } if params.contains(&(decl.line, decl.col)) => {
                (Kind::Parameter, if *mutable { 0 } else { READONLY })
            }
            Symbol::Local { mutable, .. } => (Kind::Variable, if *mutable { 0 } else { READONLY }),
            Symbol::Function(_) => (Kind::Function, 0),
            Symbol::Type(t) => (type_kind(model, t), 0),
            Symbol::Member { .. } => match kinds.get(&n.symbol) {
                Some(DeclKind::Method) => (Kind::Method, 0),
                Some(DeclKind::Variant) => (Kind::EnumMember, 0),
                Some(DeclKind::Const) => (Kind::Property, READONLY | STATIC),
                Some(DeclKind::StaticVar) => (Kind::Property, STATIC),
                _ => (Kind::Property, 0),
            },
            Symbol::Const(_) => (Kind::Variable, READONLY | STATIC),
            Symbol::Library { name, .. } if name.chars().all(|c| c.is_ascii_uppercase() || c == '_') => {
                (Kind::Variable, READONLY | STATIC | LIBRARY)
            }
            Symbol::Library { .. } => (Kind::Function, LIBRARY),
            Symbol::Builtin { receiver: None, .. } => (Kind::Function, LIBRARY),
            Symbol::Builtin { .. } => (Kind::Method, LIBRARY),
            #[allow(unreachable_patterns)]
            _ => continue,
        };
        if n.is_decl {
            mods |= DECLARATION;
        }
        if model.deprecated.contains_key(&n.symbol) {
            mods |= DEPRECATED;
        }
        let range = text::range(src, n.span);
        if range.end.line != range.start.line || range.end.character <= range.start.character {
            continue;
        }
        let (l, c) = (range.start.line, range.start.character);
        out.push(SemanticToken {
            delta_line: l - line,
            delta_start: if l == line { c - col } else { c },
            length: range.end.character - range.start.character,
            token_type: kind as u32,
            token_modifiers_bitset: mods,
        });
        (line, col) = (l, c);
    }
    out
}

fn type_kind(model: &kalinga_sema::Model, name: &str) -> Kind {
    if model.classes.contains_key(name) {
        Kind::Class
    } else if model.structs.contains_key(name) {
        Kind::Struct
    } else if model.interfaces.contains_key(name) {
        Kind::Interface
    } else if model.enums.contains_key(name) {
        Kind::Enum
    } else {
        Kind::Type
    }
}

/// Where the program's function, method and lambda parameters are declared, as
/// (line, column) in the analysed file.
fn parameters(prog: &Program) -> HashSet<(usize, usize)> {
    let mut out = HashSet::new();
    let mut add = |ps: &[Param]| {
        for p in ps.iter().filter(|p| p.pos.file == 0) {
            out.insert((p.pos.line, p.pos.col));
        }
    };
    for f in &prog.functions {
        add(&f.params);
    }
    for c in &prog.classes {
        if let Some(init) = &c.init {
            add(&init.params);
        }
        for m in &c.methods {
            add(&m.params);
        }
    }
    for s in &prog.structs {
        for m in &s.methods {
            add(&m.params);
        }
        for o in &s.ops {
            add(&o.params);
        }
    }
    for i in &prog.interfaces {
        for m in &i.methods {
            add(&m.params);
        }
    }
    let mut lambdas = Lambdas(vec![]);
    visit::program(prog, &mut lambdas);
    out.extend(lambdas.0.into_iter().filter(|s| s.file == 0).map(|s| (s.line, s.col)));
    out
}

struct Lambdas(Vec<Span>);

impl Visitor for Lambdas {
    fn expr(&mut self, e: &Expr) {
        if let ExprKind::Lambda { params, .. } = &e.kind {
            self.0.extend(params.iter().map(|(_, _, pos)| *pos));
        }
    }
}
