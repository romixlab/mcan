use crate::FdCanInstance;
use crate::message_ram_layout::{DataFieldSize, MessageRamLayout, TxBufferIdx};
use core::marker::PhantomData;

// The builder states below. Builder will go through these states step by step for consistency and
// simplicity, though MCAN itself does not impose a particular order of various blocks.
pub struct ElevenBitFilters;
pub type RamBuilderInitialState = ElevenBitFilters;
pub struct TwentyNineBitFilters;
pub struct RxFifo0;
pub struct RxFifo1;
pub struct RxBuffers;
pub struct TxEventFifo;
pub struct TxBufferElementSize;
pub struct TxBuffers;
pub struct TriggerMemory;

/// Message RAM partitioner.
///
/// All positions are 32-bit word offsets from the start of the message RAM, which is what the start
/// address fields expect (SIDFC.FLSSA, RXFxC.FxSA, TXBC.TBSA, …: bits [15:2] of the byte offset, RM0468
/// FDCAN register description; Bosch M_CAN user manual, "Message RAM").
pub struct MessageRamBuilder<S> {
    /// Next free word.
    pos: u16,
    /// One past the last usable word.
    end: u16,
    layout: MessageRamLayout,
    /// Instance the current layout is for, also used to issue TxBufferIdx-es. `None` once every
    /// instance has a layout.
    instance: Option<FdCanInstance>,
    _phantom: PhantomData<S>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum MessageRamBuilderError {
    TooManyElements,
    OutOfMemory,
    TooManyInstances,
    /// Trigger memory (TTCAN) only exists on FDCAN1.
    TriggerMemoryNotSupported,
}

pub(crate) const fn message_ram_builder() -> MessageRamBuilder<ElevenBitFilters> {
    MessageRamBuilder {
        pos: 0,
        end: crate::pac::FDCAN_MSGRAM_LEN_WORDS as u16,
        layout: MessageRamLayout::default(),
        instance: Some(FdCanInstance::FdCan1),
        _phantom: PhantomData,
    }
}

impl<S> MessageRamBuilder<S> {
    const fn into_state<S2>(self) -> MessageRamBuilder<S2> {
        MessageRamBuilder {
            pos: self.pos,
            end: self.end,
            layout: self.layout,
            instance: self.instance,
            _phantom: PhantomData,
        }
    }

    const fn instance(&self) -> Result<FdCanInstance, MessageRamBuilderError> {
        match self.instance {
            Some(instance) => Ok(instance),
            None => Err(MessageRamBuilderError::TooManyInstances),
        }
    }
}

/// Reserves `$len` elements of `$element_size_words` words at the current position and records the
/// start address and length in `layout.$addr` / `layout.$len_field`.
macro_rules! check_and_advance {
    ($self:ident, $max_elements:expr, $len:expr, $element_size_words:expr, $addr:ident, $len_field:ident) => {
        if $len > $max_elements {
            return Err(MessageRamBuilderError::TooManyElements);
        }
        // At most 64 elements of 18 words and pos <= end <= 2560, so this cannot overflow u16.
        let new_pos = $self.pos + ($len as u16) * $element_size_words;
        if new_pos > $self.end {
            return Err(MessageRamBuilderError::OutOfMemory);
        }
        $self.layout.$addr = $self.pos;
        $self.layout.$len_field = $len;
        $self.pos = new_pos;
    };
}

impl MessageRamBuilder<ElevenBitFilters> {
    const MAX_ELEMENTS: u8 = 128;

    /// Allocate zero or more 11-bit filters and move to the next step.
    pub const fn allocate_11bit_filters(
        mut self,
        len: u8,
    ) -> Result<MessageRamBuilder<TwentyNineBitFilters>, MessageRamBuilderError> {
        let instance = match self.instance() {
            Ok(instance) => instance,
            Err(e) => return Err(e),
        };
        self.layout = MessageRamLayout::default();
        self.layout.instance = Some(instance);
        self.layout.start_addr = self.pos;
        check_and_advance!(
            self,
            Self::MAX_ELEMENTS,
            len,
            1,
            eleven_bit_filters_addr,
            eleven_bit_filters_len
        );
        Ok(self.into_state())
    }
}

impl MessageRamBuilder<TwentyNineBitFilters> {
    const MAX_ELEMENTS: u8 = 64;

