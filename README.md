# Wez Expect

A flexible, extensible Rust application that monitors WezTerm terminal output for configurable patterns and responds with automated actions.

## Features

- **🎯 4 Built-in Action Types**: Wait-for-time, immediate, conditional, and external command actions
- **🌍 Timezone-aware**: Parse and schedule actions based on timestamps in any timezone
- **⚙️ Flexible Configuration**: Define multiple rules with regex patterns and priorities
- **🔄 Continuous Monitoring**: Automatically resumes watching after each action
- **📊 Progress Logging**: Shows countdown for long waits and detailed execution logs
- **🛡️ Robust Error Handling**: Automatic retry logic with exponential backoff
- **🎨 Clean Architecture**: Trait-based system makes it easy to extend

## Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) - Install via rustup
- [WezTerm](https://wezfurlong.org/wezterm/) - Must be in PATH
- **Windows users**: Visual Studio Build Tools with C++ workload ([download](https://visualstudio.microsoft.com/downloads/))

## Quick Start

```bash
# Clone or download the project
cd wez_expect

# Build release version
cargo build --release

# Run with default config
cargo run --release

# Run with custom config
cargo run --release -- --config my-config.toml
```

**Windows Note:** Use PowerShell or CMD (not Git Bash) due to linker conflicts.

## Installation

### Install as System Binary (Recommended)

```bash
cargo install --path .
```

This installs `wez_expect` to `~/.cargo/bin`, which should be in your PATH. You can then run it from anywhere:

```bash
wez_expect
wez_expect --config my-config.toml
wez_expect --pane-id 2
```

### Development Mode

For active development:

```bash
cargo run --release
cargo run --release -- --config custom.toml
```

### Update

```bash
cargo install --path . --force
```

### Uninstall

```bash
cargo uninstall wez_expect
```

## Configuration

Wez Expect uses a `config.toml` file to define monitoring rules. Each rule specifies a pattern to match and an action to take.

### Basic Configuration

```toml
[monitor]
poll_interval_secs = 30  # How often to check for patterns

[pane]
# Optional: Specify WezTerm pane ID (auto-detects if omitted)
# pane_id = 1

# Define rules: patterns to watch for and actions to take
# Rules are checked in order, first match wins

[[rules]]
name = "timeout_handler"
pattern = "resets (\\d{1,2}(?::\\d{2})?\\s?(?:am|pm)) \\((.*?)\\)"
action_type = "wait_for_time"
command = "continue\n"
enabled = true
```

## Action Types

Wez Expect supports 4 built-in action types:

### 1. Wait For Time (`wait_for_time`)

Extracts time and timezone from captured groups, waits until that time, then sends a command.

**Requirements:**
- Pattern must have exactly 2 capture groups: `(time)` and `(timezone)`

**Example:**
```toml
[[rules]]
name = "api_timeout"
pattern = "Session expires at (\\d{1,2}:\\d{2}\\s?(?:am|pm)) \\((.*?)\\)"
action_type = "wait_for_time"
command = "refresh\n"
```

**Matches:** `"Session expires at 3pm (America/New_York)"`
- Extracts: time="3pm", timezone="America/New_York"
- Waits until 3pm New York time
- Sends: `refresh\n`

### 2. Immediate (`immediate`)

Sends a command instantly when the pattern matches.

**Example:**
```toml
[[rules]]
name = "press_enter"
pattern = "Press Enter to continue"
action_type = "immediate"
command = "\n"
```

**Matches:** `"Press Enter to continue"`
- Sends: `\n` immediately
- Waits for terminal content to change before re-checking

### 3. Conditional (`conditional`)

Tests a captured group against a condition and sends different commands based on the result.

**Configuration Fields:**
- `condition_pattern`: Regex to test against the captured group
- `then_command`: Command to send if condition matches
- `else_command`: Command to send if condition doesn't match
- `apply_to_capture`: Which capture group to test (default: 1)

**Example:**
```toml
[[rules]]
name = "build_handler"
pattern = "Build (succeeded|failed) in (\\d+)s"
action_type = "conditional"
condition_pattern = "succeeded"
then_command = "deploy\n"
else_command = "echo 'Build failed'\n"
apply_to_capture = 1
```

**Matches:** `"Build succeeded in 42s"`
- Tests capture group 1 ("succeeded") against "succeeded"
- Condition matches → sends `deploy\n`

**Matches:** `"Build failed in 10s"`
- Tests capture group 1 ("failed") against "succeeded"
- Condition fails → sends `echo 'Build failed'\n`

### 4. External Command (`external_command`)

Executes shell commands instead of sending to terminal. Supports capture group substitution with `$1`, `$2`, etc.

**Configuration Fields:**
- `external_command`: Shell command to execute
- `working_dir`: Optional working directory
- `timeout_secs`: Command timeout (default: 30)
- `env`: Optional environment variables

**Example:**
```toml
[[rules]]
name = "deployment_logger"
pattern = "Deployed to (\\w+) in (\\d+)s"
action_type = "external_command"
external_command = "echo 'Deployment to $1 took $2 seconds' >> deploy.log"
working_dir = "/var/log"
timeout_secs = 10
```

**Matches:** `"Deployed to production in 120s"`
- Executes: `echo 'Deployment to production took 120 seconds' >> deploy.log`
- Working directory: `/var/log`
- Timeout: 10 seconds

**⚠️ Security Warning:** This executes arbitrary shell commands. Ensure patterns are carefully designed to avoid command injection.

## Usage

### Basic Commands

```bash
# Run with default config
wez_expect

# Specify config file
wez_expect --config my-config.toml

# Target specific pane
wez_expect --pane-id 2

# Show help
wez_expect --help
```

### Finding Pane IDs

```bash
wezterm cli list
```

Output shows pane IDs in the first column:
```
PANE_ID  WINDOW_ID  TITLE
0        0          zsh
1        0          cargo run
2        1          vim README.md
```

Specify in config:
```toml
[pane]
pane_id = 2
```

## Example Workflows

### Time-Based Timeout Handler

**Terminal shows:**
```
Your limit will reset at 3pm (America/Santiago).
```

**Wez Expect:**
1. Matches the `wait_for_time` rule
2. Extracts time: "3pm", timezone: "America/Santiago"
3. Calculates wait duration (e.g., 2h 15m 30s)
4. Waits until exactly 3pm Santiago time
5. Sends the configured command
6. Resumes monitoring

**Logs:**
```
INFO Matched rule 'timeout_handler' (wait_for_time): 3pm (America/Santiago)
INFO Parsed reset time: 2026-01-19 15:00:00 -03 (in 2h 15m 30s)
INFO Waiting until 2026-01-19 15:00:00 -03
INFO Waiting... 50% complete, 1h 7m 45s remaining
INFO Sending command: 'continue'
INFO Command sent successfully!
```

### Build Pipeline Automation

**Terminal shows:**
```
Build succeeded in 42s
```

**Wez Expect:**
1. Matches conditional rule
2. Tests "succeeded" against condition pattern
3. Condition passes → triggers deployment
4. Logs deployment notification

**Config:**
```toml
[[rules]]
pattern = "Build (succeeded|failed) in (\\d+)s"
action_type = "conditional"
condition_pattern = "succeeded"
then_command = "./deploy.sh\n"
else_command = "echo 'Skipping deployment'\n"
apply_to_capture = 1

[[rules]]
pattern = "Deployed to (\\w+)"
action_type = "external_command"
external_command = "notify-send 'Deployed to $1'"
```

## Advanced Configuration

### Multiple Rules with Priority

Rules are evaluated in order. First enabled match wins:

```toml
# High priority: specific error handling
[[rules]]
name = "critical_error"
pattern = "CRITICAL: (.*)"
action_type = "external_command"
external_command = "pagerduty-alert '$1'"

# Medium priority: general error
[[rules]]
name = "error_handler"
pattern = "ERROR: (.*)"
action_type = "immediate"
command = "retry\n"

# Low priority: any prompt
[[rules]]
name = "generic_prompt"
pattern = "Press any key"
action_type = "immediate"
command = "\n"
enabled = false  # Disabled by default
```

### Regex Pattern Tips

**TOML String Escaping:**
```toml
# Use double quotes with escaped backslashes
pattern = "\\d{2}:\\d{2}"

# Or use single quotes (backslashes are literal)
pattern = '\d{2}:\d{2}'
```

**Testing Patterns:**
- Use [regex101.com](https://regex101.com) to test your patterns
- Ensure `wait_for_time` patterns have exactly 2 capture groups
- Use non-capturing groups `(?:...)` for optional parts

## Development

```bash
# Run tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Check code without building
cargo check

# Build with optimizations
cargo build --release

# Clean build artifacts
cargo clean

# Generate and open documentation
cargo doc --open
```

## Project Structure

```
wez_expect/
├── Cargo.toml              # Dependencies and metadata
├── config.toml             # User configuration
├── src/
│   ├── lib.rs             # Public library API
│   ├── main.rs            # Binary entry point
│   ├── action/            # Action system
│   │   ├── mod.rs         # Action trait definition
│   │   ├── factory.rs     # Action factory
│   │   └── builtin/       # Built-in actions
│   │       ├── wait_for_time.rs
│   │       ├── immediate.rs
│   │       ├── conditional.rs
│   │       └── external_command.rs
│   ├── config.rs          # Configuration loading
│   ├── monitor.rs         # Terminal monitoring
│   ├── pane.rs            # WezTerm pane discovery
│   ├── parser.rs          # Time/timezone parsing
│   ├── scheduler.rs       # Async timing
│   └── sender.rs          # Command sending
├── CONFIG_SCHEMA.md        # Configuration reference
└── README.md               # This file
```

## Troubleshooting

### "No WezTerm panes found"
Ensure WezTerm is running before starting Wez Expect.

### Pattern doesn't match
- Check capture group count for `wait_for_time` (must be exactly 2)
- Test regex at [regex101.com](https://regex101.com)
- Verify actual terminal message format
- Check logs for pattern matching attempts

### Wrong pane monitored
```bash
wezterm cli list  # Find correct pane ID
```

Then specify in config:
```toml
[pane]
pane_id = 2
```

### Windows build errors
- **"linker `link.exe` not found"**: Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/)
- **Git Bash errors**: Use PowerShell or CMD

### Command not working
- Verify command syntax (include `\n` for Enter key)
- Check WezTerm pane is responding
- Review logs for send errors
- Try manual command: `wezterm cli send-text --pane-id <id> "command\n"`

## Documentation

- **[Configuration Schema](CONFIG_SCHEMA.md)** - Complete configuration reference

## Contributing

Contributions welcome! Areas for improvement:
- Additional built-in actions
- Platform-specific features
- Documentation improvements
- Test coverage expansion

## License

Open source - free to use, modify, and distribute.

## Resources

- **WezTerm CLI**: https://wezterm.org/cli/
- **Rust Book**: https://doc.rust-lang.org/book/
- **IANA Timezones**: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones
- **Regex Testing**: https://regex101.com/
