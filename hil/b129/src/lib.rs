//! B129A board support for the HIL tests: clocks, FDCAN1 bring-up and the transceiver standby pin.
//! Pinout and hardware notes: boards/b129.md.
//!
//! The g0 driver has no frame TX/RX yet (FEATURES.md R5), so these tests cover bring-up, modes, bit timing and
//! status on a real transceiver. Frame tests follow with R5 (FEATURES.md Q5a).
#![no_std]

use defmt_rtt as _;

// Cargo unifies features, so `cnt/disabled` enabled by any crate in the build compiles out the driver's counters
// too. `Counters::get` then returns 0 and "no errors" asserts would pass without checking anything
// (FEATURES.md Q9).
const _: () = assert!(!cnt::DISABLED, "HIL tests need cnt counters, don't enable cnt's `disabled` feature");

use embassy_stm32::gpio::{Level, Output, Speed};
use embassy_stm32::rcc::mux::Fdcansel;
use embassy_stm32::rcc::{Hse, HseMode, Pll, PllMul, PllPreDiv, PllRDiv, PllSource, Sysclk};
use embassy_stm32::time::Hertz;
use embassy_stm32::{Config, Peripherals};
use mcan::{
    ConfigMode, DataBitTiming, FdCan, FdCanInstance, FdCanInstances, NominalBitTiming,
};

/// FDCAN kernel clock: the 12 MHz crystal (Y2) directly, so the tests don't depend on the PLL.
pub const FDCAN_KERNEL_CLOCK_HZ: u32 = 12_000_000;

/// Clocks used by every test. Also starts the embassy time driver (TIM2).
pub fn init_clocks() -> Peripherals {
    let mut config = Config::default();
    config.rcc.hse = Some(Hse {
        freq: Hertz::mhz(12),
        mode: HseMode::Oscillator,
    });
    // CPU at 64 MHz (12 MHz / 1 * 16 / 3): the RX FIFO has only 3 elements, a slow CPU loses frames of a back to
    // back burst. The FDCAN kernel clock stays the crystal.
    config.rcc.pll = Some(Pll {
        source: PllSource::HSE,
        prediv: PllPreDiv::DIV1,
        mul: PllMul::MUL16,
        divp: None,
        divq: None,
        divr: Some(PllRDiv::DIV3),
    });
    config.rcc.sys = Sysclk::PLL1_R;
    config.rcc.mux.fdcansel = Fdcansel::HSE;
    embassy_stm32::init(config)
}

/// State handed to every test: clocks are up, the FDCAN instances untouched, the transceiver in normal mode.
pub struct Board {
    pub peripherals: Peripherals,
    pub instances: FdCanInstances,
    /// CAN_STBY (PC13). Kept alive: dropping it would float the pin and put the transceiver into standby.
    pub standby: Output<'static>,
}

impl Board {
    /// Clocks, FDCAN instances and the transceiver **out of standby** (PC13 low).
    pub fn init() -> Self {
        let peripherals = init_clocks();
        let instances = defmt::unwrap!(FdCanInstances::new());
        // SAFETY of the static lifetime: the pin is never given back, the board lives for the whole test.
        let pc13 = unsafe { peripherals.PC13.clone_unchecked() };
        let standby = Output::new(pc13, Level::Low, Speed::Low);
        Self {
            peripherals,
            instances,
            standby,
        }
    }

    /// Puts the transceiver into standby (PC13 high). Nothing is received or transmitted until [Self::wake].
    pub fn sleep(&mut self) {
        self.standby.set_high();
    }

    /// Takes the transceiver out of standby (PC13 low), then waits for it to become active.
    pub fn wake(&mut self) {
        self.standby.set_low();
        // TCAN1044V standby to normal mode takes up to 1.5 ms (t_MODE), generous margin.
        embassy_time::block_for(embassy_time::Duration::from_millis(5));
    }

    /// Connects FDCAN1 to its pins (TX PD1, RX PB8, boards/b129.md). Only needed when the core uses the pins,
    /// not in internal loopback. TX is recessive while the instance is in Config mode.
    pub fn connect_fdcan1_pins(&mut self) {
        mcan::embassy::configure_pins!(
            tx: self.peripherals.PD1.reborrow(),
            rx: self.peripherals.PB8.reborrow()
        );
    }

    /// FDCAN1 in Config mode at 1 Mbit/s nominal (83 % sample point, 12 tq: sync 1 + seg1 9 + seg2 2) and 2 Mbit/s data (12 MHz kernel clock).
    pub fn fdcan1_config(&mut self) -> FdCan<ConfigMode> {
        let can = defmt::unwrap!(self.instances.take_enabled(FdCanInstance::FdCan1));
        let mut can = defmt::unwrap!(can.into_config_mode());
        can.set_nominal_bit_timing(defmt::unwrap!(NominalBitTiming::new(1, 9, 2, 2)));
        can.set_data_bit_timing(defmt::unwrap!(DataBitTiming::new(1, 4, 1, 1)));
        can
    }
}

/// Classic CAN data frame header.
pub fn classic(id: mcan::Id) -> mcan::TxFrameHeader {
    use mcan::pac::message_ram::{Esi, FrameFormat};
    mcan::TxFrameHeader {
        frame_format: FrameFormat::Classic,
        id,
        bit_rate_switching: false,
        error_state: Esi::EsiDependsOnErrorPassive,
        marker: None,
    }
}

/// Deterministic test payload: byte `i` of frame number `n`.
pub fn pattern(n: usize, len: usize) -> [u8; 64] {
    let mut data = [0u8; 64];
    for (i, b) in data[..len].iter_mut().enumerate() {
        *b = (n * 31 + i * 7 + 1) as u8;
    }
    data
}
