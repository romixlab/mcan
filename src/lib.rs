#![no_std]

use crate::config::{
    DataBitTiming, FdCanConfig, FrameTransmissionConfig, GlobalFilter, NominalBitTiming,
    TimestampSource,
};
use crate::pac::registers::regs::Ir;
use crate::pac::{
    FDCAN_MSGRAM, FDCAN_MSGRAM_LEN_WORDS, FDCAN1_REGISTER_BLOCK_ADDR, RCC_REGISTER_BLOCK_ADDR,
};
use core::marker::PhantomData;
use static_cell::StaticCell;

pub mod common;
pub mod config;
pub mod message_ram;
pub use message_ram::{
    DataFieldSize, ElevenBitFilters, MessageRamBuilder, MessageRamBuilderError, MessageRamLayout,
};
pub mod pac;

/// Allows for Transmit Operations
pub trait Transmit {}
/// Allows for Receive Operations
pub trait Receive {}

pub struct PoweredDownMode;
pub struct ConfigMode;

pub struct InternalLoopbackMode;
impl Transmit for PoweredDownMode {}
impl Receive for PoweredDownMode {}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    PeripheralTaken,
    ClockSourceIsDisabled,
    CoreCommunicationFailed,
    UnsupportedCoreVersion,
    Timeout,
}

// we must wait two peripheral clock cycles before the clock is active
// http://efton.sk/STM32/gotcha/g183.html
const CLOCK_DOMAIN_SYNCHRONIZATION_DELAY: u32 = 100;

#[inline]
fn checked_wait<F: Fn() -> bool>(f: F, timeout_iterations: u32) -> Result<(), Error> {
    let mut elapsed = 0;
    while f() {
        elapsed += 1;
        if elapsed >= timeout_iterations {
            return Err(Error::Timeout);
        }
    }
    Ok(())
}

pub struct FdCan<M> {
    regs: pac::registers::Fdcan,
    state: &'static mut State,
    config: FdCanConfig,
    _mode: PhantomData<M>,
}

struct State {}

impl State {
    fn new() -> Self {
        State {}
    }
}
static STATE_FDCAN1: StaticCell<State> = StaticCell::new();

enum LoopbackMode {
    None,
    Internal,
    External,
}

/// Create FDCAN1 instance. This method can be called only once, otherwise Error::PeripheralTake is returned.
/// Peripheral is disabled via RCC.
#[cfg(feature = "g0")]
pub fn new_fdcan1() -> Result<FdCan<PoweredDownMode>, Error> {
    let state = STATE_FDCAN1
        .try_init(State::new())
        .ok_or(Error::PeripheralTaken)?;
    // TODO: init GPIOs here?
    let rcc = unsafe { pac::rcc_g0::Rcc::from_ptr(RCC_REGISTER_BLOCK_ADDR) };
    rcc.apbenr1().modify(|w| w.set_fdcanen(false));

    let regs = unsafe { pac::registers::Fdcan::from_ptr(FDCAN1_REGISTER_BLOCK_ADDR) };
    Ok(FdCan {
        regs,
        state,
        config: FdCanConfig::default(),
        _mode: PhantomData,
    })
}

impl<M> FdCan<M> {
    #[inline]
    fn check_core(&self) -> Result<(), Error> {
        if self.regs.endn().read().0 != 0x87654321_u32 {
            return Err(Error::CoreCommunicationFailed);
        }
        if self.regs.crel().read().rel() != 3 {
            return Err(Error::UnsupportedCoreVersion);
        }
        Ok(())
    }

    #[inline]
    fn set_power_down_mode(&mut self, enabled: bool) -> Result<(), Error> {
        // Clock stop requested. When clock stop is requested, first INIT and then CSA will be set after
        // all pending transfer requests have been completed and the CAN bus reached idle.
        self.regs.cccr().modify(|w| w.set_csr(enabled));
        checked_wait(
            || self.regs.cccr().read().csa() != enabled,
            self.config.power_down_timeout_iterations,
        )?;
        Ok(())
    }

    #[inline]
    fn enter_init_mode(&mut self) -> Result<(), Error> {
        // Due to the synchronization mechanism between the two clock domains, there may be a
        // delay until the value written to INIT can be read back. Therefore, the programmer has to
        // ensure that the previous value written to INIT has been accepted by reading INIT before
        // setting INIT to a new value.
        self.regs.cccr().modify(|w| w.set_init(true));
        checked_wait(
            || !self.regs.cccr().read().init(),
            self.config.timeout_iterations,
        )?;
        // 1 = The CPU has write access to the protected configuration registers (while CCCR.INIT = ‘1’)
        self.regs.cccr().modify(|w| w.set_cce(true));
        Ok(())
    }

