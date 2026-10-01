//! Async waits on interrupt events (FEATURES.md I3). Needs [on_interrupt](crate::interrupt::on_interrupt)
//! to be called from the instance's interrupt handlers and both vectors to be unmasked in the NVIC.
//!
//! There is one waker per instance: only one async operation per [FdCan] can wait at a time, which `&mut self`
//! already guarantees.

use crate::fdcan::FdCan;
use crate::interrupt::Interrupts;
use core::future::poll_fn;
use core::task::Poll;

impl<M> FdCan<M> {
    /// Enables `interrupts` and waits until at least one of them is flagged, then takes and returns the
    /// flagged ones (see [FdCan::take_interrupt_flags]). Returns at once for flags that are still set from
    /// before. The interrupts stay enabled.
    pub async fn wait_interrupts(&mut self, interrupts: Interrupts) -> Interrupts {
        self.enable_interrupts(interrupts);
        poll_fn(|cx| {
            self.state().waker.register(cx.waker());
            let flags = self.take_interrupt_flags(interrupts);
            if flags.is_empty() {
                Poll::Pending
            } else {
                Poll::Ready(flags)
            }
        })
        .await
    }

    /// Waits until the node is bus-off. Returns at once if it already is.
    pub async fn wait_bus_off(&mut self) {
        self.wait_bus_state(true).await
    }

    /// Waits until the node is no longer bus-off, i.e. a bus-off recovery has finished. Returns at once if it
    /// isn't bus-off.
    pub async fn wait_bus_off_recovered(&mut self) {
        self.wait_bus_state(false).await
    }

    async fn wait_bus_state(&mut self, bus_off: bool) {
        // Bus_Off fires on every change of PSR.BO; take stale flags first so the check below is current.
        self.enable_interrupts(Interrupts::BUS_OFF);
        self.take_interrupt_flags(Interrupts::BUS_OFF);
        while self.is_bus_off() != bus_off {
            self.wait_interrupts(Interrupts::BUS_OFF).await;
        }
    }
}