    /// Allocate zero or more 29-bit filters and move to the next step.
    pub const fn allocate_29bit_filters(
        mut self,
        len: u8,
    ) -> Result<MessageRamBuilder<RxFifo0>, MessageRamBuilderError> {
        check_and_advance!(
            self,
            Self::MAX_ELEMENTS,
            len,
            2,
            twenty_nine_bit_filters_addr,
            twenty_nine_bit_filters_len
        );
        Ok(self.into_state())
    }
}

impl MessageRamBuilder<RxFifo0> {
    const MAX_ELEMENTS: u8 = 64;

    /// Allocate zero or more RX FIFO0 elements and move to the next step.
    pub const fn allocate_rx_fifo0_buffers(
        mut self,
        len: u8,
        data_size: DataFieldSize,
    ) -> Result<MessageRamBuilder<RxFifo1>, MessageRamBuilderError> {
        check_and_advance!(
            self,
            Self::MAX_ELEMENTS,
            len,
            data_size.element_words(),
            rx_fifo0_addr,
            rx_fifo0_len
        );
        self.layout.rx_fifo0_data_size = data_size;
        Ok(self.into_state())
    }
}

impl MessageRamBuilder<RxFifo1> {
    const MAX_ELEMENTS: u8 = 64;

    /// Allocate zero or more RX FIFO1 elements and move to the next step.
    pub const fn allocate_rx_fifo1_buffers(
        mut self,
        len: u8,
        data_size: DataFieldSize,
    ) -> Result<MessageRamBuilder<RxBuffers>, MessageRamBuilderError> {
        check_and_advance!(
            self,
            Self::MAX_ELEMENTS,
            len,
            data_size.element_words(),
            rx_fifo1_addr,
            rx_fifo1_len
        );
        self.layout.rx_fifo1_data_size = data_size;
        Ok(self.into_state())
    }
}

impl MessageRamBuilder<RxBuffers> {
    const MAX_ELEMENTS: u8 = 64;

    /// Allocate dedicated RX buffers space and move to the next step.
    pub const fn allocate_rx_buffers(
        mut self,
        len: u8,
        data_size: DataFieldSize,
    ) -> Result<MessageRamBuilder<TxEventFifo>, MessageRamBuilderError> {
        check_and_advance!(
            self,
            Self::MAX_ELEMENTS,
            len,
            data_size.element_words(),
            rx_buffers_addr,
            rx_buffers_len
        );
        self.layout.rx_buffers_data_size = data_size;
        Ok(self.into_state())
    }

    /// Skip allocating and move to the next step.
    pub const fn skip_dedicated_buffers(self) -> MessageRamBuilder<TxEventFifo> {
        self.into_state()
    }
}

impl MessageRamBuilder<TxEventFifo> {
    const MAX_ELEMENTS: u8 = 32;

    /// Allocate zero or more TX Event FIFO elements and move to the next step.
    pub const fn allocate_tx_event_fifo_buffers(
        mut self,
        len: u8,
    ) -> Result<MessageRamBuilder<TxBufferElementSize>, MessageRamBuilderError> {
        check_and_advance!(
            self,
            Self::MAX_ELEMENTS,
            len,
            2,
            tx_event_fifo_addr,
            tx_event_fifo_len
        );
        Ok(self.into_state())
    }
}

impl MessageRamBuilder<TxBufferElementSize> {
    pub const fn tx_buffer_element_size(
        mut self,
        data_size: DataFieldSize,
    ) -> MessageRamBuilder<TxBuffers> {
        self.layout.tx_buffers_data_size = data_size;
        self.into_state()
    }
}

impl MessageRamBuilder<TxBuffers> {
    const MAX_ELEMENTS: u8 = 32;