    #[inline]
    fn zero_msg_ram(&mut self) {
        // In case the Message RAM is equipped with parity or ECC functionality, it is recommended
        // to initialize the Message RAM after hardware reset by writing e.g., 0x00000000 to each
        // Message RAM word to create valid parity/ECC checksums. This avoids that reading from
        // uninitialized Message RAM sections will activate interrupt IR.BEC (Bit Error Corrected)
        // or IR.BEU (Bit Error Uncorrected)
        for i in 0..FDCAN_MSGRAM_LEN_WORDS {
            unsafe {
                let ptr = FDCAN_MSGRAM.add(i);
                core::ptr::write_volatile(ptr, 0x0000_0000);
            }
        }
    }

    /// Enables or disables loopback mode: Internally connects the TX and RX signals.
    /// External loopback also drives TX pin.
    /// Only use external loopback for production tests, as it will destroy ongoing external bus traffic.
    #[inline]
    fn set_loopback_mode(&mut self, mode: LoopbackMode) {
        let (test, mon, lbck) = match mode {
            LoopbackMode::None => (false, false, false),
            LoopbackMode::Internal => (true, true, true),
            LoopbackMode::External => (true, false, true),
        };

        self.set_test_mode(test);
        self.set_bus_monitoring_mode(mon);

        self.regs.test().modify(|w| w.set_lbck(lbck));
    }

    /// Enables or disables silent mode: Disconnects the TX signal from the pin.
    #[inline]
    fn set_bus_monitoring_mode(&mut self, enabled: bool) {
        self.regs.cccr().modify(|w| w.set_mon(enabled));
    }

    #[inline]
    fn set_restricted_operations(&mut self, enabled: bool) {
        self.regs.cccr().modify(|w| w.set_asm(enabled));
    }

    #[inline]
    fn set_normal_operations(&mut self, _enabled: bool) {
        self.set_loopback_mode(LoopbackMode::None);
    }

    #[inline]
    fn set_test_mode(&mut self, enabled: bool) {
        self.regs.cccr().modify(|w| w.set_test(enabled));
    }

    fn into_mode<M2>(self) -> FdCan<M2> {
        FdCan {
            regs: self.regs,
            state: self.state,
            config: self.config,
            _mode: Default::default(),
        }
    }
}

impl FdCan<PoweredDownMode> {
    /// Enable peripheral clock, reset and enable configuration mode
    #[inline]
    pub fn into_config_mode(
        mut self,
    ) -> Result<FdCan<ConfigMode>, (Error, FdCan<PoweredDownMode>)> {
        if let Err(e) = self.try_config_mode() {
            return Err((e, self));
        }

        Ok(self.into_mode())
    }

    #[inline]
    fn try_config_mode(&mut self) -> Result<(), Error> {
        self.enable_reset()?;
        self.check_core()?;
        self.set_power_down_mode(false)?;
        self.enter_init_mode()?;
        self.zero_msg_ram();
        Ok(())
    }

    #[cfg(feature = "g0")]
    #[inline]
    fn enable_reset(&mut self) -> Result<(), Error> {
        let rcc = unsafe { pac::rcc_g0::Rcc::from_ptr(RCC_REGISTER_BLOCK_ADDR) };
        #[cfg(feature = "defmt")]
        defmt::debug!("FDCAN1 clock source: {}", rcc.ccipr2().read().fdcansel());

        use crate::pac::rcc_g0::vals::Fdcansel;
        match rcc.ccipr2().read().fdcansel() {
            Fdcansel::PCLK1 => {}
            Fdcansel::PLL1_Q => {
                if !rcc.pllcfgr().read().pllqen() {
                    return Err(Error::ClockSourceIsDisabled);
                }
            }
            Fdcansel::HSE => {
                if !rcc.cr().read().hseon() {
                    return Err(Error::ClockSourceIsDisabled);
                }
            }
            Fdcansel::_RESERVED_3 => {
                return Err(Error::ClockSourceIsDisabled);
            }
        }

        rcc.apbrstr1().modify(|w| w.set_fdcanrst(true));
        rcc.apbenr1().modify(|w| w.set_fdcanen(true));
        cortex_m::asm::delay(CLOCK_DOMAIN_SYNCHRONIZATION_DELAY);
        // DSB for good measure
        cortex_m::asm::dsb();
        rcc.apbrstr1().modify(|w| w.set_fdcanrst(false));

        Ok(())
    }
}

