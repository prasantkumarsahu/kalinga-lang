//! Signature help: the parameters of the call the cursor is in.

use kalinga_sema::builtins::{self, FUNCTIONS};
use kalinga_sema::index::{PROBE, Probe};
use kalinga_sema::{Model, Signature, Type, subst_sig};
use lsp_types::{ParameterInformation, ParameterLabel, SignatureHelp, SignatureHelpParams, SignatureInformation, Url};

use crate::completion::probe::probe_with;
use crate::describe::params_and_ret;
use crate::snapshot::Snapshot;
use crate::text;
use crate::world::World;

/// The call around the cursor.
struct Call {
    /// The function's name and where it starts.
    name: String,
    name_start: usize,
    /// The receiver before `name.`, if any.
    recv: Option<String>,
    /// Which argument the cursor is in (0-based).
    active: u32,
}

pub fn help(world: &mut World, p: SignatureHelpParams) -> Option<SignatureHelp> {
    let uri = p.text_document_position_params.text_document.uri;
    let text = world.text(&uri)?.to_string();
    let offset = text::offset(&text, p.text_document_position_params.position);
    let call = find_call(&text, offset)?;
    let (label, params) = match &call.recv {
        Some(recv) => member_signature(world, &uri, &text, offset, &call, recv)?,
        None => plain_signature(world, &uri, &text, offset, &call)?,
    };
    let parameters = params
        .into_iter()
        .map(|p| ParameterInformation { label: ParameterLabel::Simple(p), documentation: None })
        .collect();
    let info =
        SignatureInformation { label, documentation: None, parameters: Some(parameters), active_parameter: None };
    Some(SignatureHelp { signatures: vec![info], active_signature: Some(0), active_parameter: Some(call.active) })
}

/// The innermost call whose `(` is open before `offset` on its line.
fn find_call(text: &str, offset: usize) -> Option<Call> {
    let line_start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let line = &text[line_start..offset];
    // (position of each open `(`, commas seen inside it)
    let mut open: Vec<(usize, u32)> = vec![];
    let mut in_string = false;
    let mut chars = line.char_indices();
    while let Some((i, c)) = chars.next() {
        match (in_string, c) {
            (true, '\\') => {
                chars.next();
            }
            (true, '"') | (false, '"') => in_string = !in_string,
            (false, '(' | '[' | '{') => open.push((i, 0)),
            (false, ')' | ']' | '}') => {
                open.pop();
            }
            (false, ',') => {
                if let Some(top) = open.last_mut() {
                    top.1 += 1;
                }
            }
            _ => {}
        }
    }
    let &(paren, active) = open.last()?;
    if line.as_bytes().get(paren) != Some(&b'(') {
        return None;
    }
    let mut before = line[..paren].trim_end();
    // `first<int>(`: skip the type arguments.
    if before.ends_with('>') {
        before = &before[..before.rfind('<')?];
    }
    let name_at = text::name_start(before);
    let name = &before[name_at..];
    if name.is_empty()
        || name.starts_with(|c: char| c.is_ascii_digit())
        || ["if", "while", "for", "fn", "match", "return"].contains(&name)
    {
        return None;
    }
    let recv = before[..name_at].strip_suffix('.').map(|r| {
        let start = text::tail_start(r, |c| c.is_alphanumeric() || "_.()[]".contains(c));
        r[start..].to_string()
    });
    Some(Call { name: name.to_string(), name_start: line_start + name_at, recv, active })
}

fn signature_of(name: &str, sig: &Signature) -> (String, Vec<String>) {
    let params = sig.params.iter().map(|(n, t)| format!("{}: {}", n, t)).collect();
    (format!("{}{}", name, params_and_ret(sig)), params)
}

fn registry(f: &builtins::Function) -> (String, Vec<String>) {
    (f.signature(), f.params.iter().map(|p| p.to_string()).collect())
}

