//! Error counters, protocol status and bus-off recovery (FEATURES.md E1, E2, E3).

use crate::fdcan::{Error, FdCan, Transmit};
use crate::interrupt::PSR_RESET_ON_READ;
use crate::pac::fdcan::regs::{Ecr, Psr};

/// CAN error counters (ECR, Bosch M_CAN user manual "Error Counter Register").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ErrorCounters {
    /// Transmit error counter (TEC, 0..=255).
    pub transmit: u8,
    /// Receive error counter (REC, 0..=127).
    pub receive: u8,
    /// The receive error counter reached the error passive level of 128 (RP).
    pub receive_error_passive: bool,
    /// CAN error logging counter (CEL): errors since the last read, saturating at 255 (IR.ELO fires on
    /// the overflow). Reading ECR resets it.
    pub error_logging: u8,
}

impl ErrorCounters {
    pub(crate) fn from_ecr(ecr: Ecr) -> Self {
        Self {
            transmit: ecr.tec(),
            receive: ecr.rec(),
            receive_error_passive: ecr.rp(),
            error_logging: ecr.cel(),
        }
    }
}

/// Last error code (PSR.LEC / PSR.DLEC).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum LastErrorCode {
    /// The last frame was transferred without error.
    NoError,
    /// More than 5 equal bits in a row where stuffing applies.
    Stuff,
    /// A fixed-format part of a received frame had the wrong format.
    Form,
    /// A transmitted frame was not acknowledged.
    Ack,
    /// Sent recessive, saw dominant (outside arbitration).
    Bit1,
    /// Sent dominant, saw recessive. During bus-off recovery this is set for every sequence of 11
    /// recessive bits, which shows that the recovery is progressing.
    Bit0,
    /// CRC mismatch.
    Crc,
    /// No CAN bus event since the last read.
    NoChange,
}

impl LastErrorCode {
    const fn from_bits(bits: u8) -> Self {
        match bits & 0x7 {
            0 => Self::NoError,
            1 => Self::Stuff,
            2 => Self::Form,
            3 => Self::Ack,
            4 => Self::Bit1,
            5 => Self::Bit0,
            6 => Self::Crc,
            _ => Self::NoChange,
        }
    }

    /// An actual protocol error (not [Self::NoError] or [Self::NoChange]).
    pub const fn is_error(self) -> bool {
        !matches!(self, Self::NoError | Self::NoChange)
    }
}

/// What the protocol controller is doing (PSR.ACT).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Activity {
    /// Synchronizing on CAN communication (also while INIT is set or during bus-off recovery).
    Synchronizing,
    Idle,
    Receiver,
    Transmitter,
}

/// Fault confinement state (ISO 11898-1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum ErrorState {
    /// Both counters below 128.
    Active,
    /// A counter reached 128: the node sends passive error flags only.
    Passive,
    /// The transmit error counter exceeded 255: the node doesn't take part in bus traffic.
    BusOff,
}

/// Protocol status (PSR, Bosch M_CAN user manual "Protocol Status Register").
///
/// The error codes and the `received_*` / `protocol_exception` flags are reset by every PSR read. The driver
/// keeps what its own reads (including [on_interrupt](crate::interrupt::on_interrupt)) saw, so
/// [FdCan::protocol_status] reports everything since the previous call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ProtocolStatus {
    /// Last error in the arbitration phase or a classic frame (LEC).
    pub last_error: LastErrorCode,
    /// Last error in the data phase of an FD frame with BRS (DLEC).
    pub data_last_error: LastErrorCode,
    pub activity: Activity,
    /// A counter is at least 96 (EW).
    pub error_warning: bool,
    /// A counter reached 128 (EP).
    pub error_passive: bool,
    /// Bus-off (BO).
    pub bus_off: bool,
    /// An FD frame with the ESI flag set was received (RESI).
    pub received_esi: bool,
    /// An FD frame with the BRS flag set was received (RBRS).
    pub received_brs: bool,
    /// An FD frame was received (REDL).
    pub received_fd: bool,
    /// A protocol exception event occurred (PXE).
    pub protocol_exception: bool,
    /// Transmitter delay compensation value (TDCV), in minimum time quanta.
    pub tdc_value: u8,
}

impl ProtocolStatus {
    pub(crate) fn from_psr(psr: Psr) -> Self {
        let bits = psr.0;
        Self {
            last_error: LastErrorCode::from_bits(bits as u8),
            data_last_error: LastErrorCode::from_bits((bits >> 8) as u8),
            activity: match (bits >> 3) & 0x3 {
                0 => Activity::Synchronizing,
                1 => Activity::Idle,
                2 => Activity::Receiver,
                _ => Activity::Transmitter,
            },
            error_passive: psr.ep(),
            error_warning: psr.ew(),
            bus_off: psr.bo(),
            received_esi: psr.resi(),
            received_brs: psr.rbrs(),
            received_fd: psr.redl(),
            protocol_exception: psr.pxe(),
            tdc_value: psr.tdcv(),
        }
    }

