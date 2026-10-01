//! Host tests for the generated register map (FEATURES.md P7a).
//!
//! Expected offsets and bit positions are typed in from the reference manuals, not taken from stm32-data,
//! so they catch upstream data bugs as well as generator mistakes. Each test points the generated block at
//! a zeroed RAM buffer, writes one field through the generated API and checks which word / bit changed.

extern crate std;
use std::vec::Vec;

/// Words of fake register space, enough for every block used here (RCC on H7 goes up to 0x160).
const WORDS: usize = 0x400 / 4;

/// Runs `write` against a zeroed fake block and returns the (byte offset, value) of every changed word.
fn changed(write: impl FnOnce(*mut ())) -> Vec<(usize, u32)> {
    let mut mem = [0u32; WORDS];
    write(mem.as_mut_ptr() as *mut ());
    mem.iter()
        .enumerate()
        .filter(|(_, v)| **v != 0)
        .map(|(i, v)| (i * 4, *v))
        .collect()
}

/// Asserts that `write` sets exactly the bits `value` in the register at `offset`.
#[track_caller]
fn expect(offset: usize, value: u32, write: impl FnOnce(*mut ())) {
    assert_eq!(
        changed(write),
        [(offset, value)],
        "expected {value:#010x} at {offset:#05x}"
    );
}

#[cfg(feature = "h7")]
mod fdcan_h7 {
    //! Full M_CAN: RM0468 (STM32H72x/73x) section 56.5 "FDCAN registers", Bosch M_CAN user manual
    //! section 2.3 "Register description".
    use super::expect;
    use crate::pac::fdcan_h7::Fdcan;

    fn regs(p: *mut ()) -> Fdcan {
        unsafe { Fdcan::from_ptr(p) }
    }

    #[test]
    fn cccr() {
        expect(0x18, 1 << 0, |p| regs(p).cccr().write(|w| w.set_init(true)));
        expect(0x18, 1 << 1, |p| regs(p).cccr().write(|w| w.set_cce(true)));
        expect(0x18, 1 << 4, |p| regs(p).cccr().write(|w| w.set_csr(true)));
        expect(0x18, 1 << 8, |p| regs(p).cccr().write(|w| w.set_fdoe(true)));
        expect(0x18, 1 << 9, |p| regs(p).cccr().write(|w| w.set_bse(true)));
    }

    #[test]
    fn bit_timing() {
        expect(0x1C, 0x1FF << 16, |p| {
            regs(p).nbtp().write(|w| w.set_nbrp(0x1FF))
        });
        expect(0x1C, 0x7F << 25, |p| {
            regs(p).nbtp().write(|w| w.set_nsjw(0x7F))
        });
        expect(0x0C, 0x1F << 16, |p| {
            regs(p).dbtp().write(|w| w.set_dbrp(0x1F))
        });
    }

    #[test]
    fn interrupts() {
        expect(0x50, 1 << 9, |p| regs(p).ir().write(|w| w.set_tc(true)));
        expect(0x50, 1 << 19, |p| regs(p).ir().write(|w| w.set_drx(true)));
        expect(0x50, 1 << 25, |p| regs(p).ir().write(|w| w.set_bo(true)));
        expect(0x54, 1 << 29, |p| regs(p).ie().write(|w| w.set_arae(true)));
        expect(0x5C, 1 << 0, |p| regs(p).ile().write(|w| w.set_eint0(true)));
    }

    #[test]
    fn filter_and_ram_config() {
        expect(0x80, 0b11 << 4, |p| {
            regs(p).gfc().write(|w| w.set_anfs(0b11))
        });
        expect(0x84, 0xFF << 16, |p| {
            regs(p).sidfc().write(|w| w.set_lss(0xFF))
        });
        expect(0x88, 0x7F << 16, |p| {
            regs(p).xidfc().write(|w| w.set_lse(0x7F))
        });
        expect(0xA0, 0x7F << 16, |p| {
            regs(p).rxfc(0).write(|w| w.set_fs(0x7F))
        });
        expect(0xB0, 0x7F << 16, |p| {
            regs(p).rxfc(1).write(|w| w.set_fs(0x7F))
        });
        expect(0xAC, 0x3FFF << 2, |p| {
            regs(p).rxbc().write(|w| w.set_rbsa(0x3FFF))
        });
        expect(0xBC, 0b111 << 8, |p| {
            regs(p).rxesc().write(|w| w.set_rbds(0b111))
        });
        expect(0xC0, 0x3F << 24, |p| {
            regs(p).txbc().write(|w| w.set_tfqs(0x3F))
        });
        expect(0xC8, 0b111, |p| {
            regs(p).txesc().write(|w| w.set_tbds(0b111))
        });
        expect(0xF0, 0x3F << 16, |p| {
            regs(p).txefc().write(|w| w.set_efs(0x3F))
        });
        expect(0x100, 0x7F << 16, |p| {
            regs(p).tttmc().write(|w| w.set_tme(0x7F))
        });
    }

    #[test]
    fn tx_buffers() {
        expect(0xD0, 1 << 31, |p| {
            regs(p).txbar().write(|w| w.set_ar(31, true))
        });
        expect(0xD4, 1 << 5, |p| {
            regs(p).txbcr().write(|w| w.set_cr(5, true))
        });
        expect(0xE0, 1 << 31, |p| {
            regs(p).txbtie().write(|w| w.set_tie(31, true))
        });
    }
}

