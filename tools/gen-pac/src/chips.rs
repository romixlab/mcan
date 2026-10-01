//! The chips mcan supports and what the generator needs to know about them beyond stm32-metapac's metadata.

use crate::metadata::ChipInfo;
use std::collections::{BTreeMap, BTreeSet};

pub struct Chip {
    /// mcan cargo feature.
    pub feature: &'static str,
    /// stm32-metapac chip used as the reference for addresses and RCC bits (`src/chips/<chip>`).
    pub chip: &'static str,
    /// Message RAM size in 32-bit words. Not in metapac's metadata.
    pub msgram_len_words: usize,
    /// RCC registers and fields used by the driver's clock-source check, on top of the FDCAN enable / reset /
    /// kernel-clock-mux bits that come from the metadata. Field names are getter names (`set_` is implied).
    pub rcc_extra: &'static [(&'static str, &'static [&'static str])],
}

pub const CHIPS: &[Chip] = &[
    Chip {
        feature: "h7",
        chip: "stm32h725ig",
        // 10 KiB shared by all instances (RM0468, FDCAN message RAM).
        msgram_len_words: 2560,
        rcc_extra: &[("cr", &["hseon"]), ("pllcfgr", &["divqen"])],
    },
    Chip {
        feature: "g0",
        chip: "stm32g0b1ce",
        // TODO(R4): metapac shows two 0x350-byte blocks (424 words in total), check against RM0444.
        msgram_len_words: 512,
        rcc_extra: &[("cr", &["hseon"]), ("pllcfgr", &["pllqen"])],
    },
];

impl Chip {
    /// RCC registers (accessor names) and fields to keep.
    pub fn rcc_keep(&self, info: &ChipInfo) -> BTreeMap<String, BTreeSet<String>> {
        let mut keep: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (reg, fields) in self.rcc_extra {
            keep.entry(reg.to_string())
                .or_default()
                .extend(fields.iter().map(|f| f.to_string()));
        }
        for bit in [&info.rcc.enable, &info.rcc.reset, &info.rcc.kernel_mux] {
            keep.entry(bit.register.clone())
                .or_default()
                .insert(bit.field.clone());
        }
        keep
    }
}
