use crate::ast::*;
use crate::error::{err, Pos, Result};
use crate::token::{StrPart, Tok, Token};

pub fn parse(tokens: Vec<Token>) -> Result<Program> {
    Parser { tokens, i: 0 }.program()
}

/// Parses the expression inside a `{...}` string interpolation.
fn parse_embedded(code: &str, at: Pos) -> Result<Expr> {
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

    fn pos(&self) -> Pos {
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

    fn expect(&mut self, tok: Tok, what: &str) -> Result<Pos> {
        if self.check(&tok) {
            Ok(self.advance().pos)
        } else {
            err(format!("expected {}, found {}", what, self.peek().describe()), self.pos())
        }
    }

    fn ident(&mut self, what: &str) -> Result<(String, Pos)> {
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

    fn program(&mut self) -> Result<Program> {
        let mut prog = Program::default();
        self.skip_newlines();
        while !self.check(&Tok::Eof) {
            match self.peek() {
                Tok::Fn => prog.functions.push(self.function(Mods::default())?),
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
                Tok::Use => {
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
                            prog.crates.push((what, version, pos));
                        }
                        other => return err(format!("expected `use rust \"file.rs\"` or `use crate \"name\" = \"version\"`, found `use {}`", other), pos),
                    }
                    self.end_of_statement()?;
                }
                Tok::Extern => {
                    let pos = self.advance().pos;
                    if self.check(&Tok::Fn) {
                        self.advance();
                        let (name, _) = self.ident("function name")?;
                        let params = self.params()?;
                        let ret = self.ret_type()?;
                        let path = if self.eat(&Tok::Assign) {
                            match self.advance().tok {
                                Tok::Str(p) => Some(p),
                                other => return err(format!("expected a Rust path in quotes, found {}", other.describe()), pos),
                            }
                        } else {
                            None
                        };
                        prog.extern_fns.push(ExternFn { name, params, ret, path, pos });
                    } else if self.peek() == &Tok::Ident("type".into()) {
                        self.advance();
                        let (name, npos) = self.ident("type name")?;
                        prog.extern_types.push((name, npos));
                    } else {
                        return err("expected `extern fn` or `extern type`", pos);
                    }
                    self.end_of_statement()?;
                }
                Tok::Import => {
                    let pos = self.advance().pos;
                    match self.advance().tok {
                        Tok::Str(path) => prog.imports.push((path, pos)),
                        other => return err(format!("expected a file name in quotes after `import`, found {}", other.describe()), pos),
                    }
                    self.end_of_statement()?;
                }
                other => {
                    return err(
                        format!("expected `fn`, `class`, `struct`, `interface`, `enum` or `import` at top level, found {}", other.describe()),
                        self.pos(),
                    )
                }
            }
            self.skip_newlines();
        }
        Ok(prog)
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
        let tparams = self.tparams()?;
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
            params,
            ret,
            body,
            is_override: mods.is_override,
            is_private: mods.is_private,
            is_static: mods.is_static,
            is_abstract: mods.is_abstract,
            is_mut: mods.is_mut,
            pos,
        })
    }

    /// `<T, U>` after a function or class name.
    fn tparams(&mut self) -> Result<Vec<String>> {
        let mut out = Vec::new();
        if self.eat(&Tok::Lt) {
            loop {
                out.push(self.ident("a type parameter name")?.0);
                if !self.eat(&Tok::Comma) {
                    break;
                }
            }
            self.expect(Tok::Gt, "`>` to close the type parameters")?;
        }
        Ok(out)
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
        let tparams = self.tparams()?;
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
        let mut class = ClassDecl { name, tparams, bases, fields: vec![], init: None, methods: vec![], is_abstract, pos };
        self.skip_newlines();
        while !self.check(&Tok::RBrace) {
            let mpos = self.pos();
            let mods = self.modifiers()?;
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
                        format!("expected field, `init` or `fn` in class body, found {}", other.describe()),
                        self.pos(),
                    )
                }
            }
        }
        self.expect(Tok::RBrace, "`}`")?;
        Ok(class)
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
        if self.check(&Tok::Colon) {
            return err("structs cannot extend classes or implement interfaces (use a class for that)", self.pos());
        }
        self.expect(Tok::LBrace, "`{` to start struct body")?;
        let mut s = StructDecl { name, fields: vec![], methods: vec![], ops: vec![], pos };
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
            if self.peek() == &Tok::Ident("mut".into()) && self.tokens.get(self.i + 1).is_some_and(|t| t.tok == Tok::Fn) {
                self.advance();
                is_mut = true;
            }
            let mut mods = self.modifiers()?;
            mods.is_mut = is_mut;
            match self.peek().clone() {
                Tok::Ident(fname) if !is_mut => {
                    if mods.is_static || mods.is_abstract || mods.is_override {
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
                        return err("struct methods cannot be `abstract` or `override` (structs have no inheritance)", mpos);
                    }
                    if mods.is_mut && mods.is_static {
                        return err("a `static fn` has no `self`, so it cannot be `mut`", mpos);
                    }
                    s.methods.push(self.function(mods)?);
                    self.skip_newlines();
                }
                Tok::Init => return err("structs have no `init`: `Name(a, b)` sets the fields in order; use a `static fn` for other constructors", mpos),
                other => {
                    return err(format!("expected field, `fn`, `mut fn` or `op` in struct body, found {}", other.describe()), self.pos())
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
            other => return err(format!("expected a pattern (a variant, literal or `_`), found {}", other.describe()), pos),
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
            let body = if self.check(&Tok::LBrace) {
                let b = self.block()?;
                self.skip_newlines();
                b
            } else {
                vec![self.statement()?]
            };
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
                let cond = self.expr()?;
                let body = self.block()?;
                self.skip_newlines();
                return Ok(Stmt { kind: StmtKind::While { cond, body }, pos });
            }
            Tok::For => {
                self.advance();
                let (var, _) = self.ident("loop variable name")?;
                self.expect(Tok::In, "`in`")?;
                let first = self.expr()?;
                let iter = if self.eat(&Tok::DotDot) {
                    ForIter::Range(first, self.expr()?)
                } else {
                    ForIter::Each(first)
                };
                let body = self.block()?;
                self.skip_newlines();
                return Ok(Stmt { kind: StmtKind::For { var, iter, body }, pos });
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
        self.end_of_statement()?;
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
        if self.check(&Tok::If) {
            Ok(Some(vec![self.if_stmt()?]))
        } else {
            Ok(Some(self.block()?))
        }
    }

    // ---- expressions, lowest precedence first ----

    pub fn expr(&mut self) -> Result<Expr> {
        self.or()
    }

    fn binary_level(
        &mut self,
        next: fn(&mut Self) -> Result<Expr>,
        ops: &[(Tok, BinOp)],
    ) -> Result<Expr> {
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
        self.binary_level(
            Self::unary,
            &[(Tok::Star, BinOp::Mul), (Tok::Slash, BinOp::Div), (Tok::Percent, BinOp::Rem)],
        )
    }

    fn unary(&mut self) -> Result<Expr> {
        let pos = self.pos();
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
            args.push(self.expr()?);
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
            } else if self.eat(&Tok::LBracket) {
                let idx = self.expr()?;
                self.expect(Tok::RBracket, "`]`")?;
                e = Expr { kind: ExprKind::Index(Box::new(e), Box::new(idx)), pos };
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
                if self.check(&Tok::LParen) {
                    ExprKind::Call(name, targs, self.args()?)
                } else {
                    ExprKind::Ident(name)
                }
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