#[cfg(feature = "g0")]
mod fdcan_v1 {
    //! FDCAN lite: RM0444 (STM32G0x1) section 34.4 "FDCAN registers".
    use super::expect;
    use crate::pac::fdcan_v1::Fdcan;
    use crate::pac::fdcan_v1::vals::Anfs;

    fn regs(p: *mut ()) -> Fdcan {
        unsafe { Fdcan::from_ptr(p) }
    }

    #[test]
    fn cccr() {
        expect(0x18, 1 << 0, |p| regs(p).cccr().write(|w| w.set_init(true)));
        expect(0x18, 1 << 9, |p| regs(p).cccr().write(|w| w.set_brse(true)));
    }

    #[test]
    fn interrupts() {
        // Lite IR bits are packed differently from the full core (TC is bit 9 and BO bit 25 on H7).
        expect(0x50, 1 << 7, |p| regs(p).ir().write(|w| w.set_tc(true)));
        expect(0x50, 1 << 9, |p| regs(p).ir().write(|w| w.set_tfe(true)));
        expect(0x50, 1 << 19, |p| regs(p).ir().write(|w| w.set_bo(true)));
        expect(0x54, 1 << 23, |p| regs(p).ie().write(|w| w.set_arae(true)));
        expect(0x58, 1 << 6, |p| regs(p).ils().write(|w| w.set_perr(true)));
    }

    #[test]
    fn filter_config() {
        expect(0x80, 0b10 << 4, |p| {
            regs(p).rxgfc().write(|w| w.set_anfs(Anfs::REJECT))
        });
        expect(0x80, 0x1F << 16, |p| {
            regs(p).rxgfc().write(|w| w.set_lss(0x1F))
        });
        expect(0x80, 0xF << 24, |p| {
            regs(p).rxgfc().write(|w| w.set_lse(0xF))
        });
    }

    #[test]
    fn tx() {
        expect(0xC0, 1 << 24, |p| {
            regs(p)
                .txbc()
                .write(|w| w.set_tfqm(crate::pac::fdcan_v1::vals::Tfqm::QUEUE))
        });
        expect(0xCC, 1 << 2, |p| {
            regs(p).txbar().write(|w| w.set_ar(2, true))
        });
        expect(0xE8, 0b11, |p| regs(p).txefa().write(|w| w.set_efai(0b11)));
    }

    #[test]
    fn ckdiv() {
        expect(0x100, 0xF, |p| {
            regs(p)
                .ckdiv()
                .write(|w| w.set_pdiv(crate::pac::fdcan_v1::vals::Pdiv::from_bits(0xF)))
        });
    }
}

#[cfg(all(feature = "h7", feature = "rcc"))]
mod rcc_h7 {
    //! RM0468 section 8.7 "RCC register description".
    use super::expect;
    use crate::pac::rcc_h7::{Rcc, vals::Fdcansel};

    fn rcc(p: *mut ()) -> Rcc {
        unsafe { Rcc::from_ptr(p) }
    }

    #[test]
    fn fdcan_clock() {
        expect(0x00, 1 << 16, |p| rcc(p).cr().write(|w| w.set_hseon(true)));
        expect(0x2C, 1 << 17, |p| {
            rcc(p).pllcfgr().write(|w| w.set_divqen(0, true))
        });
        expect(0x2C, 1 << 20, |p| {
            rcc(p).pllcfgr().write(|w| w.set_divqen(1, true))
        });
        expect(0x50, 0b10 << 28, |p| {
            rcc(p)
                .d2ccip1r()
                .write(|w| w.set_fdcansel(Fdcansel::PLL2_Q))
        });
        expect(0x94, 1 << 8, |p| {
            rcc(p).apb1hrstr().write(|w| w.set_fdcanrst(true))
        });
        expect(0xEC, 1 << 8, |p| {
            rcc(p).apb1henr().write(|w| w.set_fdcanen(true))
        });
    }
}

#[cfg(all(feature = "g0", feature = "rcc"))]
mod rcc_g0 {
    //! RM0444 section 5.4 "RCC registers".
    use super::expect;
    use crate::pac::rcc_g0::{Rcc, vals::Fdcansel};

    fn rcc(p: *mut ()) -> Rcc {
        unsafe { Rcc::from_ptr(p) }
    }

    #[test]
    fn fdcan_clock() {
        expect(0x00, 1 << 16, |p| rcc(p).cr().write(|w| w.set_hseon(true)));
        expect(0x0C, 1 << 24, |p| {
            rcc(p).pllcfgr().write(|w| w.set_pllqen(true))
        });
        expect(0x2C, 1 << 12, |p| {
            rcc(p).apbrstr1().write(|w| w.set_fdcanrst(true))
        });
        expect(0x3C, 1 << 12, |p| {
            rcc(p).apbenr1().write(|w| w.set_fdcanen(true))
        });
        expect(0x58, 0b10 << 8, |p| {
            rcc(p).ccipr2().write(|w| w.set_fdcansel(Fdcansel::HSE))
        });
    }
}
