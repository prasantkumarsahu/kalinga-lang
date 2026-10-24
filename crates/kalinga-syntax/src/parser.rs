use crate::ast::*;
use crate::error::{Diagnostic, Result, Span, err};
use crate::token::{StrPart, Tok, Token};

/// Parses a file's tokens; all syntax errors are reported, one per declaration.
pub fn parse(tokens: Vec<Token>) -> std::result::Result<Program, Vec<Diagnostic>> {
    Parser { tokens, i: 0 }.program()
}

/// Parses the expression inside a `{...}` string interpolation.
fn parse_embedded(code: &str, at: Span) -> Result<Expr> {
    let tokens = crate::lexer::lex_at(code, at)?;
    let mut p = Parser { tokens, i: 0 };
    let e = p.expr()?;
    p.skip_newlines();
    if !p.check(&Tok::Eof) {
        return err(format!("unexpected {} in string interpolation", p.peek().describe()), p.pos());
    }
    Ok(e)
}

#[derive(Default, PartialEq)]
struct Mods {
    is_mut: bool,
    is_async: bool,
    is_private: bool,
    is_static: bool,
    is_abstract: bool,
    is_override: bool,
}

struct Parser {
    tokens: Vec<Token>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.tokens[self.i].tok
    }

    fn pos(&self) -> Span {
        self.tokens[self.i].pos
    }

    fn advance(&mut self) -> Token {
        let t = self.tokens[self.i].clone();
        if self.i < self.tokens.len() - 1 {
            self.i += 1;
        }
        t
    }

    fn check(&self, tok: &Tok) -> bool {
        self.peek() == tok
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        if self.check(tok) {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Is the token after the current one `tok`?
    fn next_is(&self, tok: &Tok) -> bool {
        self.tokens.get(self.i + 1).is_some_and(|t| t.tok == *tok)
    }

    fn expect(&mut self, tok: Tok, what: &str) -> Result<Span> {
        if self.check(&tok) {
            Ok(self.advance().pos)
        } else {
            err(format!("expected {}, found {}", what, self.peek().describe()), self.pos())
        }
    }

    fn ident(&mut self, what: &str) -> Result<(String, Span)> {
        match self.peek().clone() {
            Tok::Ident(name) => {
                let pos = self.advance().pos;
                Ok((name, pos))
            }
            other => err(format!("expected {}, found {}", what, other.describe()), self.pos()),
        }
    }

    fn skip_newlines(&mut self) {
        while self.check(&Tok::Newline) || self.check(&Tok::Semi) {
            self.advance();
        }
    }

    /// A statement or member ends at a newline, `;`, `}` or end of file.
    fn end_of_statement(&mut self) -> Result<()> {
        match self.peek() {
            Tok::Newline | Tok::Semi => {
                self.skip_newlines();
                Ok(())
            }
            Tok::RBrace | Tok::Eof => Ok(()),
            other => err(format!("expected end of line, found {}", other.describe()), self.pos()),
        }
    }

    /// The whole file. After an error the parser skips to the next top-level declaration,
    /// so the errors of independent declarations are all reported.
    fn program(&mut self) -> std::result::Result<Program, Vec<Diagnostic>> {
        let mut prog = Program::default();
        let mut errors = vec![];
        self.skip_newlines();
        while !self.check(&Tok::Eof) {
            if let Err(e) = self.item(&mut prog) {
                errors.push(e);
                self.skip_to_next_item();
            }
            self.skip_newlines();
        }
        if errors.is_empty() { Ok(prog) } else { Err(errors) }
    }

    /// Skips past the current token, then to the next token that starts a top-level
    /// declaration at the beginning of a line.
    fn skip_to_next_item(&mut self) {
        self.advance();
        while !self.check(&Tok::Eof) {
            let at_line_start = self.pos().col == 1;
            let starts_item = matches!(
                self.peek(),
                Tok::Fn
                    | Tok::Async
                    | Tok::Class
                    | Tok::Abstract
                    | Tok::Interface
                    | Tok::Enum
                    | Tok::Struct
                    | Tok::Const
                    | Tok::Use
                    | Tok::Extern
                    | Tok::Import
            );
            let starts_test = matches!(self.peek(), Tok::Ident(n) if n == "test") && self.next_is(&Tok::Fn);
            if at_line_start && (starts_item || starts_test) {
                return;
            }
            self.advance();
        }
    }

    /// One top-level declaration.
    fn item(&mut self, prog: &mut Program) -> Result<()> {
        match self.peek() {
            Tok::Fn => prog.functions.push(self.function(Mods::default())?),
            Tok::Async => {
                self.advance();
                if !self.check(&Tok::Fn) {
                    return err("expected `fn` after `async`", self.pos());
                }
                prog.functions.push(self.function(Mods { is_async: true, ..Mods::default() })?);
            }
            Tok::Class => prog.classes.push(self.class(false)?),
            Tok::Abstract => {
                self.advance();
                if !self.check(&Tok::Class) {
                    return err("expected `class` after `abstract`", self.pos());
                }
                prog.classes.push(self.class(true)?);
            }
            Tok::Interface => prog.interfaces.push(self.interface()?),
            Tok::Enum => prog.enums.push(self.enum_decl()?),
            Tok::Struct => prog.structs.push(self.struct_decl()?),
            Tok::Const => {
                let pos = self.advance().pos;
                prog.consts.push(self.static_decl(false, pos)?);
            }
            // `test` is only special before `fn` at the top level.
            Tok::Ident(n) if n == "test" && self.next_is(&Tok::Fn) => {
                self.advance();
                prog.tests.push(self.function(Mods::default())?);
            }
            Tok::Use => self.use_decl(prog)?,
            Tok::Extern => self.extern_decl(prog)?,
            Tok::Import => {
                let pos = self.advance().pos;
                match self.advance().tok {
                    Tok::Str(path) => {
                        let alias = if self.eat(&Tok::As) { Some(self.ident("a name after `as`")?.0) } else { None };
                        prog.imports.push(Import { path, alias, pos });
                    }
                    other => {
                        return err(
                            format!("expected a file name in quotes after `import`, found {}", other.describe()),
                            pos,
                        );
                    }
                }
                self.end_of_statement()?;
            }
            other => {
                return err(
                    format!(
                        "expected `fn`, `test fn`, `class`, `struct`, `interface`, `enum`, `const` or `import` at top level, found {}",
                        other.describe()
                    ),
                    self.pos(),
                );
            }
        }
        Ok(())
    }

    /// `use rust "file.rs"` or `use crate "name" = "version" [features ["a", ...]]`.
    fn use_decl(&mut self, prog: &mut Program) -> Result<()> {
        let pos = self.advance().pos;
        let kind = self.ident("`rust` or `crate` after `use`")?.0;
        let what = match self.advance().tok {
            Tok::Str(s) => s,
            other => return err(format!("expected a name in quotes, found {}", other.describe()), pos),
        };
        match kind.as_str() {
            "rust" => prog.rust_files.push((what, pos)),
            "crate" => {
                self.expect(Tok::Assign, "`=` and a version, e.g. `use crate \"rand\" = \"0.8\"`")?;
                let version = match self.advance().tok {
                    Tok::Str(v) => v,
                    other => return err(format!("expected a version in quotes, found {}", other.describe()), pos),
                };
                let features = self.crate_features(pos)?;
                prog.crates.push(CrateDep { name: what, version, features, pos });
            }
            other => {
                return err(
                    format!(
                        "expected `use rust \"file.rs\"` or `use crate \"name\" = \"version\"`, found `use {}`",
                        other
                    ),
                    pos,
                );
            }
        }
        self.end_of_statement()
    }

    /// `features ["a", "b"]` after a crate version (or nothing).
    fn crate_features(&mut self, pos: Span) -> Result<Vec<String>> {
        let mut features = Vec::new();
        if self.peek() != &Tok::Ident("features".into()) {
            return Ok(features);
        }
        self.advance();
        self.expect(Tok::LBracket, "`[` and a list of feature names")?;
        while !self.check(&Tok::RBracket) {
            match self.advance().tok {
                Tok::Str(f) => features.push(f),
                other => {
                    return err(format!("expected a feature name in quotes, found {}", other.describe()), pos);
                }
            }
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(Tok::RBracket, "`]`")?;
        Ok(features)
    }

    /// `extern fn name(...) -> T [= "rust::path" | = rust { ... }]` or `extern type Name`.
    fn extern_decl(&mut self, prog: &mut Program) -> Result<()> {
        let pos = self.advance().pos;
        if self.check(&Tok::Fn) {
            self.advance();
            let (name, _) = self.ident("function name")?;
            let params = self.params()?;
            let ret = self.ret_type()?;
            let (mut path, mut body) = (None, None);
            if self.eat(&Tok::Assign) {
                match self.advance().tok {
                    Tok::Str(p) => path = Some(p),
                    Tok::RustBlock(b) => body = Some(b),
                    other => {
                        return err(
                            format!("expected a Rust path in quotes or `rust {{ ... }}`, found {}", other.describe()),
                            pos,
                        );
                    }
                }
            }
            prog.extern_fns.push(ExternFn { name, params, ret, path, body, pos });
        } else if self.peek() == &Tok::Ident("type".into()) {
            self.advance();
            let (name, npos) = self.ident("type name")?;
            prog.extern_types.push((name, npos));
        } else {
            return err("expected `extern fn` or `extern type`", pos);
        }
        self.end_of_statement()
    }

    fn params(&mut self) -> Result<Vec<Param>> {
        self.expect(Tok::LParen, "`(`")?;
        let mut params = Vec::new();
        while !self.check(&Tok::RParen) {
            let (name, pos) = self.ident("parameter name")?;
            self.expect(Tok::Colon, "`:` and a type after parameter name")?;
            let ty = self.ty()?;
            params.push(Param { name, ty, pos });
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(Tok::RParen, "`)`")?;
        Ok(params)
    }

    fn ret_type(&mut self) -> Result<TypeExpr> {
        if self.eat(&Tok::Arrow) { self.ty() } else { Ok(TypeExpr::Void) }
    }

    fn function(&mut self, mods: Mods) -> Result<FnDecl> {
        let pos = self.expect(Tok::Fn, "`fn`")?;
        let (name, _) = self.ident("function name")?;
        let (tparams, bounds) = self.tparams()?;
        let params = self.params()?;
        let ret = self.ret_type()?;
        let body = if mods.is_abstract {
            if self.check(&Tok::LBrace) {
                return err("an `abstract fn` has no body", self.pos());
            }
            self.end_of_statement()?;
            vec![]
        } else {
            self.block()?
        };
        Ok(FnDecl {
            name,
            tparams,
            bounds,
            params,
            ret,
            body,
            is_override: mods.is_override,
            is_private: mods.is_private,
            is_static: mods.is_static,
            is_abstract: mods.is_abstract,
            is_mut: mods.is_mut,
            is_async: mods.is_async,
            pos,
        })
    }

    /// `<T, U: Show + Eq>` after a function, class or struct name.
    fn tparams(&mut self) -> Result<(Vec<String>, Vec<BoundDecl>)> {
        let mut names = Vec::new();
        let mut bounds = Vec::new();
        if self.eat(&Tok::Lt) {
            loop {
                let param = self.ident("a type parameter name")?.0;
                if self.eat(&Tok::Colon) {
                    let mut b = BoundDecl { param: param.clone(), names: vec![] };
                    loop {
                        b.names.push(self.ident("an interface, `Show`, `Eq` or `Ord`")?);
                        if !self.eat(&Tok::Plus) {
                            break;
                        }
                    }
                    bounds.push(b);
                }
                names.push(param);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.expect(Tok::Gt, "`>` to close the type parameters")?;
        }
        Ok((names, bounds))
    }

    /// `<int, str>` type arguments.
    fn targs(&mut self) -> Result<Vec<TypeExpr>> {
        self.expect(Tok::Lt, "`<`")?;
        let mut out = Vec::new();
        loop {
            out.push(self.ty()?);
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(Tok::Gt, "`>` to close the type arguments")?;
        Ok(out)
    }

    /// `private`, `static`, `abstract`, `override` in any order before a class member.
    fn modifiers(&mut self) -> Result<Mods> {
        let mut m = Mods::default();
        loop {
            let pos = self.pos();
            let flag = match self.peek() {
                Tok::Private => &mut m.is_private,
                Tok::Static => &mut m.is_static,
                Tok::Abstract => &mut m.is_abstract,
                Tok::Override => &mut m.is_override,
                Tok::Async => &mut m.is_async,
                _ => return Ok(m),
            };
            if *flag {
                return err(format!("{} written twice", self.peek().describe()), pos);
            }
            *flag = true;
            self.advance();
        }
    }

    fn class(&mut self, is_abstract: bool) -> Result<ClassDecl> {
        let pos = self.expect(Tok::Class, "`class`")?;
        let (name, _) = self.ident("class name")?;
        let (tparams, bounds) = self.tparams()?;
        let mut bases = Vec::new();
        if self.eat(&Tok::Colon) {
            loop {
                bases.push(self.ident("base class or interface name")?);
                if self.check(&Tok::Lt) {
                    return err("generic classes cannot be extended (yet)", self.pos());
                }
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(Tok::LBrace, "`{` to start class body")?;
        let mut class = ClassDecl {
            name,
            tparams,
            bounds,
            bases,
            fields: vec![],
            init: None,
            methods: vec![],
            statics: vec![],
            is_abstract,
            pos,
        };
        self.skip_newlines();
        while !self.check(&Tok::RBrace) {
            let mpos = self.pos();
            let mods = self.modifiers()?;
            if let Some(st) = self.static_member(&mods, mpos)? {
                class.statics.push(st);
                continue;
            }
            match self.peek().clone() {
                Tok::Ident(fname) => {
                    if mods.is_static || mods.is_abstract || mods.is_override {
                        return err("fields can only be marked `private`", mpos);
                    }
                    let fpos = self.advance().pos;
                    self.expect(Tok::Colon, "`:` and a type after field name")?;
                    let ty = self.ty()?;
                    let default = if self.eat(&Tok::Assign) { Some(self.expr()?) } else { None };
                    class.fields.push(FieldDecl { name: fname, ty, default, is_private: mods.is_private, pos: fpos });
                    self.end_of_statement()?;
                }
                Tok::Init => {
                    if mods != Mods::default() {
                        return err("`init` cannot have modifiers", mpos);
                    }
                    let ipos = self.advance().pos;
                    if class.init.is_some() {
                        return err("a class can only have one `init`", ipos);
                    }
                    let params = self.params()?;
                    let body = self.block()?;
                    class.init = Some(InitDecl { params, body, pos: ipos });
                    self.skip_newlines();
                }
                Tok::Fn => {
                    class.methods.push(self.function(mods)?);
                    self.skip_newlines();
                }
                other => {
                    return err(
                        format!("expected field, `init`, `fn` or `const` in class body, found {}", other.describe()),
                        self.pos(),
                    );
                }
            }
        }
        self.expect(Tok::RBrace, "`}`")?;
        Ok(class)
    }

    /// `const NAME: Type = value` or `static var name: Type = value` in a class or struct
    /// body, after the modifiers; `None` when the member is something else.
    fn static_member(&mut self, mods: &Mods, pos: Span) -> Result<Option<StaticDecl>> {
        let mutable = match self.peek() {
            Tok::Const if *mods == Mods::default() => false,
            Tok::Var if *mods == Mods { is_static: true, ..Mods::default() } => true,
            Tok::Const => return err("`const` cannot have modifiers", pos),
            Tok::Var => return err("write `static var name: Type = value` for a variable shared by the type", pos),
            Tok::Ident(_)
                if mods.is_static
                    && !mods.is_async
                    && self.tokens.get(self.i + 1).is_some_and(|t| t.tok == Tok::Colon) =>
            {
                return Err(Diagnostic::error("constants are written `const NAME: Type = value`".to_string(), pos)
                    .with_help("`kalinga fix` rewrites `static NAME: Type = value` to `const`"));
            }
            _ => return Ok(None),
        };
        let at = self.advance().pos;
        Ok(Some(self.static_decl(mutable, at)?))
    }

    /// The rest of `const NAME: Type = value` / `static var name: Type = value`, after
    /// `const` / `var`.
    fn static_decl(&mut self, mutable: bool, pos: Span) -> Result<StaticDecl> {
        let (kw, what) = if mutable { ("static var", "variable") } else { ("const", "constant") };
        let (name, _) = self.ident(&format!("a {} name after `{}`", what, kw))?;
        if !self.eat(&Tok::Colon) {
            return err(format!("a {} needs a type: `{} {}: Type = value`", what, kw, name), self.pos());
        }
        let ty = self.ty()?;
        if !self.eat(&Tok::Assign) {
            return err(format!("a {} needs a value: `{} {}: Type = value`", what, kw, name), self.pos());
        }
        let value = self.expr()?;
        self.end_of_statement()?;
        Ok(StaticDecl { name, ty, value, mutable, pos })
    }

    fn interface(&mut self) -> Result<InterfaceDecl> {
        let pos = self.expect(Tok::Interface, "`interface`")?;
        let (name, _) = self.ident("interface name")?;
        let mut bases = Vec::new();
        if self.eat(&Tok::Colon) {
            loop {
                bases.push(self.ident("interface name")?);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(Tok::LBrace, "`{` to start interface body")?;
        self.skip_newlines();
        let mut methods = Vec::new();
        while !self.check(&Tok::RBrace) {
            let mpos = self.expect(Tok::Fn, "`fn` (interfaces only contain method signatures)")?;
            let (mname, _) = self.ident("method name")?;
            let params = self.params()?;
            let ret = self.ret_type()?;
            methods.push(MethodSig { name: mname, params, ret, pos: mpos });
            self.end_of_statement()?;
        }
        self.expect(Tok::RBrace, "`}`")?;
        Ok(InterfaceDecl { name, bases, methods, pos })
    }

    fn struct_decl(&mut self) -> Result<StructDecl> {
        let pos = self.expect(Tok::Struct, "`struct`")?;
        let (name, _) = self.ident("struct name")?;
        let (tparams, bounds) = self.tparams()?;
        if self.check(&Tok::Colon) {
            return err("structs cannot extend classes or implement interfaces (use a class for that)", self.pos());
        }
        self.expect(Tok::LBrace, "`{` to start struct body")?;
        let mut s =
            StructDecl { name, tparams, bounds, fields: vec![], methods: vec![], ops: vec![], statics: vec![], pos };
        self.skip_newlines();
        while !self.check(&Tok::RBrace) {
            let mpos = self.pos();
            // `op +(other: T) -> T { ... }`
            if self.peek() == &Tok::Ident("op".into()) {
                let next = self.tokens.get(self.i + 1).map(|t| t.tok.clone());
                let op = match next {
                    Some(Tok::Plus) => Some(BinOp::Add),
                    Some(Tok::Minus) => Some(BinOp::Sub),
                    Some(Tok::Star) => Some(BinOp::Mul),
                    Some(Tok::Slash) => Some(BinOp::Div),
                    Some(Tok::Percent) => Some(BinOp::Rem),
                    _ => None,
                };
                if let Some(op) = op {
                    self.advance();
                    self.advance();
                    let params = self.params()?;
                    let ret = self.ret_type()?;
                    let body = self.block()?;
                    s.ops.push(OpDecl { op, params, ret, body, pos: mpos });
                    self.skip_newlines();
                    continue;
                }
            }
            // `mut fn` (a method that changes `self`)
            let mut is_mut = false;
            if self.peek() == &Tok::Ident("mut".into()) && self.tokens.get(self.i + 1).is_some_and(|t| t.tok == Tok::Fn)
            {
                self.advance();
                is_mut = true;
            }
            let mut mods = self.modifiers()?;
            mods.is_mut = is_mut;
            if !is_mut && let Some(st) = self.static_member(&mods, mpos)? {
                s.statics.push(st);
                continue;
            }
            match self.peek().clone() {
                Tok::Ident(fname) if !is_mut => {
                    if mods.is_static || mods.is_abstract || mods.is_override || mods.is_async {
                        return err("fields can only be marked `private`", mpos);
                    }
                    let fpos = self.advance().pos;
                    self.expect(Tok::Colon, "`:` and a type after field name")?;
                    let ty = self.ty()?;
                    let default = if self.eat(&Tok::Assign) { Some(self.expr()?) } else { None };
                    s.fields.push(FieldDecl { name: fname, ty, default, is_private: mods.is_private, pos: fpos });
                    self.end_of_statement()?;
                }
                Tok::Fn => {
                    if mods.is_abstract || mods.is_override {
                        return err(
                            "struct methods cannot be `abstract` or `override` (structs have no inheritance)",
                            mpos,
                        );
                    }
                    if mods.is_mut && mods.is_static {
                        return err("a `static fn` has no `self`, so it cannot be `mut`", mpos);
                    }
                    s.methods.push(self.function(mods)?);
                    self.skip_newlines();
                }
                Tok::Init => {
                    return err(
                        "structs have no `init`: `Name(a, b)` sets the fields in order; use a `static fn` for other constructors",
                        mpos,
                    );
                }
                other => {
                    return err(
                        format!("expected field, `fn`, `mut fn` or `op` in struct body, found {}", other.describe()),
                        self.pos(),
                    );
                }
            }
        }
        self.expect(Tok::RBrace, "`}`")?;
        Ok(s)
    }

    fn enum_decl(&mut self) -> Result<EnumDecl> {
        let pos = self.expect(Tok::Enum, "`enum`")?;
        let (name, _) = self.ident("enum name")?;
        self.expect(Tok::LBrace, "`{` to start enum body")?;
        self.skip_newlines();
        let mut variants = Vec::new();
        while !self.check(&Tok::RBrace) {
            let (vname, vpos) = self.ident("variant name")?;
            let fields = if self.check(&Tok::LParen) { self.params()? } else { vec![] };
            variants.push(VariantDecl { name: vname, fields, pos: vpos });
            if self.eat(&Tok::Comma) {
                self.skip_newlines();
            } else {
                self.end_of_statement()?;
            }
        }
        self.expect(Tok::RBrace, "`}`")?;
        Ok(EnumDecl { name, variants, pos })
    }

    fn pattern(&mut self) -> Result<Pattern> {
        let pos = self.pos();
        Ok(match self.advance().tok {
            Tok::Ident(n) if n == "_" => Pattern::Wildcard,
            Tok::Ident(n) => {
                // Optional `Enum.` prefix.
                let name = if self.eat(&Tok::Dot) { self.ident("variant name")?.0 } else { n };
                let mut binds = Vec::new();
                if self.eat(&Tok::LParen) {
                    while !self.check(&Tok::RParen) {
                        binds.push(self.ident("a name to bind")?.0);
                        if !self.eat(&Tok::Comma) {
                            break;
                        }
                    }
                    self.expect(Tok::RParen, "`)`")?;
                }
                Pattern::Variant(name, binds)
            }
            Tok::Int(v) => Pattern::Int(v),
            Tok::Minus => match self.advance().tok {
                Tok::Int(v) => Pattern::Int(-v),
                other => return err(format!("expected a number after `-`, found {}", other.describe()), pos),
            },
            Tok::Str(s) => Pattern::Str(s),
            Tok::True => Pattern::Bool(true),
            Tok::False => Pattern::Bool(false),
            other => {
                return err(format!("expected a pattern (a variant, literal or `_`), found {}", other.describe()), pos);
            }
        })
    }

    fn match_stmt(&mut self) -> Result<Stmt> {
        let pos = self.expect(Tok::Match, "`match`")?;
        let value = self.expr()?;
        self.expect(Tok::LBrace, "`{` to start the match arms")?;
        self.skip_newlines();
        let mut arms = Vec::new();
        while !self.check(&Tok::RBrace) {
            let apos = self.pos();
            let pattern = self.pattern()?;
            self.expect(Tok::FatArrow, "`=>` after the pattern")?;
            let body = if self.check(&Tok::LBrace) { self.block()? } else { vec![self.statement_ending(true)?] };
            self.eat(&Tok::Comma);
            self.skip_newlines();
            arms.push(MatchArm { pattern, body, pos: apos });
        }
        self.expect(Tok::RBrace, "`}`")?;
        self.skip_newlines();
        Ok(Stmt { kind: StmtKind::Match { value, arms }, pos })
    }

    fn ty(&mut self) -> Result<TypeExpr> {
        let pos = self.pos();
        let base = match self.advance().tok {
            Tok::Ident(n) => match n.as_str() {
                "int" => TypeExpr::Int,
                "float" => TypeExpr::Float,
                "bool" => TypeExpr::Bool,
                "str" => TypeExpr::Str,
                "void" => TypeExpr::Void,
                _ => {
                    // `alias.Name` for a type from `import "pkg" as alias`.
                    let n = if self.eat(&Tok::Dot) {
                        format!("{}.{}", n, self.ident("a type name after `.`")?.0)
                    } else {
                        n
                    };
                    let args = if self.check(&Tok::Lt) { self.targs()? } else { vec![] };
                    TypeExpr::Named(n, args, pos)
                }
            },
            Tok::LBracket => {
                let inner = self.ty()?;
                self.expect(Tok::RBracket, "`]`")?;
                TypeExpr::Array(Box::new(inner))
            }
            Tok::Fn => {
                self.expect(Tok::LParen, "`(` after `fn` in a function type")?;
                let mut params = Vec::new();
                while !self.check(&Tok::RParen) {
                    params.push(self.ty()?);
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.expect(Tok::RParen, "`)`")?;
                let ret = self.ret_type()?;
                // `fn(int) -> int?` means "returns an optional"; the `?` below would apply to the
                // whole function type, which is rarely wanted, so parentheses are not supported.
                return Ok(TypeExpr::Func(params, Box::new(ret)));
            }
            Tok::LBrace => {
                let key = self.ty()?;
                self.expect(Tok::Colon, "`:` between the key and value types of a map")?;
                let value = self.ty()?;
                self.skip_newlines();
                self.expect(Tok::RBrace, "`}`")?;
                TypeExpr::Map(Box::new(key), Box::new(value))
            }
            other => return err(format!("expected a type, found {}", other.describe()), pos),
        };
        if self.eat(&Tok::Question) {
            if matches!(base, TypeExpr::Void) {
                return err("`void?` is not a valid type", pos);
            }
            return Ok(TypeExpr::Optional(Box::new(base)));
        }
        Ok(base)
    }

    fn block(&mut self) -> Result<Vec<Stmt>> {
        self.expect(Tok::LBrace, "`{`")?;
        self.skip_newlines();
        let mut stmts = Vec::new();
        while !self.check(&Tok::RBrace) {
            if self.check(&Tok::Eof) {
                return err("unexpected end of file, missing `}`", self.pos());
            }
            stmts.push(self.statement()?);
        }
        self.expect(Tok::RBrace, "`}`")?;
        Ok(stmts)
    }

    fn statement(&mut self) -> Result<Stmt> {
        self.statement_ending(false)
    }

    /// A statement; with `comma_ok` (a one-line match arm) it may also end at a `,`.
    fn statement_ending(&mut self, comma_ok: bool) -> Result<Stmt> {
        let pos = self.pos();
        let kind = match self.peek() {
            Tok::Let | Tok::Var => {
                let mutable = self.advance().tok == Tok::Var;
                let (name, _) = self.ident("variable name")?;
                let ty = if self.eat(&Tok::Colon) { Some(self.ty()?) } else { None };
                self.expect(Tok::Assign, "`=` (variables must be initialized)")?;
                let value = self.expr()?;
                StmtKind::Let { name, mutable, ty, value }
            }
            Tok::If => return self.if_stmt(),
            Tok::Match => return self.match_stmt(),
            Tok::While => {
                self.advance();
                if self.eat(&Tok::Let) {
                    let (name, _) = self.ident("variable name")?;
                    self.expect(Tok::Assign, "`=`")?;
                    let value = self.expr()?;
                    let body = self.block()?;
                    self.skip_newlines();
                    return Ok(Stmt { kind: StmtKind::WhileLet { name, value, body }, pos });
                }
                let cond = self.expr()?;
                let body = self.block()?;
                self.skip_newlines();
                return Ok(Stmt { kind: StmtKind::While { cond, body }, pos });
            }
            Tok::For => {
                self.advance();
                let (first_name, _) = self.ident("loop variable name")?;
                let (index, var) = if self.eat(&Tok::Comma) {
                    (Some(first_name), self.ident("a second loop variable name")?.0)
                } else {
                    (None, first_name)
                };
                self.expect(Tok::In, "`in`")?;
                let first = self.expr()?;
                let iter = match self.peek() {
                    Tok::DotDot | Tok::DotDotEq => {
                        let inclusive = self.advance().tok == Tok::DotDotEq;
                        ForIter::Range(first, self.expr()?, inclusive)
                    }
                    _ => ForIter::Each(first),
                };
                let body = self.block()?;
                self.skip_newlines();
                return Ok(Stmt { kind: StmtKind::For { index, var, iter, body }, pos });
            }
            Tok::Return => {
                self.advance();
                let value = match self.peek() {
                    Tok::Newline | Tok::Semi | Tok::RBrace => None,
                    _ => Some(self.expr()?),
                };
                StmtKind::Return(value)
            }
            Tok::Break => {
                self.advance();
                StmtKind::Break
            }
            Tok::Continue => {
                self.advance();
                StmtKind::Continue
            }
            _ => {
                let target = self.expr()?;
                let op = match self.peek() {
                    Tok::Assign => Some(AssignOp::Set),
                    Tok::PlusAssign => Some(AssignOp::Add),
                    Tok::MinusAssign => Some(AssignOp::Sub),
                    Tok::StarAssign => Some(AssignOp::Mul),
                    Tok::SlashAssign => Some(AssignOp::Div),
                    Tok::PercentAssign => Some(AssignOp::Rem),
                    _ => None,
                };
                match op {
                    Some(op) => {
                        self.advance();
                        let value = self.expr()?;
                        StmtKind::Assign { target, op, value }
                    }
                    None => StmtKind::Expr(target),
                }
            }
        };
        if !(comma_ok && self.check(&Tok::Comma)) {
            self.end_of_statement()?;
        }
        Ok(Stmt { kind, pos })
    }

    fn if_stmt(&mut self) -> Result<Stmt> {
        let pos = self.expect(Tok::If, "`if`")?;
        let kind = if self.eat(&Tok::Let) {
            let (name, _) = self.ident("variable name")?;
            self.expect(Tok::Assign, "`=`")?;
            let value = self.expr()?;
            let then = self.block()?;
            let otherwise = self.else_part()?;
            StmtKind::IfLet { name, value, then, otherwise }
        } else {
            let cond = self.expr()?;
            let then = self.block()?;
            let otherwise = self.else_part()?;
            StmtKind::If { cond, then, otherwise }
        };
        self.skip_newlines();
        Ok(Stmt { kind, pos })
    }

    fn else_part(&mut self) -> Result<Option<Vec<Stmt>>> {
        // Allow `}` and `else` on different lines.
        let save = self.i;
        self.skip_newlines();
        if !self.eat(&Tok::Else) {
            self.i = save;
            return Ok(None);
        }
        if self.check(&Tok::If) { Ok(Some(vec![self.if_stmt()?])) } else { Ok(Some(self.block()?)) }
    }

    /// `if` used as a value, after `if`: both branches are value blocks, and `else` is
    /// required.
    fn if_expr(&mut self, pos: Span) -> Result<ExprKind> {
        if self.check(&Tok::Let) {
            return err("`if let` cannot be used as a value (yet); use a `match`, or a `var` set in each branch", pos);
        }
        let cond = self.expr()?;
        let then = self.value_block("if")?;
        self.skip_newlines();
        if !self.eat(&Tok::Else) {
            return err("an `if` used as a value needs an `else` branch", pos);
        }
        let otherwise = if self.check(&Tok::If) {
            let at = self.advance().pos;
            let inner = Expr { kind: self.if_expr(at)?, pos: at };
            vec![Stmt { kind: StmtKind::Expr(inner), pos: at }]
        } else {
            self.value_block("if")?
        };
        Ok(ExprKind::If { cond: Box::new(cond), then, otherwise })
    }

    /// `match` used as a value, after `match`: each arm is `Pattern => value` or
    /// `Pattern => { ... value }`, separated by newlines or commas.
    fn match_expr(&mut self) -> Result<ExprKind> {
        let value = self.expr()?;
        self.expect(Tok::LBrace, "`{` to start the match arms")?;
        self.skip_newlines();
        let mut arms = Vec::new();
        while !self.check(&Tok::RBrace) {
            let apos = self.pos();
            let pattern = self.pattern()?;
            self.expect(Tok::FatArrow, "`=>` after the pattern")?;
            let body = if self.check(&Tok::LBrace) {
                self.value_block("match")?
            } else {
                let e = self.expr()?;
                vec![Stmt { pos: e.pos, kind: StmtKind::Expr(e) }]
            };
            self.eat(&Tok::Comma);
            self.skip_newlines();
            arms.push(MatchArm { pattern, body, pos: apos });
        }
        self.expect(Tok::RBrace, "`}`")?;
        Ok(ExprKind::Match { value: Box::new(value), arms })
    }

    /// `{ statements... value }`: a block whose last line is the value it gives. A trailing
    /// `if` or `match` is read as a value too.
    fn value_block(&mut self, what: &str) -> Result<Vec<Stmt>> {
        self.expect(Tok::LBrace, "`{`")?;
        self.skip_newlines();
        let mut stmts = Vec::new();
        while !self.check(&Tok::RBrace) {
            if self.check(&Tok::Eof) {
                return err("unexpected end of file, missing `}`", self.pos());
            }
            let start = self.i;
            let s = self.statement()?;
            if self.check(&Tok::RBrace) && matches!(s.kind, StmtKind::If { .. } | StmtKind::Match { .. }) {
                self.i = start;
                let e = self.expr()?;
                self.skip_newlines();
                stmts.push(Stmt { pos: e.pos, kind: StmtKind::Expr(e) });
            } else {
                stmts.push(s);
            }
        }
        let end = self.expect(Tok::RBrace, "`}`")?;
        if block_value(&stmts).is_none() {
            return err(format!("this branch of an `{}` used as a value must end with its value", what), end);
        }
        Ok(stmts)
    }

    // ---- expressions, lowest precedence first ----

    pub fn expr(&mut self) -> Result<Expr> {
        self.or()
    }

    fn binary_level(&mut self, next: fn(&mut Self) -> Result<Expr>, ops: &[(Tok, BinOp)]) -> Result<Expr> {
        let mut left = next(self)?;
        'outer: loop {
            for (tok, op) in ops {
                if self.check(tok) {
                    let pos = self.advance().pos;
                    let right = next(self)?;
                    left = Expr { kind: ExprKind::Binary(*op, Box::new(left), Box::new(right)), pos };
                    continue 'outer;
                }
            }
            return Ok(left);
        }
    }

    fn or(&mut self) -> Result<Expr> {
        self.binary_level(Self::and, &[(Tok::Or, BinOp::Or)])
    }

    fn and(&mut self) -> Result<Expr> {
        self.binary_level(Self::equality, &[(Tok::And, BinOp::And)])
    }

    fn equality(&mut self) -> Result<Expr> {
        self.binary_level(Self::comparison, &[(Tok::Eq, BinOp::Eq), (Tok::Ne, BinOp::Ne)])
    }

    fn comparison(&mut self) -> Result<Expr> {
        self.binary_level(
            Self::additive,
            &[(Tok::Lt, BinOp::Lt), (Tok::Le, BinOp::Le), (Tok::Gt, BinOp::Gt), (Tok::Ge, BinOp::Ge)],
        )
    }

    fn additive(&mut self) -> Result<Expr> {
        self.binary_level(Self::multiplicative, &[(Tok::Plus, BinOp::Add), (Tok::Minus, BinOp::Sub)])
    }

    fn multiplicative(&mut self) -> Result<Expr> {
        self.binary_level(Self::unary, &[(Tok::Star, BinOp::Mul), (Tok::Slash, BinOp::Div), (Tok::Percent, BinOp::Rem)])
    }

    fn unary(&mut self) -> Result<Expr> {
        let pos = self.pos();
        if self.eat(&Tok::Await) {
            let e = self.unary()?;
            return Ok(Expr { kind: ExprKind::Await(Box::new(e)), pos });
        }
        if self.eat(&Tok::Minus) {
            let e = self.unary()?;
            return Ok(Expr { kind: ExprKind::Unary(UnOp::Neg, Box::new(e)), pos });
        }
        if self.eat(&Tok::Not) {
            let e = self.unary()?;
            return Ok(Expr { kind: ExprKind::Unary(UnOp::Not, Box::new(e)), pos });
        }
        let mut e = self.postfix()?;
        while self.check(&Tok::As) || self.check(&Tok::Is) {
            let pos = self.pos();
            let test_only = self.advance().tok == Tok::Is;
            let (class, _) = self.ident("a class name")?;
            e = Expr { kind: ExprKind::Downcast { value: Box::new(e), class, test_only }, pos };
        }
        Ok(e)
    }

    fn args(&mut self) -> Result<Vec<Expr>> {
        self.expect(Tok::LParen, "`(`")?;
        let mut args = Vec::new();
        while !self.check(&Tok::RParen) {
            // `name: value`
            if let Tok::Ident(name) = self.peek().clone()
                && self.tokens.get(self.i + 1).is_some_and(|t| t.tok == Tok::Colon)
            {
                let pos = self.advance().pos;
                self.advance();
                let value = self.expr()?;
                args.push(Expr { kind: ExprKind::Named(name, Box::new(value)), pos });
            } else {
                args.push(self.expr()?);
            }
            if !self.eat(&Tok::Comma) {
                break;
            }
        }
        self.expect(Tok::RParen, "`)`")?;
        Ok(args)
    }

    fn postfix(&mut self) -> Result<Expr> {
        let mut e = self.primary()?;
        loop {
            let pos = self.pos();
            if self.eat(&Tok::Dot) {
                let (name, _) = self.ident("field or method name after `.`")?;
                if self.check(&Tok::LParen) {
                    let args = self.args()?;
                    e = Expr { kind: ExprKind::MethodCall(Box::new(e), name, args), pos };
                } else {
                    e = Expr { kind: ExprKind::Field(Box::new(e), name), pos };
                }
            } else if self.check(&Tok::Question) {
                self.advance();
                e = Expr { kind: ExprKind::Try(Box::new(e)), pos };
            } else if self.eat(&Tok::LBracket) {
                let from = if self.check(&Tok::DotDot) { None } else { Some(Box::new(self.expr()?)) };
                if self.eat(&Tok::DotDotEq) {
                    // `xs[a..=b]` is `xs[a..b + 1]`.
                    let last = self.expr()?;
                    let one = Expr { kind: ExprKind::Int(1), pos: last.pos };
                    let to = Expr { pos: last.pos, kind: ExprKind::Binary(BinOp::Add, Box::new(last), Box::new(one)) };
                    self.expect(Tok::RBracket, "`]`")?;
                    e = Expr { kind: ExprKind::Slice(Box::new(e), from, Some(Box::new(to))), pos };
                } else if self.eat(&Tok::DotDot) {
                    let to = if self.check(&Tok::RBracket) { None } else { Some(Box::new(self.expr()?)) };
                    self.expect(Tok::RBracket, "`]`")?;
                    e = Expr { kind: ExprKind::Slice(Box::new(e), from, to), pos };
                } else {
                    self.expect(Tok::RBracket, "`]` (or `..` for a slice)")?;
                    let Some(idx) = from else { return err("expected an index", pos) };
                    e = Expr { kind: ExprKind::Index(Box::new(e), idx), pos };
                }
            } else {
                return Ok(e);
            }
        }
    }

    fn primary(&mut self) -> Result<Expr> {
        let pos = self.pos();
        let tok = self.advance().tok;
        let kind = match tok {
            Tok::Int(v) => ExprKind::Int(v),
            Tok::Float(v) => ExprKind::Float(v),
            Tok::Str(s) => ExprKind::Str(s),
            Tok::If => self.if_expr(pos)?,
            Tok::Match => self.match_expr()?,
            Tok::Fn => {
                self.expect(Tok::LParen, "`(` to start the lambda's parameters")?;
                let mut params = Vec::new();
                while !self.check(&Tok::RParen) {
                    let (name, ppos) = self.ident("parameter name")?;
                    let ty = if self.eat(&Tok::Colon) { Some(self.ty()?) } else { None };
                    params.push((name, ty, ppos));
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.expect(Tok::RParen, "`)`")?;
                let ret = if self.eat(&Tok::Arrow) { Some(self.ty()?) } else { None };
                let body = if self.eat(&Tok::FatArrow) {
                    LambdaBody::Expr(Box::new(self.expr()?))
                } else {
                    LambdaBody::Block(self.block()?)
                };
                ExprKind::Lambda { params, ret, body }
            }
            Tok::InterpStr(parts) => {
                let mut out = Vec::new();
                for p in parts {
                    match p {
                        StrPart::Lit(s) => out.push(InterpPart::Lit(s)),
                        StrPart::Code(code, at) => out.push(InterpPart::Expr(parse_embedded(&code, at)?)),
                    }
                }
                ExprKind::Interp(out)
            }
            Tok::LBrace => {
                let mut entries = Vec::new();
                self.skip_newlines();
                while !self.check(&Tok::RBrace) {
                    let k = self.expr()?;
                    self.expect(Tok::Colon, "`:` between a map key and its value")?;
                    let v = self.expr()?;
                    entries.push((k, v));
                    self.skip_newlines();
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                    self.skip_newlines();
                }
                self.skip_newlines();
                self.expect(Tok::RBrace, "`}` to close the map")?;
                ExprKind::Map(entries)
            }
            Tok::True => ExprKind::Bool(true),
            Tok::False => ExprKind::Bool(false),
            Tok::None => ExprKind::None,
            Tok::SelfKw => ExprKind::SelfRef,
            Tok::Ident(name) => {
                // `name<T>(...)`: try type arguments, fall back to a comparison.
                let mut targs = vec![];
                if self.check(&Tok::Lt) {
                    let save = self.i;
                    match self.targs() {
                        Ok(t) if self.check(&Tok::LParen) => targs = t,
                        _ => self.i = save,
                    }
                }
                if self.check(&Tok::LParen) { ExprKind::Call(name, targs, self.args()?) } else { ExprKind::Ident(name) }
            }
            Tok::Super => {
                if self.check(&Tok::LParen) {
                    ExprKind::SuperInit(self.args()?)
                } else {
                    self.expect(Tok::Dot, "`(` or `.` after `super`")?;
                    let (name, _) = self.ident("method name")?;
                    ExprKind::SuperCall(name, self.args()?)
                }
            }
            Tok::LParen => {
                let e = self.expr()?;
                self.expect(Tok::RParen, "`)`")?;
                return Ok(e);
            }
            Tok::LBracket => {
                let mut items = Vec::new();
                while !self.check(&Tok::RBracket) {
                    items.push(self.expr()?);
                    if !self.eat(&Tok::Comma) {
                        break;
                    }
                }
                self.expect(Tok::RBracket, "`]`")?;
                ExprKind::Array(items)
            }
            other => return err(format!("expected an expression, found {}", other.describe()), pos),
        };
        Ok(Expr { kind, pos })
    }
}
