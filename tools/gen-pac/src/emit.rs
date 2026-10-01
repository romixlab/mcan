//! Emits `mapping_<feature>.rs`: addresses and FDCAN RCC bits of one chip.

use crate::chips::Chip;
use crate::metadata::ChipInfo;
use anyhow::{Result, bail};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};

pub fn mapping(chip: &Chip, info: &ChipInfo, trimmed_rcc: &str) -> Result<String> {
    let doc = format!(
        " Addresses and FDCAN RCC bits for `{}` (reference chip {}). FDCAN register version `{}`.",
        chip.feature,
        chip.chip.to_uppercase(),
        info.fdcan_version
    );
    let instances = info.instances.iter().map(|i| {
        let name = format_ident!("{}_REGISTER_BLOCK_ADDR", i.name);
        let addr = hex(i.address);
        quote! { pub(crate) const #name: *mut () = #addr as *mut (); }
    });
    let msgram = hex(info.msgram_address);
    let msgram_len = proc_macro2::Literal::usize_unsuffixed(chip.msgram_len_words);
    let rcc_addr = hex(info.rcc_address);

    let rcc = &info.rcc;
    let (en_reg, en) = (
        format_ident!("{}", rcc.enable.register),
        format_ident!("{}", rcc.enable.field),
    );
    let set_en = format_ident!("set_{}", rcc.enable.field);
    let (rst_reg, set_rst) = (
        format_ident!("{}", rcc.reset.register),
        format_ident!("set_{}", rcc.reset.field),
    );
    let (mux_reg, mux) = (
        format_ident!("{}", rcc.kernel_mux.register),
        format_ident!("{}", rcc.kernel_mux.field),
    );
    let mux_ty = mux_type(trimmed_rcc, &rcc.kernel_mux.register, &rcc.kernel_mux.field)?;

    let tokens = quote! {
        #![doc = #doc]

        #(#instances)*
        /// Start of the FDCAN message RAM.
        pub(crate) const FDCAN_MSGRAM_ADDR: *mut u32 = #msgram as *mut u32;
        /// Message RAM size in 32-bit words.
        pub(crate) const FDCAN_MSGRAM_LEN_WORDS: usize = #msgram_len;

        #[cfg(feature = "rcc")]
        pub(crate) const RCC_REGISTER_BLOCK_ADDR: *mut () = #rcc_addr as *mut ();

        /// FDCAN clock enable, reset and kernel clock selection. One set for all instances.
        #[cfg(feature = "rcc")]
        pub(crate) mod rcc_fdcan {
            use crate::pac::rcc::Rcc;

            #[inline]
            pub(crate) fn is_enabled(rcc: Rcc) -> bool {
                rcc.#en_reg().read().#en()
            }

            #[inline]
            pub(crate) fn set_enabled(rcc: Rcc, enabled: bool) {
                rcc.#en_reg().modify(|w| w.#set_en(enabled));
            }

            #[inline]
            pub(crate) fn set_reset(rcc: Rcc, reset: bool) {
                rcc.#rst_reg().modify(|w| w.#set_rst(reset));
            }

            #[inline]
            pub(crate) fn kernel_clock(rcc: Rcc) -> #mux_ty {
                rcc.#mux_reg().read().#mux()
            }
        }
    };
    Ok(prettyplease::unparse(&syn::parse2(tokens)?))
}

/// The return type of the kernel clock mux getter in the trimmed RCC file (`super::vals::Fdcansel`), as
/// a `crate::pac::rcc::vals::...` path.
fn mux_type(trimmed_rcc: &str, register: &str, field: &str) -> Result<TokenStream> {
    use crate::trim::{fns, reg_fieldset, type_name};
    let file = syn::parse_file(trimmed_rcc)?;
    fn inherent(item: &syn::Item) -> Option<&syn::ItemImpl> {
        match item {
            syn::Item::Impl(i) if i.trait_.is_none() => Some(i),
            _ => None,
        }
    }
    let Some(fieldset) = file
        .items
        .iter()
        .filter_map(inherent)
        .flat_map(fns)
        .find(|f| f.sig.ident == register)
        .and_then(|f| reg_fieldset(&f.sig.output))
    else {
        bail!("RCC accessor {register} not found")
    };
    let getter = file
        .items
        .iter()
        .filter_map(|item| match item {
            syn::Item::Mod(m) if m.ident == "regs" => m.content.as_ref(),
            _ => None,
        })
        .flat_map(|(_, items)| items.iter().filter_map(inherent))
        .filter(|i| type_name(&i.self_ty).as_deref() == Some(fieldset.as_str()))
        .flat_map(fns)
        .find(|f| f.sig.ident == field);
    match getter.map(|f| &f.sig.output) {
        Some(syn::ReturnType::Type(_, t)) => match type_name(t) {
            Some(name) => {
                let name = format_ident!("{name}");
                Ok(quote!(crate::pac::rcc::vals::#name))
            }
            None => bail!("RCC field {register}.{field} has an unexpected type"),
        },
        _ => bail!("RCC field {register}.{field} not found"),
    }
}

/// `0x4000_A000` style literal.
fn hex(v: u64) -> syn::LitInt {
    let s = format!("{v:08X}");
    let (hi, lo) = s.split_at(s.len() - 4);
    syn::LitInt::new(&format!("0x{hi}_{lo}"), Span::call_site())
}