    /// Allocate dedicated TX buffer and get a TxBufferIdx that can be later used to interact with it.
    pub const fn allocate_dedicated_tx_buffer(
        mut self,
    ) -> Result<(TxBufferIdx, Self), MessageRamBuilderError> {
        let instance = match self.instance() {
            Ok(instance) => instance,
            Err(e) => return Err(e),
        };
        let idx = self.layout.tx_buffers_len;
        if idx >= Self::MAX_ELEMENTS {
            return Err(MessageRamBuilderError::TooManyElements);
        }
        self.layout.tx_buffers_len += 1;
        Ok((TxBufferIdx { instance, idx }, self))
    }

    /// Allocate zero or more FIFO/Queue buffers, the total number of buffers together with dedicated ones cannot exceed 32.
    pub const fn allocate_fifo_or_queue(
        mut self,
        fifo_or_queue_len: u8,
    ) -> Result<MessageRamBuilder<TriggerMemory>, MessageRamBuilderError> {
        let dedicated_len = self.layout.tx_buffers_len;
        let Some(len) = fifo_or_queue_len.checked_add(dedicated_len) else {
            return Err(MessageRamBuilderError::TooManyElements);
        };
        // The layout keeps the number of dedicated buffers (TXBC.NDTB), the region covers both.
        check_and_advance!(
            self,
            Self::MAX_ELEMENTS,
            len,
            self.layout.tx_buffers_data_size.element_words(),
            tx_buffers_addr,
            tx_buffers_len
        );
        self.layout.tx_buffers_len = dedicated_len;
        self.layout.tx_fifo_or_queue_len = fifo_or_queue_len;
        Ok(self.into_state())
    }
}

impl MessageRamBuilder<TriggerMemory> {
    const MAX_ELEMENTS: u8 = 64;

