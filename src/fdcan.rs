use crate::config::FdCanConfig;
use crate::message_ram_builder::message_ram_builder;
use crate::pac::{
    FDCAN_MSGRAM_ADDR, FDCAN_MSGRAM_LEN_WORDS, FDCAN1_REGISTER_BLOCK_ADDR,
    FDCAN2_REGISTER_BLOCK_ADDR, RCC_REGISTER_BLOCK_ADDR,
};
use crate::{CLOCK_DOMAIN_SYNCHRONIZATION_DELAY, MessageRamBuilder, RamBuilderInitialState, pac};
use core::marker::PhantomData;

pub struct FdCan<M> {
    pub(crate) can: pac::registers::Fdcan,
    pub(crate) instance: FdCanInstance,
    #[cfg(feature = "embassy")]
    pub(crate) state: &'static mut crate::embassy::State,
    pub(crate) config: FdCanConfig,
    pub(crate) _mode: PhantomData<M>,
}

/// Allows for Transmit Operations
pub trait Transmit {}

/// Allows for Receive Operations
pub trait Receive {}

/// Allows for the FdCan Instance to enter ConfigMode or for it's clock to be disabled.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct PoweredDownMode;

/// Allows for the configuration for the Instance
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ConfigMode;

/// This mode can be used for a “Hot Selftest” meaning the FDCAN can be tested without
/// affecting a running CAN system connected to the FDCAN_TX and FDCAN_RX pins. In this
/// mode, FDCAN_RX pin is disconnected from the FDCAN and FDCAN_TX pin is held
/// recessive.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct InternalLoopbackMode;
impl Transmit for InternalLoopbackMode {}
impl Receive for InternalLoopbackMode {}

/// This mode is provided for hardware self-test. To be independent of external stimulation,
/// the FDCAN ignores acknowledge errors (recessive bit sampled in the acknowledgement slot of a
/// data / remote frame) in Loop Back mode. In this mode, the FDCAN performs internal
/// feedback from its transmitted output to its receiver input. The actual value of the FDCAN_RX
/// input pin is disregarded by the FDCAN. The transmitted messages can be monitored at the
/// FDCAN_TX transmit pin.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ExternalLoopbackMode;
impl Transmit for ExternalLoopbackMode {}
impl Receive for ExternalLoopbackMode {}

/// The normal use of the FdCan instance after configurations
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct NormalOperationMode;
impl Transmit for NormalOperationMode {}
impl Receive for NormalOperationMode {}

/// In Restricted operation mode, the node is able to receive data and remote frames and to give
/// acknowledgement to valid frames, but it does not send data frames, remote frames, active error
/// frames, or overload frames. In case of an error condition or overload condition, it does not
/// send dominant bits, instead it waits for the occurrence of bus idle condition to resynchronize
/// itself to the CAN communication. The error counters for transmitting and receive are frozen while
/// error logging (can_errors) is active.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct RestrictedOperationMode;
impl Receive for RestrictedOperationMode {}

///  In Bus monitoring mode (for more details refer to ISO11898-1, 10.12 Bus monitoring),
/// the FDCAN is able to receive valid data frames and valid remote frames, but cannot start a
/// transmission. In this mode, it sends only recessive bits on the CAN bus. If the FDCAN is
/// required to send a dominant bit (ACK bit, overload flag, active error flag), the bit is
/// rerouted internally so that the FDCAN can monitor it, even if the CAN bus remains in recessive
/// state. In Bus monitoring mode, the TXBRP register is held in reset state. The Bus monitoring
/// mode can be used to analyze the traffic on a CAN bus without affecting it by the transmission
/// of dominant bits.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct BusMonitoringMode;
impl Receive for BusMonitoringMode {}

/// Test mode must be used for production tests or self-test only. The software control for
/// FDCAN_TX pin interferes with all CAN protocol functions. It is not recommended to use test
/// modes for application.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct TestMode;

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Error {
    PeripheralTaken,
    ClockSourceIsDisabled,
    CoreCommunicationFailed,
    UnsupportedCoreVersion,
    Timeout,
    /// Not all instances where returned to FdCanInstances prior to clock disabling (one clock feeds all instances).
    MissingInstance,
    /// [put_back](FdCanInstances::put_back) called with the same [FdCanInstance](FdCanInstance) twice.
    /// or tried to use TxBufferIdx from one CAN instance with another.
    WrongInstance,
    TxBufferIndexOutOfRange,
    WrongDataSize,
}

pub(crate) enum LoopbackMode {
    None,
    Internal,
    External,
}

