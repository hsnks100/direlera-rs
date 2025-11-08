# direlera-rs

**direlera-rs** is a Rust-based server that uses the Kaillera protocol to facilitate online multiplayer for emulators.

> ⚠️ **Experimental Project**: This is an early-stage experimental project. Stability and user experience have not been thoroughly tested or optimized yet. Use at your own risk.

## What is Kaillera?

Kaillera is a network protocol that enables online multiplayer gaming in emulators. Developed in the late 1990s, it has been widely used in various emulators such as MAME, Project64, and Snes9x. Through Kaillera, users can play retro games together in real-time over the internet.

## Why This Project?

Direlera-rs is an experimental attempt to reimplement the Kaillera server protocol using modern tools:

- **Learning**: Exploring Rust's async I/O and network programming capabilities
- **Protocol Analysis**: Better understanding of the Kaillera protocol through implementation
- **Transparency**: Providing Wireshark dissector for protocol analysis and debugging
- **Modernization**: Experimenting with a Rust-based implementation of the legacy protocol

## Current Features

- Kaillera 0.83 protocol implementation (basic)
- Multi-room game hosting
- Global chat and in-game chat
- Ping calculation
- TOML configuration file
- Wireshark protocol dissector (Lua)
- EUC-KR encoding support

## Getting Started

### Prerequisites

- Rust 1.70 or higher (install: https://rustup.rs/)

### Running the Server

1. **Download from Releases** (Recommended)

   Download the latest version from the [Releases page](https://github.com/yourusername/direlera-rs/releases).

   ```bash
   # After extracting
   cd direlera-rs
   ./direlera-rs  # Linux/macOS
   direlera-rs.exe  # Windows
   ```

2. **Build from Source**

   ```bash
   git clone https://github.com/yourusername/direlera-rs.git
   cd direlera-rs
   cargo build --release
   ./target/release/direlera-rs
   ```

3. **Run in Development Mode**

   ```bash
   cargo run
   ```

The server runs on the following ports by default:

- **Main Port**: 8080 (game logic)
- **Control Port**: 27888 (initial connection and ping)

### Configuration

You can configure the server by modifying the `direlera.toml` file:

```toml
main_port = 27888
sub_port = 27999
debug = false
random_ping = false
priority = 32
key = "your-secret-key"
notice = """
Write your server notice here.
You can write multiple lines.
Korean characters are supported too!
"""
```

## Wireshark Dissector Setup

The included Wireshark dissector allows you to analyze Kaillera protocol packets.

### Installation Steps

1. **Find Wireshark Plugin Directory**

   In Wireshark: `Help → About Wireshark → Folders → Personal Lua Plugins`

   Common paths:

   - **Windows**: `%APPDATA%\Wireshark\plugins\`
   - **Linux**: `~/.local/lib/wireshark/plugins/`
   - **macOS**: `~/.wireshark/plugins/` or `/Applications/Wireshark.app/Contents/PlugIns/wireshark/`

2. **Copy the Dissector**

   ```bash
   # Windows (PowerShell)
   Copy-Item kaillera.lua "$env:APPDATA\Wireshark\plugins\"

   # Linux/macOS
   cp kaillera.lua ~/.local/lib/wireshark/plugins/
   ```

3. **Restart Wireshark**

   After restarting Wireshark, the Kaillera protocol will be automatically recognized.

4. **Usage**

   - Start capturing on UDP ports 27888 and 8080
   - Use filter: `kaillera` to display only Kaillera packets

## How It Works

For a detailed explanation of the Kaillera game synchronization protocol, including:

- Game Data (0x12) and Game Cache (0x13) packet behavior
- Per-player caching mechanisms
- Frame synchronization with mixed connection types
- Frame interleaving algorithm
- Preemptive padding for multi-delay synchronization

See **[GAME_SYNC_PROTOCOL.md](GAME_SYNC_PROTOCOL.md)** - This document describes the actual protocol behavior discovered through reverse engineering and packet analysis with Wireshark.

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for details.

Quick summary:

1. Check existing issues or create a new one
2. Create a feature branch from the `develop` branch
3. Commit your changes and submit a PR to the `develop` branch

## License

This project is licensed under the terms specified in the [LICENSE](LICENSE) file.

## References

- [Kaillera Official Website](http://www.kaillera.com/)
- [EmuLinker-K](https://github.com/sysfce2/EmuLinker-K) - Similar Kotlin implementation
- [Protocol Documentation](protocol.txt) - Detailed Kaillera protocol documentation

## Contact

Please report bugs or feature requests on [GitHub Issues](https://github.com/yourusername/direlera-rs/issues).