impl FdCan<ConfigMode> {
    #[inline]
    pub fn into_internal_loopback(
        mut self,
    ) -> Result<FdCan<InternalLoopbackMode>, (Error, FdCan<ConfigMode>)> {
        self.set_loopback_mode(LoopbackMode::Internal);
        if let Err(e) = self.leave_init_mode() {
            return Err((e, self));
        }
        Ok(self.into_mode())
    }

    #[inline]
    fn leave_init_mode(&mut self) -> Result<(), Error> {
        self.apply_config(self.config);

        self.regs.cccr().modify(|w| w.set_cce(false));
        self.regs.cccr().modify(|w| w.set_init(false));
        checked_wait(
            || self.regs.cccr().read().init(),
            self.config.timeout_iterations,
        )?;
        Ok(())
    }

    /// Applies the settings of a new FdCanConfig See [`FdCanConfig`]
    #[inline]
    pub fn apply_config(&mut self, config: FdCanConfig) {
        self.set_data_bit_timing(config.dbtr);
        self.set_nominal_bit_timing(config.nbtr);
        self.set_automatic_retransmit(config.automatic_retransmit);
        self.set_transmit_pause(config.transmit_pause);
        self.set_frame_transmit(config.frame_transmit);
        self.select_interrupt_line_1(config.interrupt_line_config);
        self.set_non_iso_mode(config.non_iso_mode);
        self.set_edge_filtering(config.edge_filtering);
        self.set_protocol_exception_handling(config.protocol_exception_handling);
        self.set_global_filter(config.global_filter);
        self.set_layout(config.layout);
    }

    /// Configures the bit timings.
    ///
    /// You can use <http://www.bittiming.can-wiki.info/> to calculate the `btr` parameter. Enter
    /// parameters as follows:
    ///
    /// - *Clock Rate*: The input clock speed to the CAN peripheral (*not* the CPU clock speed).
    ///   This is the clock rate of the peripheral bus the CAN peripheral is attached to (e.g., APB1).
    /// - *Sample Point*: Should normally be left at the default value of 87.5%.
    /// - *SJW*: Should normally be left at the default value of 1.
    ///
    /// Then copy the `CAN_BUS_TIME` register value from the table and pass it as the `btr`
    /// parameter to this method.
    #[inline]
    pub fn set_nominal_bit_timing(&mut self, btr: NominalBitTiming) {
        self.config.nbtr = btr;

        self.regs.nbtp().write(|w| {
            w.set_nbrp(btr.nbrp() - 1);
            w.set_ntseg1(btr.ntseg1() - 1);
            w.set_ntseg2(btr.ntseg2() - 1);
            w.set_nsjw(btr.nsjw() - 1);
        });
    }

    /// Configures the data bit timings for the FdCan Variable Bitrates.
    /// This is not used when frame_transmit is set to anything other than AllowFdCanAndBRS.
    #[inline]
    pub fn set_data_bit_timing(&mut self, btr: DataBitTiming) {
        self.config.dbtr = btr;

        self.regs.dbtp().write(|w| {
            w.set_dbrp(btr.dbrp() - 1);
            w.set_dtseg1(btr.dtseg1() - 1);
            w.set_dtseg2(btr.dtseg2() - 1);
            w.set_dsjw(btr.dsjw() - 1);
        });
    }

    /// Enables or disables automatic retransmission of messages
    ///
    /// If this is enabled, the CAN peripheral will automatically try to retransmit each frame
    /// util it can be sent. Otherwise, it will try only once to send each frame.
    ///
    /// Automatic retransmission is enabled by default.
    #[inline]
    pub fn set_automatic_retransmit(&mut self, enabled: bool) {
        self.regs.cccr().modify(|w| w.set_dar(!enabled));
        self.config.automatic_retransmit = enabled;
    }

    /// Configures the transmit pause feature. See
    /// [`FdCanConfig::set_transmit_pause`]
    #[inline]
    pub fn set_transmit_pause(&mut self, enabled: bool) {
        self.regs.cccr().modify(|w| w.set_txp(enabled));
        self.config.transmit_pause = enabled;
    }

    /// Configures non-iso mode. See [`FdCanConfig::set_non_iso_mode`]
    #[inline]
    pub fn set_non_iso_mode(&mut self, enabled: bool) {
        self.regs.cccr().modify(|w| w.set_niso(enabled));
        self.config.non_iso_mode = enabled;
    }

