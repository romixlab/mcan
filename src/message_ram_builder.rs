use crate::FdCanInstance;
use crate::message_ram_layout::{DataFieldSize, MessageRamLayout, TxBufferIdx};
use core::marker::PhantomData;
use paste::paste;
use static_cell::StaticCell;

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
pub struct MessageRamBuilder<S> {
    pos: u16,
    end: u16,
    layout: MessageRamLayout,
    /// Used to track for which instance layout is being done and to issue TxBufferIdx-es.
    instance: Option<FdCanInstance>,
    _phantom: PhantomData<S>,
}

#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum MessageRamBuilderError {
    TooManyElements,
    OutOfMemory,
    TooManyInstances,
}

pub(crate) fn message_ram_builder()
-> Result<MessageRamBuilder<ElevenBitFilters>, MessageRamBuilderError> {
    let end = crate::pac::FDCAN_MSGRAM_LEN_WORDS as u16 - 4;
    Ok(MessageRamBuilder {
        pos: 0,
        end,
        layout: MessageRamLayout::default(),
        instance: Some(FdCanInstance::FdCan1),
        _phantom: Default::default(),
    })
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
}

macro_rules! check_and_advance {
    ($self:ident, $max_elements:expr, $len:expr, $element_size_words:expr, $dest:ident) => {
        if $len > $max_elements {
            return Err(MessageRamBuilderError::TooManyElements);
        }
        let new_pos = $self.pos + ($len as u16) * $element_size_words * 4;
        if new_pos > $self.end {
            return Err(MessageRamBuilderError::OutOfMemory);
        }
        paste! {
            $self.layout.[<$dest _addr>] = $self.pos;
            $self.layout.[<$dest _len>] = $len;
        }
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
        if self.instance.is_none() {
            return Err(MessageRamBuilderError::TooManyInstances);
        }
        check_and_advance!(self, Self::MAX_ELEMENTS, len, 1, eleven_bit_filters);
        Ok(self.into_state())
    }

    /// Merge this builder with the other. Useful if doing full re-init and re-layout of multiple CAN instances.
    pub fn recombine(&mut self, _other: MessageRamBuilder<ElevenBitFilters>) {
        todo!()
    }
}

impl MessageRamBuilder<TwentyNineBitFilters> {
    const MAX_ELEMENTS: u8 = 64;

    /// Allocate zero or more 29-bit filters and move to the next step.
    pub const fn allocate_29bit_filters(
        mut self,
        len: u8,
    ) -> Result<MessageRamBuilder<RxFifo0>, MessageRamBuilderError> {
        check_and_advance!(self, Self::MAX_ELEMENTS, len, 2, twenty_nine_bit_filters);
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
            2 + data_size.words(),
            rx_fifo0
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
            2 + data_size.words(),
            rx_fifo1
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
            2 + data_size.words(),
            rx_buffers
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
        check_and_advance!(self, Self::MAX_ELEMENTS, len, 2, tx_event_fifo);
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
        let idx = self.layout.tx_buffers_len;
        self.layout.tx_buffers_len += 1;
        if self.layout.tx_buffers_len > Self::MAX_ELEMENTS {
            return Err(MessageRamBuilderError::TooManyElements);
        }
        let idx = TxBufferIdx {
            instance: self.instance.expect("checked on step one"),
            idx,
        };
        Ok((idx, self))
    }

    /// Allocate zero or more FIFO/Queue buffers, the total number of buffers together with dedicated ones cannot exceed 32.
    pub const fn allocate_fifo_or_queue(
        mut self,
        fifo_or_queue_len: u8,
    ) -> Result<MessageRamBuilder<TriggerMemory>, MessageRamBuilderError> {
        let len = fifo_or_queue_len + self.layout.tx_buffers_len;
        check_and_advance!(
            self,
            Self::MAX_ELEMENTS,
            len,
            2 + self.layout.tx_buffers_data_size.words(),
            tx_buffers
        );
        self.layout.tx_fifo_or_queue_len = fifo_or_queue_len;
        Ok(self.into_state())
    }
}

impl MessageRamBuilder<TriggerMemory> {
    const MAX_ELEMENTS: u8 = 64;

    /// Allocate zero or more trigger elements and get a MessageRamLayout.
    /// Also get a MessageRamBuilder in initial state to build layouts for other instances, if any.
    pub const fn allocate_triggers(
        mut self,
        len: u8,
    ) -> Result<(MessageRamLayout, MessageRamBuilder<ElevenBitFilters>), MessageRamBuilderError>
    {
        check_and_advance!(self, Self::MAX_ELEMENTS, len, 2, trigger_memory);
        let layout = self.layout;
        let next_instance = match self.instance.expect("checked on step one") {
            FdCanInstance::FdCan1 => Some(FdCanInstance::FdCan2),
            FdCanInstance::FdCan2 => Some(FdCanInstance::FdCan3),
            FdCanInstance::FdCan3 => None,
        };
        self.instance = next_instance;
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
