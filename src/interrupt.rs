//! Interrupt sources, line selection and the interrupt handler (FEATURES.md I1, I2).
//!
//! The driver enables only the interrupts listed in [FdCanConfig::interrupts](crate::config::FdCanConfig)
//! (plus Bus_Off when automatic bus-off recovery is on). Each source goes to interrupt line 0 or 1
//! (FDCAN_INTR0_IT / FDCAN_INTR1_IT, `FDCANx_IT0` / `FDCANx_IT1` in the vector table). Call [on_interrupt]
//! from both handlers. It clears only the flags it handles and latches them per instance, so
//! [FdCan::take_interrupt_flags] and the async waits still see them.

use crate::fdcan::FdCan;
use crate::pac::fdcan::regs::{Ie, Ile, Ils, Ir, Txbcie, Txbtie};
use crate::{FdCanInstance, RxFifo};
use portable_atomic::{AtomicBool, AtomicU32, Ordering};

/// One of the two FDCAN interrupt lines of an instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum InterruptLine {
    /// FDCAN_INTR0_IT, vector `FDCANx_IT0`.
    Line0,
    /// FDCAN_INTR1_IT, vector `FDCANx_IT1`.
    Line1,
}

/// A set of interrupt sources: a mask over the IR / IE registers of the selected core.
///
/// Bit positions differ between the full (H7) and lite (G0, G4, L5, H5) cores, so build sets from the named
/// constants. Sources that only exist on the full core (FIFO watermarks, dedicated RX buffers) are only
/// defined with the `h7` feature.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct Interrupts(u32);

