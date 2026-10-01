//! B135A/B board support for the HIL tests: clocks, FDCAN1 bring-up and blocking TX/RX helpers.
//! Pinout and hardware notes: boards/b135.md.
#![no_std]

use defmt_rtt as _;
use embassy_stm32::pac::rcc::vals::{Pllm, Plln, Pllsrc};
use embassy_stm32::rcc::mux::Fdcansel;
use embassy_stm32::rcc::{
    AHBPrescaler, APBPrescaler, HseMode, Pll, PllDiv, SupplyConfig, Sysclk, VoltageScale,
};
use embassy_stm32::time::Hertz;
use embassy_stm32::{Config, rcc};
use embassy_time::{Duration, Instant};
use mcan::fdcan::Receive;
use mcan::{
    FdCan, FdCanInstances, MessageRamBuilder, RamBuilderInitialState, RxFifo, RxFrameHeader,
    TxBufferIdx, TxFrameHeader,
};

/// FDCAN kernel clock: 24 MHz HSE bypass / 12 * 128 / 4 (PLL1_Q).
pub const FDCAN_KERNEL_CLOCK_HZ: u32 = 64_000_000;

/// Clocks used by every test. Also starts the embassy time driver.
pub fn init_clocks() -> embassy_stm32::Peripherals {
    let mut config = Config::default();
    config.rcc.hse = Some(rcc::Hse {
        freq: Hertz::mhz(24),
        mode: HseMode::Bypass,
    });
    config.rcc.pll1 = Some(Pll {
        source: Pllsrc::HSE,
        prediv: Pllm::DIV12,
        mul: Plln::MUL128,
        divp: Some(PllDiv::DIV2),
        divq: Some(PllDiv::DIV4),
        divr: None,
    });
    config.rcc.voltage_scale = VoltageScale::Scale0;
    config.rcc.supply_config = SupplyConfig::DirectSMPS;
    config.rcc.sys = Sysclk::PLL1_P;
    config.rcc.ahb_pre = AHBPrescaler::DIV2;
    config.rcc.apb1_pre = APBPrescaler::DIV2;
    config.rcc.mux.fdcansel = Fdcansel::PLL1_Q;
    embassy_stm32::init(config)
}

/// State handed to every test: clocks are up, FDCAN instances and the RAM builder are untouched.
pub struct Board {
    pub peripherals: embassy_stm32::Peripherals,
    pub instances: FdCanInstances,
    /// Taken by the first layout a test builds.
    pub builder: Option<MessageRamBuilder<RamBuilderInitialState>>,
}

impl Board {
    pub fn init() -> Self {
        let peripherals = init_clocks();
        let (instances, builder) = defmt::unwrap!(FdCanInstances::new());
        Self {
            peripherals,
            instances,
            builder: Some(builder),
        }
    }
}

/// Generous upper bound for one frame at the bit rates used here.
pub const FRAME_TIMEOUT: Duration = Duration::from_millis(10);

/// Writes a dedicated TX buffer, requests transmission and waits until nothing is pending any more.
pub fn send<M: mcan::fdcan::Transmit>(
    can: &mut FdCan<M>,
    idx: TxBufferIdx,
    header: TxFrameHeader,
    data: &[u8],
) {
    defmt::unwrap!(can.write_tx_buffer_pend(idx, header, data));
    let deadline = Instant::now() + FRAME_TIMEOUT;
    while !can.is_idle() {
        defmt::assert!(Instant::now() < deadline, "TX did not complete");
    }
}

/// Waits for a frame in `fifo` and returns its header and the number of bytes copied into `buf`.
pub fn receive<M: Receive>(
    can: &mut FdCan<M>,
    fifo: RxFifo,
    buf: &mut [u8],
) -> (RxFrameHeader, usize) {
    let deadline = Instant::now() + FRAME_TIMEOUT;
    loop {
        if let Some(r) = defmt::unwrap!(can.receive_fifo(fifo, buf)) {
            return r;
        }
        defmt::assert!(Instant::now() < deadline, "no frame received");
    }
}

/// Test payload: distinct per frame number and byte position.
pub fn pattern(frame: usize, len: usize) -> [u8; 64] {
    core::array::from_fn(|i| {
        if i < len {
            (frame * 31 + i * 7) as u8
        } else {
            0
        }
    })
}