/// All FDCAN instances and an entry point for this driver.
/// Clock, enable and reset are the same for all of them, so it's only possible to enable or disable if all instances are present.
pub struct FdCanInstances {
    fdcan1: Option<FdCan<PoweredDownMode>>,
    // TODO: make second/third channel optional to conserve memory?
    fdcan2: Option<FdCan<PoweredDownMode>>,
    #[cfg(feature = "h7")]
    fdcan3: Option<FdCan<PoweredDownMode>>,

    #[cfg(feature = "g0")]
    rcc: pac::rcc_g0::Rcc,
    #[cfg(feature = "h7")]
    rcc: pac::rcc_h7::Rcc,
}

/// FDCAN instance number as an enum
#[derive(Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum FdCanInstance {
    FdCan1,
    FdCan2,
    #[cfg(feature = "h7")]
    FdCan3,
}

impl FdCanInstances {
    /// Creates FDCAN instances in powered down state (enable a flag is cleared in RCC as well).
    /// This method can be called only once, otherwise Error::PeripheralTaken is returned.
    pub fn new() -> Result<(Self, MessageRamBuilder<RamBuilderInitialState>), Error> {
        #[cfg(feature = "embassy")]
        let fdcan1_state = crate::embassy::state_fdcan1()?;
        #[cfg(feature = "embassy")]
        let fdcan2_state = crate::embassy::state_fdcan2()?;
        #[cfg(all(feature = "embassy", feature = "h7"))]
        let fdcan3_state = crate::embassy::state_fdcan3()?;

        let ram_builder = message_ram_builder().map_err(|_| Error::PeripheralTaken)?;

        let mut s = Self::empty();

        #[cfg(feature = "g0")]
        s.rcc.apbenr1().modify(|w| w.set_fdcanen(false));

        #[cfg(feature = "h7")]
        s.rcc.apb1henr().modify(|w| w.set_fdcanen(false));

        let fdcan1_regs = unsafe { pac::registers::Fdcan::from_ptr(FDCAN1_REGISTER_BLOCK_ADDR) };
        let fdcan2_regs = unsafe { pac::registers::Fdcan::from_ptr(FDCAN2_REGISTER_BLOCK_ADDR) };
        #[cfg(feature = "h7")]
        let fdcan3_regs =
            unsafe { pac::registers::Fdcan::from_ptr(pac::FDCAN3_REGISTER_BLOCK_ADDR) };

        let fdcan1 = FdCan {
            can: fdcan1_regs,
            instance: FdCanInstance::FdCan1,
            #[cfg(feature = "embassy")]
            state: fdcan1_state,
            config: FdCanConfig::default(),
            _mode: PhantomData,
        };
        let fdcan2 = FdCan {
            can: fdcan2_regs,
            instance: FdCanInstance::FdCan2,
            #[cfg(feature = "embassy")]
            state: fdcan2_state,
            config: FdCanConfig::default(),
            _mode: PhantomData,
        };
        #[cfg(feature = "h7")]
        let fdcan3 = FdCan {
            can: fdcan3_regs,
            instance: FdCanInstance::FdCan3,
            #[cfg(feature = "embassy")]
            state: fdcan3_state,
            config: FdCanConfig::default(),
            _mode: PhantomData,
        };
        s.fdcan1 = Some(fdcan1);
        s.fdcan2 = Some(fdcan2);
        #[cfg(feature = "h7")]
        {
            s.fdcan3 = Some(fdcan3);
        }
        Ok((s, ram_builder))
    }

    /// There is no need to keep FdCanInstances around if all instances were taken from it, but if clocks need to be disabled, then
    /// this method can be used to re-create it.
    pub fn empty() -> Self {
        #[cfg(feature = "g0")]
        let rcc = unsafe { pac::rcc_g0::Rcc::from_ptr(RCC_REGISTER_BLOCK_ADDR) };
        #[cfg(feature = "h7")]
        let rcc = unsafe { pac::rcc_h7::Rcc::from_ptr(RCC_REGISTER_BLOCK_ADDR) };

        Self {
            fdcan1: None,
            fdcan2: None,
            #[cfg(feature = "h7")]
            fdcan3: None,
            rcc,
        }
    }

    /// Enable clock and reset all FDCAN instances if not already and take the requested instance out of this struct.
    pub fn take_enabled(
        &mut self,
        instance: FdCanInstance,
    ) -> Result<FdCan<PoweredDownMode>, Error> {
        #[cfg(feature = "g0")]
        let is_enabled = self.rcc.apbenr1().read().fdcanen();
        #[cfg(feature = "h7")]
        let is_enabled = self.rcc.apb1henr().read().fdcanen();

        if !is_enabled {
            self.enable_reset()?;
        }

        match instance {
            FdCanInstance::FdCan1 => self.fdcan1.take().ok_or(Error::PeripheralTaken),
            FdCanInstance::FdCan2 => self.fdcan2.take().ok_or(Error::PeripheralTaken),
            #[cfg(feature = "h7")]
            FdCanInstance::FdCan3 => self.fdcan3.take().ok_or(Error::PeripheralTaken),
        }
    }