    /// Allocate zero or more trigger elements and get a MessageRamLayout.
    /// Also get a MessageRamBuilder in initial state to build layouts for other instances, if any.
    ///
    /// Trigger memory is only used by TTCAN, which only FDCAN1 supports (RM0468, FDCAN introduction).
    pub const fn allocate_triggers(
        mut self,
        len: u8,
    ) -> Result<(MessageRamLayout, MessageRamBuilder<ElevenBitFilters>), MessageRamBuilderError>
    {
        let instance = match self.instance() {
            Ok(instance) => instance,
            Err(e) => return Err(e),
        };
        if len > 0 && !matches!(instance, FdCanInstance::FdCan1) {
            return Err(MessageRamBuilderError::TriggerMemoryNotSupported);
        }
        check_and_advance!(
            self,
            Self::MAX_ELEMENTS,
            len,
            2,
            trigger_memory_addr,
            trigger_memory_len
        );
        self.layout.end_addr = self.pos;
        let layout = self.layout;
        self.instance = match instance {
            FdCanInstance::FdCan1 => Some(FdCanInstance::FdCan2),
            FdCanInstance::FdCan2 => Some(FdCanInstance::FdCan3),
            FdCanInstance::FdCan3 => None,
        };
        self.layout = MessageRamLayout::default();
        Ok((layout, self.into_state()))
    }
}

macro_rules! unwrap_or_return {
    ($expr:expr) => {
        match $expr {
            Ok(b) => b,
            Err(e) => return Err(e),
        }
    };
}

pub const fn basic_layout(
    builder: MessageRamBuilder<RamBuilderInitialState>,
) -> Result<(MessageRamLayout, MessageRamBuilder<RamBuilderInitialState>), MessageRamBuilderError> {
    let b = unwrap_or_return!(builder.allocate_11bit_filters(1));
    let b = unwrap_or_return!(b.allocate_29bit_filters(1));
    let b = unwrap_or_return!(b.allocate_rx_fifo0_buffers(1, DataFieldSize::_64Bytes));
    let b = unwrap_or_return!(b.allocate_rx_fifo1_buffers(0, DataFieldSize::_64Bytes));
    let b = b.skip_dedicated_buffers();
    let b = unwrap_or_return!(b.allocate_tx_event_fifo_buffers(1));
    let b = b.tx_buffer_element_size(DataFieldSize::_64Bytes);
    let b = unwrap_or_return!(b.allocate_fifo_or_queue(1));
    let (layout, builder) = unwrap_or_return!(b.allocate_triggers(0));
    Ok((layout, builder))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Layout used by `examples/h7_embassy`: every region non-empty except RX FIFO1 and triggers.
    fn example_layout(
        builder: MessageRamBuilder<ElevenBitFilters>,
    ) -> Result<
        (
            MessageRamLayout,
            MessageRamBuilder<ElevenBitFilters>,
            [TxBufferIdx; 3],
        ),
        MessageRamBuilderError,
    > {
        let builder = builder
            .allocate_11bit_filters(3)?
            .allocate_29bit_filters(3)?
            .allocate_rx_fifo0_buffers(3, DataFieldSize::_64Bytes)?
            .allocate_rx_fifo1_buffers(0, DataFieldSize::_64Bytes)?
            .allocate_rx_buffers(3, DataFieldSize::_64Bytes)?
            .allocate_tx_event_fifo_buffers(3)?
            .tx_buffer_element_size(DataFieldSize::_64Bytes);
        let (idx1, builder) = builder.allocate_dedicated_tx_buffer()?;
        let (idx2, builder) = builder.allocate_dedicated_tx_buffer()?;
        let (idx3, builder) = builder.allocate_dedicated_tx_buffer()?;
        let (layout, builder) = builder.allocate_fifo_or_queue(3)?.allocate_triggers(0)?;
        Ok((layout, builder, [idx1, idx2, idx3]))
    }

    /// Start addresses are 32-bit word offsets into the message RAM (RM0468 56.5.x: SIDFC.FLSSA,
    /// XIDFC.FLESA, RXFxC.FxSA, RXBC.RBSA, TXEFC.EFSA, TXBC.TBSA, TTTMC.TMSA are bits [15:2] of the
    /// byte address). Element sizes in words: std filter 1, ext filter 2, RX/TX element 2 + data,
    /// TX event 2, trigger 2.
    #[test]
    fn start_addresses_are_word_offsets() {
        let builder = message_ram_builder();
        let (l, _, idx) = example_layout(builder).ok().unwrap();
        assert_eq!(l.eleven_bit_filters_addr, 0);
        assert_eq!(l.eleven_bit_filters_len, 3);
        assert_eq!(l.twenty_nine_bit_filters_addr, 3);
        assert_eq!(l.twenty_nine_bit_filters_len, 3);
        assert_eq!(l.rx_fifo0_addr, 3 + 3 * 2);
        assert_eq!(l.rx_fifo0_len, 3);
        assert_eq!(l.rx_fifo1_addr, 9 + 3 * 18);
        assert_eq!(l.rx_fifo1_len, 0);
        assert_eq!(l.rx_buffers_addr, 63);
        assert_eq!(l.rx_buffers_len, 3);
        assert_eq!(l.tx_event_fifo_addr, 63 + 3 * 18);
        assert_eq!(l.tx_event_fifo_len, 3);
        assert_eq!(l.tx_buffers_addr, 117 + 3 * 2);
        assert_eq!(l.tx_buffers_len, 3);
        assert_eq!(l.tx_fifo_or_queue_len, 3);
        assert_eq!(l.trigger_memory_addr, 123 + 6 * 18);
        assert_eq!(l.trigger_memory_len, 0);
        assert_eq!(idx.map(|i| i.idx), [0, 1, 2]);
        assert!(idx.iter().all(|i| i.instance == FdCanInstance::FdCan1));
    }

    #[test]
    fn next_instance_starts_fresh_after_previous() {
        let builder = message_ram_builder();
        let (l1, builder, _) = example_layout(builder).ok().unwrap();
        let (l2, builder, idx2) = example_layout(builder).ok().unwrap();
        let (l3, _, idx3) = example_layout(builder).ok().unwrap();

        let size = l1.trigger_memory_addr;
        assert_eq!(l2.eleven_bit_filters_addr, size);
        assert_eq!(l2.tx_buffers_addr, l1.tx_buffers_addr + size);
        assert_eq!(l3.eleven_bit_filters_addr, 2 * size);

        // Dedicated TX buffer count and indices restart per instance.
        assert_eq!(l2.tx_buffers_len, 3);
        assert_eq!(l3.tx_buffers_len, 3);
        assert_eq!(idx2.map(|i| i.idx), [0, 1, 2]);
        assert!(idx2.iter().all(|i| i.instance == FdCanInstance::FdCan2));
        assert!(idx3.iter().all(|i| i.instance == FdCanInstance::FdCan3));
    }

    #[test]
    fn fourth_instance_is_rejected() {
        let mut builder = message_ram_builder();
        for _ in 0..3 {
            builder = basic_layout(builder).ok().unwrap().1;
        }
        assert!(matches!(
            builder.allocate_11bit_filters(0),
            Err(MessageRamBuilderError::TooManyInstances)
        ));
    }

    /// 128 + 64 * 2 + 2 * 64 * 18 = 2560 words, the whole H7 message RAM.
    fn fill_ram(fifo1_len: u8) -> Result<MessageRamBuilder<RxBuffers>, MessageRamBuilderError> {
        message_ram_builder()
            .allocate_11bit_filters(128)?
            .allocate_29bit_filters(64)?
            .allocate_rx_fifo0_buffers(64, DataFieldSize::_64Bytes)?
            .allocate_rx_fifo1_buffers(fifo1_len, DataFieldSize::_64Bytes)
    }

    #[test]
    fn whole_ram_can_be_used() {
        assert_eq!(crate::pac::FDCAN_MSGRAM_LEN_WORDS, 2560);
        let b = fill_ram(64).ok().unwrap();
        assert!(matches!(
            b.skip_dedicated_buffers().allocate_tx_event_fifo_buffers(1),
            Err(MessageRamBuilderError::OutOfMemory)
        ));
        let b = fill_ram(63).ok().unwrap();
        assert!(b.allocate_rx_buffers(1, DataFieldSize::_64Bytes).is_ok());
    }

    #[test]
    fn element_count_limits() {
        let b = message_ram_builder();
        assert!(matches!(
            b.allocate_11bit_filters(129),
            Err(MessageRamBuilderError::TooManyElements)
        ));
    }

    #[test]
    fn tx_fifo_len_overflow_is_an_error() {
        let b = message_ram_builder()
            .allocate_11bit_filters(0)
            .and_then(|b| b.allocate_29bit_filters(0))
            .and_then(|b| b.allocate_rx_fifo0_buffers(0, DataFieldSize::_8Bytes))
            .and_then(|b| b.allocate_rx_fifo1_buffers(0, DataFieldSize::_8Bytes))
            .map(|b| b.skip_dedicated_buffers())
            .and_then(|b| b.allocate_tx_event_fifo_buffers(0))
            .map(|b| b.tx_buffer_element_size(DataFieldSize::_8Bytes))
            .and_then(|b| b.allocate_dedicated_tx_buffer())
            .ok()
            .unwrap()
            .1;
        // 255 + 1 dedicated buffer overflows u8.
        assert!(matches!(
            b.allocate_fifo_or_queue(255),
            Err(MessageRamBuilderError::TooManyElements)
        ));
    }

    #[test]
    fn layout_records_instance_and_region() {
        let (l1, builder, _) = example_layout(message_ram_builder()).ok().unwrap();
        let (l2, _) = basic_layout(builder).ok().unwrap();
        assert_eq!(l1.instance, Some(FdCanInstance::FdCan1));
        assert_eq!((l1.start_addr, l1.end_addr), (0, 231));
        assert_eq!(l2.instance, Some(FdCanInstance::FdCan2));
        // 1 + 2 + 18 (FIFO0) + 2 (TX event) + 18 (one TX FIFO element)
        assert_eq!((l2.start_addr, l2.end_addr), (231, 231 + 41));
        assert_eq!(MessageRamLayout::default().instance, None);
    }

    #[test]
    fn tx_buffer_addr_uses_element_size() {
        let (l, _, _) = example_layout(message_ram_builder()).ok().unwrap();
        assert_eq!(l.tx_buffer_addr(0), Some(123));
        assert_eq!(l.tx_buffer_addr(1), Some(123 + 18));
        assert_eq!(l.tx_buffer_addr(2), Some(123 + 36));
        // Only dedicated buffers are addressable by index, FIFO/queue elements are not.
        assert_eq!(l.tx_buffer_addr(3), None);
        assert_eq!(MessageRamLayout::default().tx_buffer_addr(0), None);
    }

    #[test]
    fn dedicated_tx_buffers_limit() {
        let mut b = message_ram_builder()
            .allocate_11bit_filters(0)
            .and_then(|b| b.allocate_29bit_filters(0))
            .and_then(|b| b.allocate_rx_fifo0_buffers(0, DataFieldSize::_8Bytes))
            .and_then(|b| b.allocate_rx_fifo1_buffers(0, DataFieldSize::_8Bytes))
            .map(|b| b.skip_dedicated_buffers())
            .and_then(|b| b.allocate_tx_event_fifo_buffers(0))
            .map(|b| b.tx_buffer_element_size(DataFieldSize::_8Bytes))
            .ok()
            .unwrap();
        for i in 0..32 {
            let (idx, next) = b.allocate_dedicated_tx_buffer().ok().unwrap();
            assert_eq!(idx.idx, i);
            b = next;
        }
        assert!(matches!(
            b.allocate_dedicated_tx_buffer(),
            Err(MessageRamBuilderError::TooManyElements)
        ));
    }

    #[test]
    fn trigger_memory_only_on_fdcan1() {
        let to_triggers = |b: MessageRamBuilder<ElevenBitFilters>| {
            b.allocate_11bit_filters(0)
                .and_then(|b| b.allocate_29bit_filters(0))
                .and_then(|b| b.allocate_rx_fifo0_buffers(0, DataFieldSize::_8Bytes))
                .and_then(|b| b.allocate_rx_fifo1_buffers(0, DataFieldSize::_8Bytes))
                .map(|b| b.skip_dedicated_buffers())
                .and_then(|b| b.allocate_tx_event_fifo_buffers(0))
                .map(|b| b.tx_buffer_element_size(DataFieldSize::_8Bytes))
                .and_then(|b| b.allocate_fifo_or_queue(0))
        };
        let (l1, b) = to_triggers(message_ram_builder())
            .and_then(|b| b.allocate_triggers(4))
            .ok()
            .unwrap();
        assert_eq!((l1.trigger_memory_addr, l1.trigger_memory_len), (0, 4));
        assert_eq!(l1.end_addr, 8);
        assert!(matches!(
            to_triggers(b).and_then(|b| b.allocate_triggers(1)),
            Err(MessageRamBuilderError::TriggerMemoryNotSupported)
        ));
    }

    #[test]
    fn rx_fifo_element_addr_uses_element_size() {
        use crate::message_ram_layout::RxFifo;
        let (l, _, _) = example_layout(message_ram_builder()).ok().unwrap();
        // FIFO0 at word 9, 3 elements of 2 + 16 words.
        assert_eq!(
            l.rx_fifo_element_addr(RxFifo::Fifo0, 0).map(|e| e.0),
            Some(9)
        );
        assert_eq!(
            l.rx_fifo_element_addr(RxFifo::Fifo0, 2).map(|e| e.0),
            Some(9 + 36)
        );
        assert_eq!(l.rx_fifo_element_addr(RxFifo::Fifo0, 3), None);
        // FIFO1 has no elements.
        assert_eq!(l.rx_fifo_element_addr(RxFifo::Fifo1, 0), None);

        let l = message_ram_builder()
            .allocate_11bit_filters(0)
            .and_then(|b| b.allocate_29bit_filters(0))
            .and_then(|b| b.allocate_rx_fifo0_buffers(2, DataFieldSize::_8Bytes))
            .and_then(|b| b.allocate_rx_fifo1_buffers(4, DataFieldSize::_12Bytes))
            .map(|b| b.skip_dedicated_buffers())
            .and_then(|b| b.allocate_tx_event_fifo_buffers(0))
            .map(|b| b.tx_buffer_element_size(DataFieldSize::_8Bytes))
            .and_then(|b| b.allocate_fifo_or_queue(0))
            .and_then(|b| b.allocate_triggers(0))
            .ok()
            .unwrap()
            .0;
        assert_eq!(
            l.rx_fifo_element_addr(RxFifo::Fifo1, 3),
            Some((2 * 4 + 3 * 5, DataFieldSize::_12Bytes))
        );
    }
}
