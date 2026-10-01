//! Reads the per-chip metadata (`src/chips/<chip>/metadata.rs`) of stm32-metapac with `syn`.
//!
//! The metadata is Rust source: `static PERIPHERALS: &[Peripheral] = &[Peripheral { name: "FDCAN1", address:
//! 0x4000a000, registers: Some(PeripheralRegisters { kind: "can", version: "fdcan_h7", .. }), rcc:
//! Some(PeripheralRcc { kernel_clock: Mux(..), enable: Some(..), reset: Some(..), .. }), .. }, ..]`.

use crate::chips::Chip;
use anyhow::{Context, Result, bail};
use std::path::Path;
use syn::{Expr, ExprStruct, Item, Lit};

/// A register field in RCC, lower-cased to match the generated accessor names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RccBit {
    pub register: String,
    pub field: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FdcanRcc {
    pub enable: RccBit,
    pub reset: RccBit,
    pub kernel_mux: RccBit,
}

pub struct Instance {
    /// `FDCAN1`, `FDCAN2`, ...
    pub name: String,
    pub address: u64,
}

/// What the generator uses from one chip's metadata.
pub struct ChipInfo {
    pub instances: Vec<Instance>,
    /// Register block version, e.g. `fdcan_h7` or `fdcan_v1`.
    pub fdcan_version: String,
    /// Start of the message RAM (first block on chips with one block per instance).
    pub msgram_address: u64,
    pub rcc_address: u64,
    /// RCC register block version, e.g. `h7` or `g0x1`.
    pub rcc_version: String,
    /// Shared by all instances.
    pub rcc: FdcanRcc,
}

struct Peripheral {
    name: String,
    address: u64,
    kind: Option<String>,
    version: Option<String>,
    rcc: Option<FdcanRcc>,
}

impl ChipInfo {
    pub fn load(metapac: &Path, chip: &Chip) -> Result<Self> {
        let periphs = load_peripherals(metapac, chip.chip)
            .with_context(|| format!("metadata of {}", chip.chip))?;
        let ctx = || format!("metadata of {}", chip.chip);

        let mut fdcans: Vec<&Peripheral> = periphs
            .iter()
            .filter(|p| p.kind.as_deref() == Some("can"))
            .collect();
        fdcans.retain(|p| p.version.as_deref().is_some_and(|v| v.starts_with("fdcan")));
        fdcans.sort_by(|a, b| a.name.cmp(&b.name));
        let Some(first) = fdcans.first() else {
            bail!("{}: no FDCAN instances", ctx())
        };
        let fdcan_version = first.version.clone().unwrap_or_default();
        let rcc = first
            .rcc
            .clone()
            .with_context(|| format!("{}: {} has no RCC info", ctx(), first.name))?;
        for p in &fdcans {
            // The driver assumes one register map and one shared clock / reset for all instances.
            if p.version.as_deref() != Some(&fdcan_version) {
                bail!(
                    "{}: {} has register version {:?}, {} has {fdcan_version}",
                    ctx(),
                    p.name,
                    p.version,
                    first.name
                );
            }
            if p.rcc.as_ref() != Some(&rcc) {
                bail!(
                    "{}: {} and {} have different RCC bits",
                    ctx(),
                    p.name,
                    first.name
                );
            }
        }

        let msgram_address = periphs
            .iter()
            .filter(|p| p.kind.as_deref() == Some("fdcanram"))
            .map(|p| p.address)
            .min()
            .with_context(|| format!("{}: no FDCANRAM", ctx()))?;
        let rcc_periph = periphs
            .iter()
            .find(|p| p.name == "RCC")
            .with_context(|| format!("{}: no RCC", ctx()))?;

        Ok(ChipInfo {
            instances: fdcans
                .iter()
                .map(|p| Instance {
                    name: p.name.clone(),
                    address: p.address,
                })
                .collect(),
            fdcan_version,
            msgram_address,
            rcc_address: rcc_periph.address,
            rcc_version: rcc_periph
                .version
                .clone()
                .context("RCC has no register version")?,
            rcc,
        })
    }
}

