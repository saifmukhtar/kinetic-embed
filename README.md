# Kinetic Embed

This repository contains the core Engine and C-FFI bridge for the Kinetic Network. 
It is designed to be embedded directly into mobile (Android/iOS) and edge devices, allowing them to participate in the Kinetic P2P mesh network natively without requiring a centralized backend.

## Structure
- `core/`: The Rust engine and FFI boundary.
- `include/`: The C-header (`embed.h`) for bindings.