    /// Configures edge filtering. See [`FdCanConfig::set_edge_filtering`]
    #[inline]
    pub fn set_edge_filtering(&mut self, enabled: bool) {
        self.regs.cccr().modify(|w| w.set_efbi(enabled));
        self.config.edge_filtering = enabled;
    }

    /// Configures frame transmission mode. See
    /// [`FdCanConfig::set_frame_transmit`]
    #[inline]
    pub fn set_frame_transmit(&mut self, fts: FrameTransmissionConfig) {
        let (fdoe, brse) = match fts {
            FrameTransmissionConfig::ClassicCanOnly => (false, false),
            FrameTransmissionConfig::AllowFdCan => (true, false),
            FrameTransmissionConfig::AllowFdCanAndBRS => (true, true),
        };

        self.regs.cccr().modify(|w| {
            w.set_fdoe(fdoe);
            w.set_bse(brse);
        });

        self.config.frame_transmit = fts;
    }

    /// Selects Interrupt Line 1 for the given interrupts. Interrupt Line 0 is
    /// selected for all other interrupts. See
    /// [`FdCanConfig::select_interrupt_line_1`]
    pub fn select_interrupt_line_1(&mut self, l1int: Ir) {
        self.regs.ils().modify(|w| w.0 = l1int.0);

        self.config.interrupt_line_config = l1int;
    }

    /// Sets the protocol exception handling on/off
    #[inline]
    pub fn set_protocol_exception_handling(&mut self, enabled: bool) {
        self.regs.cccr().modify(|w| w.set_pxhd(!enabled));

        self.config.protocol_exception_handling = enabled;
    }

    /// Configures and resets the timestamp counter
    #[inline]
    pub fn set_timestamp_counter_source(&mut self, select: TimestampSource) {
        let (tcp, tss) = match select {
            TimestampSource::None => (0, 0b00),
            TimestampSource::Prescaler(p) => (p as u8, 0b01),
            TimestampSource::FromTIM3 => (0, 0b10),
        };
        self.regs.tscc().write(|w| {
            w.set_tcp(tcp);
            w.set_tss(tss);
        });

        self.config.timestamp_source = select;
    }

    /// Configures the global filter settings
    #[inline]
    pub fn set_global_filter(&mut self, filter: GlobalFilter) {
        self.regs.gfc().modify(|w| {
            w.set_anfs(filter.handle_standard_frames as u8);
            w.set_anfe(filter.handle_extended_frames as u8);
            w.set_rrfs(filter.reject_remote_standard_frames);
            w.set_rrfe(filter.reject_remote_extended_frames);
        });
    }

    /// Configures RAM layout for this instance
    #[inline]
    pub fn set_layout(&mut self, layout: MessageRamLayout) {
        self.config.layout = layout;
        self.regs.sidfc().modify(|w| {
            w.set_flssa(layout.eleven_bit_filters_addr);
            w.set_lss(layout.eleven_bit_filters_len);
        });
        self.regs.xidfc().modify(|w| {
            w.set_flesa(layout.twenty_nine_bit_filters_addr);
            w.set_lse(layout.twenty_nine_bit_filters_len);
        });
        self.regs.rxfc(0).modify(|w| {
            w.set_fsa(layout.rx_fifo0_addr);
            w.set_fs(layout.rx_fifo0_len);
        });
        self.regs.rxfc(1).modify(|w| {
            w.set_fsa(layout.rx_fifo1_addr);
            w.set_fs(layout.rx_fifo1_len);
        });
        self.regs.rxbc().modify(|w| {
            w.set_rbsa(layout.rx_buffers_addr);
        });
        self.regs.rxesc().modify(|w| {
            w.set_fds(0, layout.rx_fifo0_data_size.config_register());
            w.set_fds(1, layout.rx_fifo1_data_size.config_register());
        });
        self.regs.txefc().modify(|w| {
            w.set_efsa(layout.tx_event_fifo_addr);
            w.set_efs(layout.tx_event_fifo_len);
        });
        self.regs.txbc().modify(|w| {
            w.set_tbsa(layout.tx_buffers_addr);
            w.set_tfqs(layout.tx_fifo_or_queue_len);
            w.set_ndtb(layout.tx_buffers_len);
        });
        self.regs
            .txesc()
            .modify(|w| w.set_tbds(layout.tx_buffers_data_size.config_register()));
        #[cfg(feature = "h7")]
        self.regs.tttmc().modify(|w| {
            w.set_tmsa(layout.trigger_memory_addr);
            w.set_tme(layout.trigger_memory_len);
        });
    }
}