    /// Disable clock for all instances if they are all present, otherwise return MissingInstances error.
    pub fn disable(&mut self) -> Result<(), Error> {
        #[cfg(feature = "h7")]
        let all_present = self.fdcan1.is_some() && self.fdcan2.is_some() && self.fdcan3.is_some();
        #[cfg(feature = "g0")]
        let all_present = self.fdcan1.is_some() && self.fdcan2.is_some();
        if !all_present {
            return Err(Error::MissingInstance);
        }

        #[cfg(feature = "h7")]
        self.rcc.apb1henr().modify(|w| w.set_fdcanen(false));
        #[cfg(feature = "g0")]
        self.rcc.apbenr1().modify(|w| w.set_fdcanen(false));

        Ok(())
    }

    /// Put back an instance in [PoweredDownMode](PoweredDownMode), if it is not in this state yet, call into_powered_down_mode() and wait for it to finish.
    pub fn put_back(
        &mut self,
        fdcan: FdCan<PoweredDownMode>,
        instance: FdCanInstance,
    ) -> Result<(), Error> {
        match instance {
            FdCanInstance::FdCan1 => {
                if self.fdcan1.is_some() {
                    return Err(Error::WrongInstance);
                }
                self.fdcan1 = Some(fdcan);
            }
            FdCanInstance::FdCan2 => {
                if self.fdcan2.is_some() {
                    return Err(Error::WrongInstance);
                }
                self.fdcan2 = Some(fdcan);
            }
            #[cfg(feature = "h7")]
            FdCanInstance::FdCan3 => {
                if self.fdcan3.is_some() {
                    return Err(Error::WrongInstance);
                }
                self.fdcan3 = Some(fdcan);
            }
        }
        Ok(())
    }

    #[cfg(feature = "g0")]
    #[inline]
    fn enable_reset(&mut self) -> Result<(), Error> {
        if self.fdcan1.is_none() || self.fdcan2.is_none() {
            return Err(Error::MissingInstance);
        }

        #[cfg(feature = "defmt")]
        defmt::debug!(
            "FDCAN clock source: {}",
            self.rcc.ccipr2().read().fdcansel()
        );

        use crate::pac::rcc_g0::vals::Fdcansel;
        match self.rcc.ccipr2().read().fdcansel() {
            Fdcansel::PCLK1 => {}
            Fdcansel::PLL1_Q => {
                if !self.rcc.pllcfgr().read().pllqen() {
                    return Err(Error::ClockSourceIsDisabled);
                }
            }
            Fdcansel::HSE => {
                if !self.rcc.cr().read().hseon() {
                    return Err(Error::ClockSourceIsDisabled);
                }
            }
            Fdcansel::_RESERVED_3 => {
                return Err(Error::ClockSourceIsDisabled);
            }
        }

        self.rcc.apbrstr1().modify(|w| w.set_fdcanrst(true));
        self.rcc.apbenr1().modify(|w| w.set_fdcanen(true));
        cortex_m::asm::delay(CLOCK_DOMAIN_SYNCHRONIZATION_DELAY);
        // DSB for good measure
        cortex_m::asm::dsb();
        self.rcc.apbrstr1().modify(|w| w.set_fdcanrst(false));

        Ok(())
    }

    #[cfg(feature = "h7")]
    #[inline]
    fn enable_reset(&mut self) -> Result<(), Error> {
        if self.fdcan1.is_none() || self.fdcan2.is_none() || self.fdcan3.is_none() {
            return Err(Error::MissingInstance);
        }

        #[cfg(feature = "defmt")]
        defmt::debug!(
            "FDCAN clock source: {}",
            self.rcc.d2ccip1r().read().fdcansel()
        );

        use crate::pac::rcc_h7::vals::Fdcansel;
        match self.rcc.d2ccip1r().read().fdcansel() {
            Fdcansel::HSE => {
                if !self.rcc.cr().read().hseon() {
                    return Err(Error::ClockSourceIsDisabled);
                }
            }
            Fdcansel::PLL1_Q => {
                if !self.rcc.pllcfgr().read().divqen(0) {
                    return Err(Error::ClockSourceIsDisabled);
                }
            }
            Fdcansel::PLL2_Q => {
                if !self.rcc.pllcfgr().read().divqen(1) {
                    return Err(Error::ClockSourceIsDisabled);
                }
            }
            Fdcansel::_RESERVED_3 => {
                return Err(Error::ClockSourceIsDisabled);
            }
        }

        self.rcc.apb1hrstr().modify(|w| w.set_fdcanrst(true));
        self.rcc.apb1henr().modify(|w| w.set_fdcanen(true));
        cortex_m::asm::delay(CLOCK_DOMAIN_SYNCHRONIZATION_DELAY);
        // DSB for good measure
        cortex_m::asm::dsb();
        self.rcc.apb1hrstr().modify(|w| w.set_fdcanrst(false));

        Ok(())
    }
}

