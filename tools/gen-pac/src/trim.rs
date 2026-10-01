//! Trims a chiptool RCC block to a set of registers and fields, on the `syn` syntax tree.
//!
//! chiptool output has three parts: the block (`pub struct Rcc` + `impl Rcc` with one accessor per register),
//! `pub mod regs` (one fieldset struct per register with getters / setters, `Default`, `Debug` and
//! `defmt::Format` impls) and `pub mod vals` (enums used by fields). We keep the requested accessors, their
//! fieldsets with only the requested fields (Debug / defmt filtered to match), and the enums still in use.

use anyhow::{Context, Result, bail};
use quote::quote;
use std::collections::{BTreeMap, BTreeSet};
use syn::punctuated::Punctuated;
use syn::visit::Visit;
use syn::{Expr, ImplItem, Item, ItemImpl, ItemMod, Stmt, Type};

/// `keep`: register accessor name → field getter names.
pub fn trim_rcc(src: &str, keep: &BTreeMap<String, BTreeSet<String>>) -> Result<String> {
    let mut file = syn::parse_file(src)?;

    // Block accessors, remembering the fieldset type of each kept register.
    let mut fieldsets: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let block = file
        .items
        .iter_mut()
        .find_map(|item| match item {
            Item::Impl(i)
                if i.trait_.is_none() && type_name(&i.self_ty).as_deref() == Some("Rcc") =>
            {
                Some(i)
            }
            _ => None,
        })
        .context("no `impl Rcc` block")?;
    let mut found = BTreeSet::new();
    let mut err = None;
    block.items.retain(|item| {
        let ImplItem::Fn(f) = item else { return true };
        let name = f.sig.ident.to_string();
        if name == "from_ptr" || name == "as_ptr" {
            return true;
        }
        let Some(fields) = keep.get(&name) else {
            return false;
        };
        match reg_fieldset(&f.sig.output) {
            Some(fs) => {
                fieldsets.insert(fs, fields.clone());
            }
            None => err = Some(format!("cannot find the fieldset type of accessor {name}")),
        }
        found.insert(name);
        true
    });
    if let Some(e) = err {
        bail!(e);
    }
    let missing: Vec<_> = keep.keys().filter(|k| !found.contains(*k)).collect();
    if !missing.is_empty() {
        bail!("registers not found upstream: {missing:?}");
    }

    let regs = module(&mut file, "regs")?;
    let mut err = None;
    regs.retain_mut(|item| match trim_regs_item(item, &fieldsets) {
        Ok(keep) => keep,
        Err(e) => {
            err = Some(e);
            false
        }
    });
    if let Some(e) = err {
        return Err(e);
    }

    // Enums still referenced as `super::vals::X` by the kept fieldsets.
    let mut used = UsedVals::default();
    for item in module(&mut file, "regs")?.iter() {
        used.visit_item(item);
    }
    let used = used.0;
    module(&mut file, "vals")?.retain(|item| item_name(item).is_some_and(|n| used.contains(&n)));

    Ok(prettyplease::unparse(&file))
}

/// Decides whether to keep an item of `mod regs`, trimming it in place.
fn trim_regs_item(item: &mut Item, fieldsets: &BTreeMap<String, BTreeSet<String>>) -> Result<bool> {
    let Some(name) = item_name(item) else {
        return Ok(true);
    };
    let Some(fields) = fieldsets.get(&name) else {
        return Ok(false);
    };
    let Item::Impl(imp) = item else {
        return Ok(true);
    }; // the struct itself
    match trait_name(imp).as_deref() {
        None => {
            let wanted: BTreeSet<String> = fields
                .iter()
                .flat_map(|f| [f.clone(), format!("set_{f}")])
                .collect();
            imp.items.retain(
                |i| matches!(i, ImplItem::Fn(f) if wanted.contains(&f.sig.ident.to_string())),
            );
            let got: BTreeSet<String> = imp
                .items
                .iter()
                .filter_map(|i| match i {
                    ImplItem::Fn(f) => Some(f.sig.ident.to_string()),
                    _ => None,
                })
                .collect();
            let missing: Vec<_> = wanted.difference(&got).collect();
            if !missing.is_empty() {
                bail!("{name}: fields not found upstream: {missing:?}");
            }
        }
        Some("Debug") => filter_debug(imp, fields).with_context(|| format!("Debug for {name}"))?,
        Some("Format") => {
            filter_defmt(imp, fields).with_context(|| format!("defmt::Format for {name}"))?
        }
        Some(_) => {} // Default
    }
    Ok(true)
}

/// `f.debug_struct("X").field("a", &self.a()).field("b[0]", &self.b(0usize)).finish()` → only kept fields.
fn filter_debug(imp: &mut ItemImpl, fields: &BTreeSet<String>) -> Result<()> {
    fn filter(expr: Expr, fields: &BTreeSet<String>) -> Expr {
        match expr {
            Expr::MethodCall(mut mc) if mc.method == "field" => {
                let receiver = filter(*mc.receiver, fields);
                let keep = match mc.args.first() {
                    Some(Expr::Lit(l)) => match &l.lit {
                        syn::Lit::Str(s) => fields.contains(base_field(&s.value())),
                        _ => true,
                    },
                    _ => true,
                };
                if keep {
                    mc.receiver = Box::new(receiver);
                    Expr::MethodCall(mc)
                } else {
                    receiver
                }
            }
            Expr::MethodCall(mut mc) => {
                mc.receiver = Box::new(filter(*mc.receiver, fields));
                Expr::MethodCall(mc)
            }
            other => other,
        }
    }
    let body = &mut only_fn(imp)?.block;
    match body.stmts.last_mut() {
        Some(Stmt::Expr(e, None)) => {
            *e = filter(e.clone(), fields);
            Ok(())
        }
        _ => bail!("unexpected body"),
    }
}