macro_rules! sources {
    ($($(#[$m:meta])* $name:ident = $h7:expr, $lite:expr;)*) => {
        impl Interrupts {
            $(
                $(#[$m])*
                #[cfg(feature = "h7")]
                pub const $name: Self = Self(1 << $h7);
                $(#[$m])*
                #[cfg(not(feature = "h7"))]
                pub const $name: Self = Self(1 << $lite);
            )*
        }
    };
}

// Bit positions: RM0468 FDCAN_IR (= Bosch M_CAN user manual IR) and RM0444 FDCAN_IR.
sources! {
    /// RF0N: new message in RX FIFO 0.
    RX_FIFO0_NEW_MESSAGE = 0, 0;
    /// RF0F: RX FIFO 0 full.
    RX_FIFO0_FULL = 2, 1;
    /// RF0L: message lost, RX FIFO 0 was full (or written with size 0).
    RX_FIFO0_MESSAGE_LOST = 3, 2;
    /// RF1N: new message in RX FIFO 1.
    RX_FIFO1_NEW_MESSAGE = 4, 3;
    /// RF1F: RX FIFO 1 full.
    RX_FIFO1_FULL = 6, 4;
    /// RF1L: message lost, RX FIFO 1 was full.
    RX_FIFO1_MESSAGE_LOST = 7, 5;
    /// HPM: high-priority message received.
    HIGH_PRIORITY_MESSAGE = 8, 6;
    /// TC: transmission completed (for buffers enabled in TXBTIE, all of them).
    TX_COMPLETED = 9, 7;
    /// TCF: transmission cancellation finished (for buffers enabled in TXBCIE, all of them).
    TX_CANCELLATION_FINISHED = 10, 8;
    /// TFE: TX FIFO empty.
    TX_FIFO_EMPTY = 11, 9;
    /// TEFN: new TX event FIFO entry.
    TX_EVENT_FIFO_NEW_ENTRY = 12, 10;
    /// TEFF: TX event FIFO full.
    TX_EVENT_FIFO_FULL = 14, 11;
    /// TEFL: TX event FIFO element lost.
    TX_EVENT_FIFO_ELEMENT_LOST = 15, 12;
    /// TSW: timestamp counter wrapped around.
    TIMESTAMP_WRAPAROUND = 16, 13;
    /// MRAF: message RAM access failure.
    MESSAGE_RAM_ACCESS_FAILURE = 17, 14;
    /// TOO: timeout counter reached zero.
    TIMEOUT_OCCURRED = 18, 15;
    /// ELO: CAN error logging counter (ECR.CEL) overflowed.
    ERROR_LOGGING_OVERFLOW = 22, 16;
    /// EP: PSR.EP (error passive) changed.
    ERROR_PASSIVE = 23, 17;
    /// EW: PSR.EW (error warning, a counter >= 96) changed.
    WARNING_STATUS = 24, 18;
    /// BO: PSR.BO (bus-off) changed, both entering and leaving bus-off.
    BUS_OFF = 25, 19;
    /// WDI: message RAM watchdog.
    WATCHDOG = 26, 20;
    /// PEA: protocol error in the arbitration phase (or a classic frame), see PSR.LEC.
    PROTOCOL_ERROR_ARBITRATION = 27, 21;
    /// PED: protocol error in the data phase of an FD frame with BRS, see PSR.DLEC.
    PROTOCOL_ERROR_DATA = 28, 22;
    /// ARA: access to a reserved address.
    ACCESS_TO_RESERVED_ADDRESS = 29, 23;
}

#[cfg(feature = "h7")]
impl Interrupts {
    /// RF0W: RX FIFO 0 watermark reached.
    pub const RX_FIFO0_WATERMARK: Self = Self(1 << 1);
    /// RF1W: RX FIFO 1 watermark reached.
    pub const RX_FIFO1_WATERMARK: Self = Self(1 << 5);
    /// TEFW: TX event FIFO watermark reached.
    pub const TX_EVENT_FIFO_WATERMARK: Self = Self(1 << 13);
    /// DRX: message stored in a dedicated RX buffer.
    pub const DEDICATED_RX_BUFFER: Self = Self(1 << 19);
    /// Every named source. IR bits 20/21 (BEC/BEU, message RAM ECC) are left out until FEATURES.md E5.
    pub const ALL: Self = Self(0x3FCF_FFFF);
}

#[cfg(not(feature = "h7"))]
impl Interrupts {
    /// Every source (IR bits 23:0).
    pub const ALL: Self = Self(0x00FF_FFFF);
}

impl Interrupts {
    pub const NONE: Self = Self(0);
    /// Errors and bus state: [Self::ERROR_PASSIVE], [Self::WARNING_STATUS], [Self::BUS_OFF],
    /// [Self::PROTOCOL_ERROR_ARBITRATION], [Self::PROTOCOL_ERROR_DATA], [Self::ERROR_LOGGING_OVERFLOW].
    pub const ERRORS: Self = Self::ERROR_PASSIVE
        .union(Self::WARNING_STATUS)
        .union(Self::BUS_OFF)
        .union(Self::PROTOCOL_ERROR_ARBITRATION)
        .union(Self::PROTOCOL_ERROR_DATA)
        .union(Self::ERROR_LOGGING_OVERFLOW);

    /// RFxN for `fifo`.
    pub const fn rx_fifo_new_message(fifo: RxFifo) -> Self {
        match fifo {
            RxFifo::Fifo0 => Self::RX_FIFO0_NEW_MESSAGE,
            RxFifo::Fifo1 => Self::RX_FIFO1_NEW_MESSAGE,
        }
    }

    /// RFxF for `fifo`.
    pub const fn rx_fifo_full(fifo: RxFifo) -> Self {
        match fifo {
            RxFifo::Fifo0 => Self::RX_FIFO0_FULL,
            RxFifo::Fifo1 => Self::RX_FIFO1_FULL,
        }
    }

    /// RFxL for `fifo`.
    pub const fn rx_fifo_message_lost(fifo: RxFifo) -> Self {
        match fifo {
            RxFifo::Fifo0 => Self::RX_FIFO0_MESSAGE_LOST,
            RxFifo::Fifo1 => Self::RX_FIFO1_MESSAGE_LOST,
        }
    }

    /// The raw IR / IE bit mask.
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Builds a set from a raw IR / IE value, dropping bits that are no source of the selected core.
    pub const fn from_bits_truncate(bits: u32) -> Self {
        Self(bits & Self::ALL.0)
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl core::ops::BitOr for Interrupts {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        self.union(rhs)
    }
}

impl core::ops::BitOrAssign for Interrupts {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = self.union(rhs);
    }
}

impl core::ops::BitAnd for Interrupts {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        self.intersection(rhs)
    }
}

/// Lite cores select the line per group of sources, one ILS bit per group (RM0444 FDCAN_ILS):
/// RXFIFO0, RXFIFO1, SMSG, TFERR, MISC, BERR, PERR.
#[cfg(not(feature = "h7"))]
const ILS_GROUPS: [Interrupts; 7] = {
    use Interrupts as I;
    [
        I::RX_FIFO0_NEW_MESSAGE
            .union(I::RX_FIFO0_FULL)
            .union(I::RX_FIFO0_MESSAGE_LOST),
        I::RX_FIFO1_NEW_MESSAGE
            .union(I::RX_FIFO1_FULL)
            .union(I::RX_FIFO1_MESSAGE_LOST),
        I::TX_COMPLETED
            .union(I::TX_CANCELLATION_FINISHED)
            .union(I::HIGH_PRIORITY_MESSAGE),
        I::TX_FIFO_EMPTY
            .union(I::TX_EVENT_FIFO_NEW_ENTRY)
            .union(I::TX_EVENT_FIFO_FULL)
            .union(I::TX_EVENT_FIFO_ELEMENT_LOST),
        I::TIMESTAMP_WRAPAROUND
            .union(I::MESSAGE_RAM_ACCESS_FAILURE)
            .union(I::TIMEOUT_OCCURRED),
        I::ERROR_PASSIVE.union(I::ERROR_LOGGING_OVERFLOW),
        I::ACCESS_TO_RESERVED_ADDRESS
            .union(I::PROTOCOL_ERROR_DATA)
            .union(I::PROTOCOL_ERROR_ARBITRATION)
            .union(I::WATCHDOG)
            .union(I::BUS_OFF)
            .union(I::WARNING_STATUS),
    ]
};

/// ILS value that routes `line1` to line 1 and everything else to line 0.
///
/// Full core: one ILS bit per IR bit. Lite cores: a group goes to line 1 if any of its sources is in `line1`.
pub(crate) const fn ils_bits(line1: Interrupts) -> u32 {
    #[cfg(feature = "h7")]
    {
        line1.intersection(Interrupts::ALL).0
    }
    #[cfg(not(feature = "h7"))]
    {
        let mut ils = 0;
        let mut i = 0;
        while i < ILS_GROUPS.len() {
            if line1.intersects(ILS_GROUPS[i]) {
                ils |= 1 << i;
            }
            i += 1;
        }
        ils
    }
}

/// The sources that an ILS value routes to line 1.
pub(crate) const fn line1_sources(ils: u32) -> Interrupts {
    #[cfg(feature = "h7")]
    {
        Interrupts::from_bits_truncate(ils)
    }
    #[cfg(not(feature = "h7"))]
    {
        let mut set = Interrupts::NONE;
        let mut i = 0;
        while i < ILS_GROUPS.len() {
            if ils & (1 << i) != 0 {
                set = set.union(ILS_GROUPS[i]);
            }
            i += 1;
        }
        set
    }
}

/// Sources on `line`, given the ILS value.
const fn line_sources(ils: u32, line: InterruptLine) -> Interrupts {
    let line1 = line1_sources(ils);
    match line {
        InterruptLine::Line0 => Interrupts::ALL.difference(line1),
        InterruptLine::Line1 => line1,
    }
}

/// ILE value: a line is enabled when at least one enabled source is routed to it.
pub(crate) const fn ile_bits(enabled: Interrupts, ils: u32) -> (bool, bool) {
    (
        enabled.intersects(line_sources(ils, InterruptLine::Line0)),
        enabled.intersects(line_sources(ils, InterruptLine::Line1)),
    )
}

/// PSR fields that a read resets (Bosch M_CAN user manual PSR, RM0468 / RM0444 FDCAN_PSR): LEC and DLEC go
/// to 7 ("no change"), RESI, RBRS, REDL and PXE to 0.
pub(crate) const PSR_RESET_ON_READ: u32 = 0x7 | 0x7 << 8 | 0xF << 11;
/// LEC = DLEC = 7, flags clear: nothing latched.
pub(crate) const PSR_NOTHING_LATCHED: u32 = 0x7 | 0x7 << 8;

/// Combines two reads of the reset-on-read PSR fields. Error codes come from `newer` unless it says "no
/// change" (7), flags are or-ed. Bits outside [PSR_RESET_ON_READ] come from `newer`.
pub(crate) const fn merge_psr(newer: u32, older: u32) -> u32 {
    const LEC: u32 = 0x7;
    const DLEC: u32 = 0x7 << 8;
    const FLAGS: u32 = 0xF << 11;
    let lec = if newer & LEC == LEC {
        older & LEC
    } else {
        newer & LEC
    };
    let dlec = if newer & DLEC == DLEC {
        older & DLEC
    } else {
        newer & DLEC
    };
    (newer & !PSR_RESET_ON_READ) | lec | dlec | ((newer | older) & FLAGS)
}

/// Per-instance state shared between the driver and [on_interrupt].
pub(crate) struct State {
    /// IR flags cleared by [on_interrupt] and not yet taken.
    events: AtomicU32,
    /// Reset-on-read PSR fields seen by reads that didn't hand them to the user.
    psr: AtomicU32,
    pub(crate) automatic_bus_off_recovery: AtomicBool,
    #[cfg(feature = "asynchronous")]
    pub(crate) waker: embassy_sync::waitqueue::AtomicWaker,
}

impl State {
    const fn new() -> Self {
        Self {
            events: AtomicU32::new(0),
            psr: AtomicU32::new(PSR_NOTHING_LATCHED),
            automatic_bus_off_recovery: AtomicBool::new(false),
            #[cfg(feature = "asynchronous")]
            waker: embassy_sync::waitqueue::AtomicWaker::new(),
        }
    }

    /// Takes the latched flags in `mask`.
    pub(crate) fn take_events(&self, mask: u32) -> u32 {
        self.events.fetch_and(!mask, Ordering::AcqRel) & mask
    }

    pub(crate) fn peek_events(&self) -> u32 {
        self.events.load(Ordering::Acquire)
    }

    /// Keeps the reset-on-read fields of a PSR read for the next [Self::take_psr].
    pub(crate) fn latch_psr(&self, psr: u32) {
        let newer = psr & PSR_RESET_ON_READ;
        _ = self
            .psr
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |older| {
                Some(merge_psr(newer, older))
            });
    }

    /// Merges a fresh PSR read with everything latched since the last call and resets the latch.
    pub(crate) fn take_psr(&self, psr: u32) -> u32 {
        merge_psr(psr, self.psr.swap(PSR_NOTHING_LATCHED, Ordering::AcqRel))
    }
}

#[cfg(feature = "h7")]
const INSTANCES: usize = 3;
#[cfg(not(feature = "h7"))]
const INSTANCES: usize = 2;

static STATES: [State; INSTANCES] = [const { State::new() }; INSTANCES];

pub(crate) fn state(instance: FdCanInstance) -> &'static State {
    &STATES[instance as usize]
}

/// Interrupt handler. Call it from both interrupt vectors of the instance (`FDCANx_IT0` with
/// [InterruptLine::Line0], `FDCANx_IT1` with [InterruptLine::Line1]).
///
/// Handles the flags that are set, enabled and routed to `line`: clears them in IR, latches them for
/// [FdCan::take_interrupt_flags] and wakes the instance's async waiter. On Bus_Off it reads PSR (keeping the
/// reset-on-read fields for [FdCan::protocol_status]) and, if [automatic bus-off
/// recovery](crate::config::FdCanConfig::automatic_bus_off_recovery) is on, starts the recovery by clearing
/// CCCR.INIT (Bosch M_CAN user manual, "Bus_Off Recovery"). Flags of disabled sources are left alone.
pub fn on_interrupt(instance: FdCanInstance, line: InterruptLine) {
    let regs = instance.regs();
    let state = state(instance);

    let ir = regs.ir().read().0;
    let enabled = regs.ie().read().0;
    let routed = line_sources(regs.ils().read().0, line).0;
    let flags = ir & enabled & routed;
    if flags == 0 {
        return;
    }
    // Write 1 to clear only what is handled here (RM0468 / RM0444 FDCAN_IR: rc_w1).
    regs.ir().write_value(Ir(flags));

    if flags & Interrupts::BUS_OFF.0 != 0 {
        let psr = regs.psr().read();
        state.latch_psr(psr.0);
        if psr.bo() && state.automatic_bus_off_recovery.load(Ordering::Acquire) {
            // INIT was set by the core on entering bus-off. Clearing it starts the recovery sequence
            // (129 × 11 recessive bits), after which the error counters are reset.
            regs.cccr().modify(|w| w.set_init(false));
        }
    }

    state.events.fetch_or(flags, Ordering::AcqRel);
    #[cfg(feature = "asynchronous")]
    state.waker.wake();
}

impl<M> FdCan<M> {
    /// Interrupt sources the driver has enabled (IE).
    #[inline]
    pub fn enabled_interrupts(&self) -> Interrupts {
        Interrupts::from_bits_truncate(self.can.ie().read().0)
    }

    /// Enables `interrupts` in addition to the configured ones, in any mode. They stay enabled across mode
    /// transitions. The interrupt line follows [FdCanConfig::interrupt_line_1](crate::config::FdCanConfig).
    #[inline]
    pub fn enable_interrupts(&mut self, interrupts: Interrupts) {
        self.config.interrupts = self.config.interrupts.union(interrupts);
        self.write_interrupt_enables();
    }

    /// Disables `interrupts`. Bus_Off stays enabled while automatic bus-off recovery is on.
    #[inline]
    pub fn disable_interrupts(&mut self, interrupts: Interrupts) {
        self.config.interrupts = self.config.interrupts.difference(interrupts);
        self.write_interrupt_enables();
    }

    /// Writes IE, TXBTIE, TXBCIE and ILE from the config. ILS must already be written.
    pub(crate) fn write_interrupt_enables(&mut self) {
        let mut enabled = self.config.interrupts;
        if self.config.automatic_bus_off_recovery {
            enabled = enabled.union(Interrupts::BUS_OFF);
        }
        // TC / TCF only fire for buffers enabled here; IE decides whether they're used at all.
        let all_buffers = crate::pac::variant::TX_BUFFERS_ALL;
        self.can.txbtie().write_value(Txbtie(all_buffers));
        self.can.txbcie().write_value(Txbcie(all_buffers));
        self.can.ie().write_value(Ie(enabled.0));
        let (line0, line1) = ile_bits(enabled, self.can.ils().read().0);
        let mut ile = Ile(0);
        ile.set_eint0(line0);
        ile.set_eint1(line1);
        self.can.ile().write_value(ile);
    }

    /// Writes ILS from the config. On lite cores the stored selection is widened to whole groups.
    pub(crate) fn write_interrupt_lines(&mut self) {
        let ils = ils_bits(self.config.interrupt_line_1);
        self.can.ils().write_value(Ils(ils));
        self.config.interrupt_line_1 = line1_sources(ils);
    }

    /// Interrupt flags that are set, in IR or latched by [on_interrupt], without clearing them.
    #[inline]
    pub fn interrupt_flags(&self) -> Interrupts {
        Interrupts::from_bits_truncate(self.can.ir().read().0 | self.state().peek_events())
    }

    /// Returns which of `which` were flagged since they were last taken and clears them, both in IR and in
    /// the flags latched by [on_interrupt]. Each event is reported once, whether or not the interrupt
    /// handler saw it first.
    #[inline]
    pub fn take_interrupt_flags(&mut self, which: Interrupts) -> Interrupts {
        let mask = which.intersection(Interrupts::ALL).0;
        let ir = self.can.ir().read().0 & mask;
        if ir != 0 {
            self.can.ir().write_value(Ir(ir));
        }
        Interrupts(ir | self.state().take_events(mask))
    }

    #[inline]
    pub(crate) fn state(&self) -> &'static State {
        state(self.instance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The named constants sit where the generated register accessors put the fields.
    #[test]
    fn bits_match_pac() {
        let cases: &[(Interrupts, fn(&mut Ir))] = &[
            (Interrupts::RX_FIFO0_NEW_MESSAGE, |r| r.set_rfn(0, true)),
            (Interrupts::RX_FIFO0_FULL, |r| r.set_rff(0, true)),
            (Interrupts::RX_FIFO0_MESSAGE_LOST, |r| r.set_rfl(0, true)),
            (Interrupts::RX_FIFO1_NEW_MESSAGE, |r| r.set_rfn(1, true)),
            (Interrupts::RX_FIFO1_FULL, |r| r.set_rff(1, true)),
            (Interrupts::RX_FIFO1_MESSAGE_LOST, |r| r.set_rfl(1, true)),
            (Interrupts::HIGH_PRIORITY_MESSAGE, |r| r.set_hpm(true)),
            (Interrupts::TX_COMPLETED, |r| r.set_tc(true)),
            (Interrupts::TX_CANCELLATION_FINISHED, |r| r.set_tcf(true)),
            #[cfg(feature = "h7")]
            (Interrupts::TX_FIFO_EMPTY, |r| r.set_tef(true)),
            #[cfg(not(feature = "h7"))]
            (Interrupts::TX_FIFO_EMPTY, |r| r.set_tfe(true)),
            (Interrupts::TX_EVENT_FIFO_NEW_ENTRY, |r| r.set_tefn(true)),
            (Interrupts::TX_EVENT_FIFO_FULL, |r| r.set_teff(true)),
            (Interrupts::TX_EVENT_FIFO_ELEMENT_LOST, |r| r.set_tefl(true)),
            (Interrupts::TIMESTAMP_WRAPAROUND, |r| r.set_tsw(true)),
            (Interrupts::MESSAGE_RAM_ACCESS_FAILURE, |r| r.set_mraf(true)),
            (Interrupts::TIMEOUT_OCCURRED, |r| r.set_too(true)),
            (Interrupts::ERROR_LOGGING_OVERFLOW, |r| r.set_elo(true)),
            (Interrupts::ERROR_PASSIVE, |r| r.set_ep(true)),
            (Interrupts::WARNING_STATUS, |r| r.set_ew(true)),
            (Interrupts::BUS_OFF, |r| r.set_bo(true)),
            (Interrupts::WATCHDOG, |r| r.set_wdi(true)),
            (Interrupts::PROTOCOL_ERROR_ARBITRATION, |r| r.set_pea(true)),
            (Interrupts::PROTOCOL_ERROR_DATA, |r| r.set_ped(true)),
            (Interrupts::ACCESS_TO_RESERVED_ADDRESS, |r| r.set_ara(true)),
            #[cfg(feature = "h7")]
            (Interrupts::RX_FIFO0_WATERMARK, |r| r.set_rfw(0, true)),
            #[cfg(feature = "h7")]
            (Interrupts::RX_FIFO1_WATERMARK, |r| r.set_rfw(1, true)),
            #[cfg(feature = "h7")]
            (Interrupts::TX_EVENT_FIFO_WATERMARK, |r| r.set_tefw(true)),
            #[cfg(feature = "h7")]
            (Interrupts::DEDICATED_RX_BUFFER, |r| r.set_drx(true)),
        ];
        let mut all = 0;
        for (set, f) in cases {
            let mut ir = Ir(0);
            f(&mut ir);
            assert_eq!(set.bits(), ir.0, "{set:?}");
            assert_eq!(all & ir.0, 0, "{set:?} defined twice");
            all |= ir.0;
        }
        assert_eq!(
            all,
            Interrupts::ALL.bits(),
            "ALL is the union of the named sources"
        );
    }

    #[test]
    fn set_operations() {
        let a = Interrupts::BUS_OFF | Interrupts::ERROR_PASSIVE;
        assert!(a.contains(Interrupts::BUS_OFF));
        assert!(!a.contains(Interrupts::BUS_OFF | Interrupts::TX_COMPLETED));
        assert!(a.intersects(Interrupts::ERRORS));
        assert_eq!(a.difference(Interrupts::BUS_OFF), Interrupts::ERROR_PASSIVE);
        assert_eq!(Interrupts::from_bits_truncate(u32::MAX), Interrupts::ALL);
        assert!(Interrupts::NONE.is_empty());
        assert_eq!(
            Interrupts::rx_fifo_message_lost(RxFifo::Fifo1),
            Interrupts::RX_FIFO1_MESSAGE_LOST
        );
    }

    #[cfg(feature = "h7")]
    #[test]
    fn lines_full_core() {
        // One ILS bit per source, same position as in IR (Bosch M_CAN user manual ILS).
        let line1 = Interrupts::BUS_OFF | Interrupts::RX_FIFO1_NEW_MESSAGE;
        let ils = ils_bits(line1);
        assert_eq!(ils, (1 << 25) | (1 << 4));
        let mut reg = Ils(0);
        reg.set_bol(true);
        reg.set_rfnl(1, true);
        assert_eq!(ils, reg.0);
        assert_eq!(line1_sources(ils), line1);
        assert_eq!(
            line_sources(ils, InterruptLine::Line0),
            Interrupts::ALL.difference(line1)
        );
    }

    #[cfg(not(feature = "h7"))]
    #[test]
    fn lines_lite_core() {
        // Groups and ILS bits from RM0444 FDCAN_ILS.
        let mut reg = Ils(0);
        reg.set_perr(true);
        assert_eq!(ils_bits(Interrupts::BUS_OFF), reg.0);
        assert_eq!(
            line1_sources(reg.0),
            Interrupts::BUS_OFF
                | Interrupts::WARNING_STATUS
                | Interrupts::WATCHDOG
                | Interrupts::PROTOCOL_ERROR_ARBITRATION
                | Interrupts::PROTOCOL_ERROR_DATA
                | Interrupts::ACCESS_TO_RESERVED_ADDRESS
        );
        let mut reg = Ils(0);
        reg.set_rxfifo(1, true);
        reg.set_berr(true);
        assert_eq!(
            ils_bits(Interrupts::RX_FIFO1_FULL | Interrupts::ERROR_PASSIVE),
            reg.0
        );
        let mut reg = Ils(0);
        reg.set_rxfifo(0, true);
        reg.set_smsg(true);
        reg.set_tferr(true);
        reg.set_misc(true);
        assert_eq!(
            ils_bits(
                Interrupts::RX_FIFO0_NEW_MESSAGE
                    | Interrupts::TX_COMPLETED
                    | Interrupts::TX_FIFO_EMPTY
                    | Interrupts::TIMESTAMP_WRAPAROUND
            ),
            reg.0
        );
        // The groups cover every source exactly once.
        let mut all = Interrupts::NONE;
        for g in ILS_GROUPS {
            assert!(!all.intersects(g));
            all |= g;
        }
        assert_eq!(all, Interrupts::ALL);
        assert_eq!(line1_sources(0x7F), Interrupts::ALL);
    }

    #[test]
    fn line_enables() {
        let ils = ils_bits(Interrupts::BUS_OFF);
        assert_eq!(ile_bits(Interrupts::NONE, ils), (false, false));
        assert_eq!(ile_bits(Interrupts::BUS_OFF, ils), (false, true));
        assert_eq!(
            ile_bits(Interrupts::RX_FIFO0_NEW_MESSAGE, ils),
            (true, false)
        );
        assert_eq!(
            ile_bits(Interrupts::RX_FIFO0_NEW_MESSAGE | Interrupts::BUS_OFF, ils),
            (true, true)
        );
    }

    #[test]
    fn psr_merge() {
        // LEC = Ack (3), DLEC = no change, RESI set, BO set (not reset on read).
        let older = 3 | 7 << 8 | 1 << 11;
        // LEC = no change, DLEC = CRC (6), RBRS set, EP set.
        let newer = 7 | 6 << 8 | 1 << 12 | 1 << 5;
        let m = merge_psr(newer, older);
        assert_eq!(m & 7, 3, "older LEC kept when newer says no change");
        assert_eq!(m >> 8 & 7, 6, "newer DLEC wins");
        assert_eq!(m >> 11 & 0xF, 0b11, "flags or-ed");
        assert_eq!(m & 1 << 5, 1 << 5, "status bits from newer");
        // A newer error code replaces the older one.
        assert_eq!(merge_psr(5, 3) & 7, 5);
        assert_eq!(
            merge_psr(PSR_NOTHING_LATCHED, PSR_NOTHING_LATCHED),
            PSR_NOTHING_LATCHED
        );
    }

    #[test]
    fn state_latches() {
        let s = State::new();
        s.events.fetch_or(0b1010, Ordering::AcqRel);
        assert_eq!(s.take_events(0b0010), 0b0010);
        assert_eq!(s.peek_events(), 0b1000);
        s.latch_psr(3 | 7 << 8 | 1 << 7);
        s.latch_psr(PSR_NOTHING_LATCHED);
        let psr = s.take_psr(7 | 7 << 8);
        assert_eq!(psr & 7, 3, "LEC read by an earlier PSR access is not lost");
        assert_eq!(psr & 1 << 7, 0, "only reset-on-read fields are latched");
        assert_eq!(s.take_psr(7 | 7 << 8) & 7, 7, "latch reset after take");
    }
}
