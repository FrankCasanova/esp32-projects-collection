# ESP32-C3 Onboard LED Blink Using Hardware Timer

This project implements a simple timer that blinks the onboard LED of an ESP32-C3 microcontroller every second.

## Project Overview

The ESP32-C3 toggles its onboard LED in a continuous 1-second interval:
- LED turns on for 0.5 seconds
- LED turns off for 0.5 seconds
- Cycle repeats indefinitely

## Hardware Configuration

The project uses the ESP32-C3's onboard LED connected to GPIO8. No external components are required.

```mermaid
graph LR
    A[ESP32-C3] -->|GPIO8| B(Onboard LED)
    B -->|GND| C[Ground]
    style B fill:#00ff00
```

### Components Required
- ESP32-C3 development board (with onboard LED)

## Software Implementation

The project is written in Rust using the `esp-hal` crate. The main loop toggles the LED using a hardware timer:
1. Initialize onboard LED on GPIO8
2. Configure and start hardware timer
3. Continuously check elapsed time
4. Toggle LED every second

### Code Structure
- `src/bin/main.rs`: Main application logic
- `Cargo.toml`: Project dependencies and configuration
- `rust-toolchain.toml`: Rust toolchain version

## Flashing

To flash this project to your ESP32-C3:

1. Connect your ESP32-C3 via USB
2. Build and flash the project:
```bash
cargo run --release
```

## Timing Sequence

| State | Duration |
|-------|----------|
| LED ON | 0.5 seconds |
| LED OFF | 0.5 seconds |
| **Total Cycle** | **1 second** |

## License

This project is licensed under the MIT License - see the LICENSE file for details.
