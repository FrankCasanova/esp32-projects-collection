esp_hal::i2s
Module masterCopy item path
Settings
Help

Summary
Source
Available on crate feature unstable only.
Inter-IC Sound (I2S)
Overview
I2S (Inter-IC Sound) is a synchronous serial communication protocol usually used for transmitting audio data between two digital audio devices. Espressif devices may contain more than one I2S peripheral(s). These peripherals can be configured to input and output sample data via the I2S driver.

Configuration
I2S supports different data formats, including varying data and channel widths, different standards, such as the Philips standard and configurable pin mappings for I2S clock (BCLK), word select (WS), and data input/output (DOUT/DIN).

The driver uses DMA (Direct Memory Access) for efficient data transfer and supports various configurations, such as different data formats, standards (e.g., Philips) and pin configurations. It relies on other peripheral modules, such as

GPIO
DMA
system (to configure and enable the I2S peripheral)
Examples
I2S Read
let dma_channel = peripherals.DMA_CH0;
let (mut rx_buffer, rx_descriptors, _, _) = dma_buffers!(4 * 4092, 0);

let i2s = I2s::new(
    peripherals.I2S0,
    Standard::Philips,
    DataFormat::Data16Channel16,
    Rate::from_hz(44100),
    dma_channel,
);
let i2s = i2s.with_mclk(peripherals.GPIO0);
let mut i2s_rx = i2s
    .i2s_rx
    .with_bclk(peripherals.GPIO1)
    .with_ws(peripherals.GPIO2)
    .with_din(peripherals.GPIO5)
    .build(rx_descriptors);

let mut transfer = i2s_rx.read_dma_circular(&mut rx_buffer)?;

loop {
    let avail = transfer.available()?;

    if avail > 0 {
        let mut rcv = [0u8; 5000];
        transfer.pop(&mut rcv[..avail])?;
    }
}
Implementation State
Only TDM Philips standard is supported.
Modules
asynch
Async functionality
Structs
I2s
Instance of the I2S peripheral driver
I2sRx
I2S RX channel
I2sTx
I2S TX channel
Enums
DataFormat
Supported data formats
Error
I2S Error
I2sInterrupt
Represents the various interrupt types for the I2S peripheral.
Standard
Supported standards.
Traits
AcceptedWord
Data types that the I2S peripheral can work with.
Instance