fn load_peripherals(metapac: &Path, chip: &str) -> Result<Vec<Peripheral>> {
    let chip_dir = metapac.join("src/chips").join(chip);
    let chip_file = parse(&chip_dir.join("metadata.rs"))?;

    // The peripherals live in a shared file pulled in with `include!("../metadata_XXXX.rs")`.
    let include = chip_file
        .items
        .iter()
        .find_map(|item| match item {
            Item::Macro(m) if m.mac.path.is_ident("include") => {
                m.mac.parse_body::<syn::LitStr>().ok()
            }
            _ => None,
        })
        .context("no include!() in metadata.rs")?;
    let shared = parse(&chip_dir.join(include.value()))?;

    let list = shared
        .items
        .iter()
        .find_map(|item| match item {
            Item::Static(s) if s.ident == "PERIPHERALS" => Some(&*s.expr),
            _ => None,
        })
        .context("no PERIPHERALS static")?;
    let Expr::Reference(r) = list else {
        bail!("PERIPHERALS is not a reference")
    };
    let Expr::Array(arr) = &*r.expr else {
        bail!("PERIPHERALS is not an array")
    };

    arr.elems.iter().map(peripheral).collect()
}

fn parse(path: &Path) -> Result<syn::File> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    syn::parse_file(&text).with_context(|| format!("parsing {}", path.display()))
}

fn peripheral(expr: &Expr) -> Result<Peripheral> {
    let s = as_struct(expr, "Peripheral")?;
    let name = lit_str(field(s, "name")?)?;
    let ctx = || format!("peripheral {name}");
    let regs = option(field(s, "registers")?)
        .map(|e| as_struct(e, "PeripheralRegisters"))
        .transpose()?;
    let rcc = match option(field(s, "rcc")?) {
        Some(e) => fdcan_rcc(as_struct(e, "PeripheralRcc")?).with_context(ctx)?,
        None => None,
    };
    Ok(Peripheral {
        address: lit_int(field(s, "address")?).with_context(ctx)?,
        kind: regs
            .map(|r| field(r, "kind").and_then(lit_str))
            .transpose()?,
        version: regs
            .map(|r| field(r, "version").and_then(lit_str))
            .transpose()?,
        rcc,
        name,
    })
}

/// Enable, reset and kernel clock mux. None if one of them is missing (only relevant for FDCAN).
fn fdcan_rcc(s: &ExprStruct) -> Result<Option<FdcanRcc>> {
    let bit = |e: &Expr| -> Result<RccBit> {
        let s = as_struct(e, "PeripheralRccRegister")?;
        Ok(RccBit {
            register: lit_str(field(s, "register")?)?.to_lowercase(),
            field: lit_str(field(s, "field")?)?.to_lowercase(),
        })
    };
    let enable = option(field(s, "enable")?).map(bit).transpose()?;
    let reset = option(field(s, "reset")?).map(bit).transpose()?;
    // `kernel_clock: Mux(PeripheralRccRegister { .. })` or `Clock("..")`.
    let kernel_mux = match field(s, "kernel_clock")? {
        Expr::Call(c) if is_path(&c.func, "Mux") => c.args.first().map(bit).transpose()?,
        _ => None,
    };
    Ok(match (enable, reset, kernel_mux) {
        (Some(enable), Some(reset), Some(kernel_mux)) => Some(FdcanRcc {
            enable,
            reset,
            kernel_mux,
        }),
        _ => None,
    })
}

fn as_struct<'a>(e: &'a Expr, name: &str) -> Result<&'a ExprStruct> {
    match e {
        Expr::Struct(s) if s.path.is_ident(name) => Ok(s),
        _ => bail!("expected a {name} struct literal"),
    }
}

fn field<'a>(s: &'a ExprStruct, name: &str) -> Result<&'a Expr> {
    s.fields
        .iter()
        .find(|f| matches!(&f.member, syn::Member::Named(i) if i == name))
        .map(|f| &f.expr)
        .with_context(|| format!("no field {name}"))
}

/// `Some(x)` → `Some(&x)`, `None` → `None`.
fn option(e: &Expr) -> Option<&Expr> {
    match e {
        Expr::Call(c) if is_path(&c.func, "Some") => c.args.first(),
        _ => None,
    }
}

fn is_path(e: &Expr, name: &str) -> bool {
    matches!(e, Expr::Path(p) if p.path.is_ident(name))
}

fn lit_str(e: &Expr) -> Result<String> {
    match e {
        Expr::Lit(l) => match &l.lit {
            Lit::Str(s) => Ok(s.value()),
            _ => bail!("expected a string literal"),
        },
        _ => bail!("expected a string literal"),
    }
}

fn lit_int(e: &Expr) -> Result<u64> {
    match e {
        Expr::Lit(l) => match &l.lit {
            Lit::Int(i) => Ok(i.base10_parse()?),
            _ => bail!("expected an integer literal"),
        },
        _ => bail!("expected an integer literal"),
    }
}
