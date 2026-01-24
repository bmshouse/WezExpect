# Quick Start Guide

## Prerequisites

- Rust installed ([rustup.rs](https://rustup.rs/))
- WezTerm installed and in PATH
- **Windows**: Visual Studio Build Tools with C++ workload

## Build & Run

```bash
# Build optimized release
cargo build --release

# Or run directly (builds automatically)
cargo run --release
```

**Windows users:** Use PowerShell or CMD, not Git Bash.

## Installation

For easier daily use, install Wez Expect as a system binary:

```bash
cargo install --path .
```

The binary will be installed to `~/.cargo/bin/wez_expect` (automatically in your PATH).

Then run from anywhere without `cargo`:

```bash
wez_expect                  # Use default config.toml
wez_expect --config custom  # Custom config
wez_expect --pane-id 2      # Specific pane
wez_expect --help           # Show all options
```

**Updating after pulling changes:**

```bash
cargo install --path . --force
```

**Uninstall:**

```bash
cargo uninstall wez_expect
```

## Development Mode

If you're developing or testing changes, use `cargo run` instead of installing:

```bash
cargo run --release               # Uses default config.toml
cargo run --release -- --help     # Show help
```

## Configure

Edit `config.toml`:

```toml
[monitor]
poll_interval_secs = 30

[pane]
# pane_id = 1  # Optional: specify pane to monitor

[[rules]]
name = "timeout_message"
pattern = "You've hit your limit · resets (\\d{1,2}(?::\\d{2})?\\s?(?:am|pm)) \\((.*?)\\)"
action = "wait_for_time"  # or "immediate"
command = "continue"
enabled = true
```

### Rule Types
- **wait_for_time**: Parse time/timezone, wait, then send command (requires 2 capture groups)
- **immediate**: Send command immediately when pattern matches (no capture groups needed)

## Common Commands

```bash
# Run with custom config
cargo run --release -- --config my-config.toml

# Target specific pane
cargo run --release -- --pane-id 2

# Show help
cargo run --release -- --help

# Run tests
cargo test

# Clean build
cargo clean
```

## Find Pane IDs

```bash
wezterm cli list
```

Add the ID to `config.toml`:
```toml
[pane]
pane_id = 2
```

## Test It

**Test wait_for_time action:**
```bash
echo 'Your limit will reset at 3pm (America/Denver).'
```
Wez Expect will detect, parse time, wait, and send command.

**Test immediate action (add rule first):**
```bash
echo 'Press Enter to continue'
```
Wez Expect will immediately send Enter key.

## Multiple Rules Example

```toml
[[rules]]
name = "timeout"
pattern = "resets (\\d{1,2}(?::\\d{2})?\\s?(?:am|pm)) \\((.*?)\\)"
action = "wait_for_time"
command = "continue\n"

[[rules]]
name = "prompt"
pattern = "Press Enter"
action = "immediate"
command = "\n"
```
First matching enabled rule wins.

## Troubleshooting

**"No WezTerm panes found"**
→ Start WezTerm first

**Pattern doesn't match**
→ For `wait_for_time`: Check pattern has 2 capture groups `(time)` and `(timezone)`
→ For `immediate`: Pattern can be any regex

**Wrong pane monitored**
→ Specify `pane_id` in config

**Rule not matching**
→ Check `enabled = true`
→ Check rule order (first match wins)

**Windows build errors**
→ Install Visual Studio Build Tools with C++ workload

**TOML parse error**
→ Use single quotes `'...'` for regex patterns, not `r#"..."#`

## Resources

- Full docs: `README.md`
- TOML syntax: `TOML_SYNTAX_NOTE.md`
- WezTerm CLI: https://wezterm.org/cli/