/// `defmt::write!(f, "X {{ a: {=bool:?}, b: {:?} }}", self.a(), self.b())` → only kept fields. chiptool emits
/// exactly one placeholder per field, in the same order as the arguments.
fn filter_defmt(imp: &mut ItemImpl, fields: &BTreeSet<String>) -> Result<()> {
    let body = &mut only_fn(imp)?.block;
    let mac = match body.stmts.last_mut() {
        Some(Stmt::Macro(m)) => &mut m.mac,
        Some(Stmt::Expr(Expr::Macro(m), _)) => &mut m.mac,
        _ => bail!("unexpected body"),
    };
    let args = mac.parse_body_with(Punctuated::<Expr, syn::Token![,]>::parse_terminated)?;
    let mut args = args.into_iter();
    let (Some(f), Some(Expr::Lit(fmt))) = (args.next(), args.next()) else {
        bail!("unexpected arguments")
    };
    let syn::Lit::Str(fmt) = &fmt.lit else {
        bail!("format string is not a string literal")
    };
    let fmt = fmt.value();
    let open = fmt.find(" {{ ").context("no `{{` in format string")?;
    let inner = fmt[open + 4..]
        .strip_suffix(" }}")
        .context("no `}}` in format string")?;
    let pieces: Vec<&str> = inner.split(", ").collect();
    let values: Vec<Expr> = args.collect();
    if pieces.len() != values.len() {
        bail!(
            "{} placeholders but {} arguments",
            pieces.len(),
            values.len()
        );
    }
    let (pieces, values): (Vec<&str>, Vec<Expr>) = pieces
        .into_iter()
        .zip(values)
        .filter(|(p, _)| fields.contains(base_field(p.split(':').next().unwrap_or_default())))
        .unzip();
    let fmt = format!("{} {{{{ {} }}}}", &fmt[..open], pieces.join(", "));
    mac.tokens = quote!(#f, #fmt, #(#values),*);
    Ok(())
}

/// `divqen[1]` → `divqen`.
fn base_field(name: &str) -> &str {
    name.split('[').next().unwrap_or(name)
}

/// The fns of an impl block.
pub fn fns(imp: &ItemImpl) -> impl Iterator<Item = &syn::ImplItemFn> {
    imp.items.iter().filter_map(|i| match i {
        ImplItem::Fn(f) => Some(f),
        _ => None,
    })
}

fn only_fn(imp: &mut ItemImpl) -> Result<&mut syn::ImplItemFn> {
    imp.items
        .iter_mut()
        .find_map(|i| match i {
            ImplItem::Fn(f) => Some(f),
            _ => None,
        })
        .context("impl without fn")
}

fn module<'a>(file: &'a mut syn::File, name: &str) -> Result<&'a mut Vec<Item>> {
    file.items
        .iter_mut()
        .find_map(|item| match item {
            Item::Mod(ItemMod {
                ident,
                content: Some((_, items)),
                ..
            }) if ident == name => Some(items),
            _ => None,
        })
        .with_context(|| format!("no `mod {name}`"))
}

/// The type an item belongs to: struct / enum name, or the implementing type. `impl From<X> for u8` → X.
fn item_name(item: &Item) -> Option<String> {
    match item {
        Item::Struct(s) => Some(s.ident.to_string()),
        Item::Enum(e) => Some(e.ident.to_string()),
        Item::Impl(i) => {
            let self_ty = type_name(&i.self_ty)?;
            if self_ty == "u8" {
                let (_, path, _) = i.trait_.as_ref()?;
                let syn::PathArguments::AngleBracketed(a) = &path.segments.last()?.arguments else {
                    return None;
                };
                match a.args.first()? {
                    syn::GenericArgument::Type(t) => type_name(t),
                    _ => None,
                }
            } else {
                Some(self_ty)
            }
        }
        _ => None,
    }
}

fn trait_name(imp: &ItemImpl) -> Option<String> {
    imp.trait_
        .as_ref()
        .and_then(|(_, p, _)| p.segments.last())
        .map(|s| s.ident.to_string())
}

pub fn type_name(t: &Type) -> Option<String> {
    match t {
        Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
        _ => None,
    }
}

/// `-> crate::pac::common::Reg<regs::X, RW>` → `X`.
pub fn reg_fieldset(output: &syn::ReturnType) -> Option<String> {
    let syn::ReturnType::Type(_, t) = output else {
        return None;
    };
    let Type::Path(p) = &**t else { return None };
    let syn::PathArguments::AngleBracketed(a) = &p.path.segments.last()?.arguments else {
        return None;
    };
    match a.args.first()? {
        syn::GenericArgument::Type(t) => type_name(t),
        _ => None,
    }
}

#[derive(Default)]
struct UsedVals(BTreeSet<String>);

impl<'ast> Visit<'ast> for UsedVals {
    fn visit_path(&mut self, p: &'ast syn::Path) {
        let segs: Vec<String> = p.segments.iter().map(|s| s.ident.to_string()).collect();
        if let Some(i) = segs.iter().position(|s| s == "vals")
            && let Some(name) = segs.get(i + 1)
        {
            self.0.insert(name.clone());
        }
        syn::visit::visit_path(self, p);
    }
}