impl<M> FdCan<M> {
    #[inline]
    fn check_core(&self) -> Result<(), Error> {
        if self.can.endn().read().0 != 0x87654321_u32 {
            return Err(Error::CoreCommunicationFailed);
        }
        if self.can.crel().read().rel() != 3 {
            return Err(Error::UnsupportedCoreVersion);
        }
        Ok(())
    }

    // TODO: make async version that can await for power down mode
    #[inline]
    pub(crate) fn set_power_down_mode(&mut self, enabled: bool) -> Result<(), Error> {
        // Clock stop requested. When clock stop is requested, first INIT and then CSA will be set after
        // all pending transfer requests have been completed and the CAN bus reached idle.
        self.can.cccr().modify(|w| w.set_csr(enabled));
        crate::util::checked_wait(
            || self.can.cccr().read().csa() != enabled,
            self.config.timeout_iterations_long,
        )?;
        Ok(())
    }

    #[inline]
    fn enter_init_mode(&mut self) -> Result<(), Error> {
        // Due to the synchronization mechanism between the two clock domains, there may be a
        // delay until the value written to INIT can be read back. Therefore, the programmer has to
        // ensure that the previous value written to INIT has been accepted by reading INIT before
        // setting INIT to a new value.
        self.can.cccr().modify(|w| w.set_init(true));
        crate::util::checked_wait(
            || !self.can.cccr().read().init(),
            self.config.timeout_iterations_short,
        )?;
        // 1 = The CPU has write access to the protected configuration registers (while CCCR.INIT = ‘1’)
        self.can.cccr().modify(|w| w.set_cce(true));
        Ok(())
    }

    #[inline]
    fn zero_msg_ram(&mut self) {
        // In case the Message RAM is equipped with parity or ECC functionality, it is recommended
        // to initialize the Message RAM after hardware reset by writing e.g., 0x00000000 to each
        // Message RAM word to create valid parity/ECC checksums. This avoids it that reading from
        // uninitialized Message RAM sections will activate interrupt IR.BEC (Bit Error Corrected)
        // or IR.BEU (Bit Error Uncorrected)
        for i in 0..FDCAN_MSGRAM_LEN_WORDS {
            unsafe {
                let ptr = FDCAN_MSGRAM_ADDR.add(i);
                core::ptr::write_volatile(ptr, 0x0000_0000);
            }
        }
    }

    /// Enables or disables loopback mode: Internally connects the TX and RX signals.
    /// External loopback also drives TX pin.
    /// Only use external loopback for production tests, as it will destroy ongoing external bus traffic.
    #[inline]
    pub(crate) fn set_loopback_mode(&mut self, mode: LoopbackMode) {
        let (test, mon, lbck) = match mode {
            LoopbackMode::None => (false, false, false),
            LoopbackMode::Internal => (true, true, true),
            LoopbackMode::External => (true, false, true),
        };

        self.set_test_mode(test);
        self.set_bus_monitoring_mode(mon);

        self.can.test().modify(|w| w.set_lbck(lbck));
    }

    /// Enables or disables silent mode: Disconnects the TX signal from the pin.
    #[inline]
    pub(crate) fn set_bus_monitoring_mode(&mut self, enabled: bool) {
        self.can.cccr().modify(|w| w.set_mon(enabled));
    }

    #[inline]
    pub(crate) fn set_restricted_operations(&mut self, enabled: bool) {
        self.can.cccr().modify(|w| w.set_asm(enabled));
    }

    #[inline]
    pub(crate) fn set_normal_operations(&mut self, _enabled: bool) {
        self.set_loopback_mode(LoopbackMode::None);
    }

    #[inline]
    pub(crate) fn set_test_mode(&mut self, enabled: bool) {
        self.can.cccr().modify(|w| w.set_test(enabled));
    }

    pub(crate) fn into_mode<M2>(self) -> FdCan<M2> {
        FdCan {
            can: self.can,
            instance: self.instance,
            #[cfg(feature = "embassy")]
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
        self.check_core()?;
        self.set_power_down_mode(false)?;
        self.enter_init_mode()?;
        self.zero_msg_ram();
        Ok(())
    }
}

#[cfg(feature = "defmt")]
impl<M> defmt::Format for FdCan<M> {
    fn format(&self, f: defmt::Formatter) {
        defmt::write!(f, "FdCan<{}>", self.instance)
    }
}