    pub const fn error_state(&self) -> ErrorState {
        if self.bus_off {
            ErrorState::BusOff
        } else if self.error_passive {
            ErrorState::Passive
        } else {
            ErrorState::Active
        }
    }
}

impl<M> FdCan<M> {
    /// Reads the error counters. This resets the CAN error logging counter (CEL).
    #[inline]
    pub fn error_counters(&mut self) -> ErrorCounters {
        ErrorCounters::from_ecr(self.can.ecr().read())
    }

    /// Reads the protocol status, including errors seen by earlier PSR reads of the driver (see
    /// [ProtocolStatus]).
    #[inline]
    pub fn protocol_status(&mut self) -> ProtocolStatus {
        let psr = self.can.psr().read().0;
        ProtocolStatus::from_psr(Psr(self.state().take_psr(psr)))
    }

    /// Reads PSR for the driver's own use, keeping the reset-on-read fields for [Self::protocol_status].
    #[inline]
    pub(crate) fn read_psr(&self) -> Psr {
        let psr = self.can.psr().read();
        if psr.0 & PSR_RESET_ON_READ != crate::interrupt::PSR_NOTHING_LATCHED {
            self.state().latch_psr(psr.0);
        }
        psr
    }

    /// Whether the node is bus-off (PSR.BO). Doesn't consume the error codes of [Self::protocol_status].
    #[inline]
    pub fn is_bus_off(&self) -> bool {
        self.read_psr().bo()
    }
}

impl<M: Transmit> FdCan<M> {
    /// Starts the bus-off recovery if the node is bus-off and recovery hasn't started yet.
    ///
    /// On bus-off the core sets CCCR.INIT. Clearing it (without a mode change, CCE stays clear) starts the
    /// recovery: after 129 sequences of 11 recessive bits the node becomes error active again with both
    /// counters at zero (Bosch M_CAN user manual, "Bus_Off Recovery"; RM0468 / RM0444 FDCAN "Bus-off
    /// recovery"). PSR.BO stays set until then, [Self::is_bus_off] or the Bus_Off interrupt tell when it's
    /// done.
    ///
    /// Returns whether a recovery was started. See also
    /// [FdCanConfig::automatic_bus_off_recovery](crate::config::FdCanConfig::automatic_bus_off_recovery).
    pub fn recover_from_bus_off(&mut self) -> Result<bool, Error> {
        if !self.is_bus_off() || !self.can.cccr().read().init() {
            return Ok(false);
        }
        self.can.cccr().modify(|w| w.set_init(false));
        // INIT crosses clock domains, wait until the write is visible (RM0468 FDCAN_CCCR.INIT).
        crate::util::checked_wait(
            || self.can.cccr().read().init(),
            self.config.timeout_iterations_short,
        )?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ecr_decoding() {
        // TEC 200, REC 127 + RP, CEL 5 (RM0468 FDCAN_ECR: TEC 7:0, REC 14:8, RP 15, CEL 23:16).
        let ecr = Ecr(200 | 127 << 8 | 1 << 15 | 5 << 16);
        assert_eq!(
            ErrorCounters::from_ecr(ecr),
            ErrorCounters {
                transmit: 200,
                receive: 127,
                receive_error_passive: true,
                error_logging: 5,
            }
        );
    }

    #[test]
    fn psr_decoding() {
        // LEC Ack, ACT Transmitter, EP, EW, BO, DLEC CRC, RESI, RBRS, REDL, PXE, TDCV 0x55
        // (RM0468 FDCAN_PSR: LEC 2:0, ACT 4:3, EP 5, EW 6, BO 7, DLEC 10:8, RESI 11, RBRS 12, REDL 13,
        // PXE 14, TDCV 22:16).
        let psr = Psr(3 | 3 << 3 | 1 << 5 | 1 << 6 | 1 << 7 | 6 << 8 | 0xF << 11 | 0x55 << 16);
        let s = ProtocolStatus::from_psr(psr);
        assert_eq!(
            s,
            ProtocolStatus {
                last_error: LastErrorCode::Ack,
                data_last_error: LastErrorCode::Crc,
                activity: Activity::Transmitter,
                error_warning: true,
                error_passive: true,
                bus_off: true,
                received_esi: true,
                received_brs: true,
                received_fd: true,
                protocol_exception: true,
                tdc_value: 0x55,
            }
        );
        assert_eq!(s.error_state(), ErrorState::BusOff);

        let s = ProtocolStatus::from_psr(Psr(7 | 1 << 3 | 7 << 8));
        assert_eq!(s.last_error, LastErrorCode::NoChange);
        assert_eq!(s.data_last_error, LastErrorCode::NoChange);
        assert_eq!(s.activity, Activity::Idle);
        assert_eq!(s.error_state(), ErrorState::Active);
        assert!(!LastErrorCode::NoChange.is_error() && !LastErrorCode::NoError.is_error());
        assert!(LastErrorCode::Bit0.is_error());
    }
}