/// `recv.name(` — a method, a `static fn`, an enum variant, a library function or a
/// package's function.
fn member_signature(
    world: &mut World,
    uri: &Url,
    text: &str,
    offset: usize,
    call: &Call,
    recv: &str,
) -> Option<(String, Vec<String>)> {
    let good = world.stable_snapshot(uri, text::line_index(text, offset))?;
    if let Some(unit) = good.analysis.units.file_aliases.first().and_then(|a| a.get(recv)).cloned() {
        let m = good.model()?;
        let (fname, sig) = m.functions.iter().find(|(n, _)| {
            n.split("__").next() == Some(call.name.as_str())
                && unit_of_function(&good, n).as_deref() == Some(unit.as_str())
        })?;
        let _ = fname;
        let (label, params) = signature_of(&format!("{}.{}", recv, call.name), sig);
        return Some((good.show(&label), params));
    }
    let (snap, probe) = probe_with(world, uri, text, call.name_start, offset, PROBE)?;
    let m = snap.model()?;
    let found = match probe {
        Probe::Library(l) => return FUNCTIONS.iter().find(|f| f.name == format!("{}.{}", l, call.name)).map(registry),
        Probe::TypeMembers(t) => type_member(m, &t, &call.name)?,
        Probe::Members { ty, .. } => match method(m, &ty, &call.name) {
            Some(s) => signature_of(&call.name, &s),
            None => {
                let r = kalinga_sema::index::registry_receiver(&ty)?;
                let meth = builtins::methods_of(r).find(|x| x.name == call.name)?;
                (meth.signature(), meth.params.iter().map(|p| p.to_string()).collect())
            }
        },
        Probe::Scope { .. } => return None,
    };
    Some((snap.show(&found.0), found.1.iter().map(|p| snap.show(p)).collect()))
}

fn unit_of_function(snap: &Snapshot, name: &str) -> Option<String> {
    let d = snap.decl(&kalinga_sema::index::Symbol::Function(name.to_string()))?;
    snap.analysis.units.file_unit.get(d.span.file as usize).cloned()
}

fn type_member(m: &Model, t: &str, name: &str) -> Option<(String, Vec<String>)> {
    if let Some(e) = m.enums.get(t) {
        let fields = e.variant(name)?;
        let sig = Signature { params: fields.clone(), ret: Type::Void };
        return Some(signature_of(&format!("{}.{}", t, name), &sig));
    }
    if let Some((_, sig)) = m.classes.contains_key(t).then(|| m.find_static(t, name)).flatten() {
        return Some(signature_of(name, &sig));
    }
    let meth = m.structs.get(t)?.methods.get(name)?;
    Some(signature_of(name, &meth.sig))
}

/// Method `name` of a value of type `ty`, with type arguments filled in.
fn method(m: &Model, ty: &Type, name: &str) -> Option<Signature> {
    match ty {
        Type::Class(c) => m.find_method(c, name).map(|(_, s)| s),
        Type::Generic(c, targs) => m.find_method(c, name).map(|(_, s)| subst_sig(&s, &m.bindings(c, targs))),
        Type::Interface(i) => m.interface_method(i, name),
        Type::Struct(st, targs) => {
            let meth = m.structs.get(st)?.methods.get(name)?;
            Some(subst_sig(&meth.sig, &m.struct_bindings(st, targs)))
        }
        _ => None,
    }
}

/// `name(` — a local function value, a function, a constructor or a built-in.
fn plain_signature(
    world: &mut World,
    uri: &Url,
    text: &str,
    offset: usize,
    call: &Call,
) -> Option<(String, Vec<String>)> {
    if let Some((_, Probe::Scope { locals, .. })) = probe_with(world, uri, text, call.name_start, offset, PROBE)
        && let Some((_, Type::Func(ps, ret))) = locals.iter().find(|(n, _)| *n == call.name)
    {
        let params: Vec<String> = ps.iter().map(|t| t.to_string()).collect();
        let ret = if **ret == Type::Void { String::new() } else { format!(" -> {}", ret) };
        return Some((format!("{}({}){}", call.name, params.join(", "), ret), params));
    }
    let good = world.stable_snapshot(uri, text::line_index(text, offset))?;
    if let Some(m) = good.model() {
        let found = if let Some(sig) = m.functions.get(&call.name).or(m.extern_fns.get(&call.name).map(|(s, _)| s)) {
            Some(signature_of(&call.name, sig))
        } else if m.classes.contains_key(&call.name) {
            let params = m.find_init(&call.name).map(|(_, s)| s.params).unwrap_or_default();
            Some(signature_of(&call.name, &Signature { params, ret: Type::Void }))
        } else {
            m.structs
                .get(&call.name)
                .map(|s| signature_of(&call.name, &Signature { params: s.fields.clone(), ret: Type::Void }))
        };
        if let Some((label, params)) = found {
            return Some((good.show(&label), params.iter().map(|p| good.show(p)).collect()));
        }
    }
    builtins::function(&call.name).map(registry)
}
