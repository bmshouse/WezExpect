# Configuration Schema Reference

Complete reference for all configuration options in Wez Expect.

## Table of Contents

- [Configuration File Format](#configuration-file-format)
- [Global Settings](#global-settings)
- [Action Types](#action-types)
- [Configuration Examples](#configuration-examples)
- [Validation Rules](#validation-rules)

## Configuration File Format

Wez Expect uses TOML for configuration. The configuration file (`config.toml` by default) has these main sections:

```toml
[monitor]    # Monitoring settings
[pane]       # WezTerm pane selection
[[rules]]    # Pattern matching rules (array)
```

## Global Settings

### `[monitor]` Section

Controls how Wez Expect polls and monitors the terminal.

```toml
[monitor]
poll_interval_secs = 30  # How often to check patterns (integer, seconds)
```

**Fields:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `poll_interval_secs` | integer | 3 | Seconds between pattern checks (1-300) |

**Validation:**
- Must be > 0
- Warning if > 300 (considered too long)

**Example:**
```toml
[monitor]
poll_interval_secs = 5  # Check every 5 seconds
```

### `[pane]` Section

Specifies which WezTerm pane to monitor.

```toml
[pane]
pane_id = 1  # Optional: specific pane ID
```

**Fields:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `pane_id` | integer | auto-detect | WezTerm pane ID to monitor |

**Auto-detection:**
- If omitted, selects first available pane
- Use `wezterm cli list` to find pane IDs

**Example:**
```toml
[pane]
pane_id = 2  # Monitor pane #2

# Or omit to auto-detect
[pane]
# Auto-detects first available pane
```

## Action Types

### Rule Structure

All rules share these common fields:

```toml
[[rules]]
name = "rule_name"           # Optional: descriptive name for logging
pattern = "regex pattern"    # Required: regex to match
action_type = "type"         # Required: action type identifier
enabled = true               # Optional: enable/disable rule (default: true)
# ... action-specific fields ...
```

**Common Fields:**

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `name` | string | No | "rule #N" | Descriptive name for logging |
| `pattern` | string | Yes | - | Regex pattern to match |
| `action_type` | string | Yes | - | Type of action to perform |
| `enabled` | boolean | No | true | Whether this rule is active |

**Rule Evaluation:**
- Rules are checked in order (top to bottom)
- First enabled match wins
- Disabled rules are skipped

### 1. Wait For Time Action

**Type:** `wait_for_time`

Extracts time and timezone from terminal output, waits until that time, then sends a command.

**Required Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `pattern` | string | Regex with exactly 2 capture groups: (time) (timezone) |
| `command` | string | Command to send after waiting |

**Pattern Requirements:**
- **Must have exactly 2 capture groups**
- Capture group 1: Time string (e.g., "3pm", "11:30am")
- Capture group 2: Timezone string (e.g., "America/Santiago", "UTC")

**Supported Time Formats:**
- `3pm` → 15:00
- `11:30am` → 11:30
- `12:00pm` → 12:00 (noon)
- `12:00am` → 00:00 (midnight)
- `1am`, `11pm`, etc.

**Supported Timezones:**
- Any IANA timezone (e.g., `America/New_York`, `Europe/London`, `UTC`)
- Abbreviations like `EST`, `PST` (if parseable)

**Behavior:**
1. Matches pattern and extracts time + timezone
2. Parses time into target datetime
3. If time already passed today, schedules for tomorrow
4. Waits until target time (with progress logging for long waits)
5. Sends command
6. Resumes monitoring

**Example:**
```toml
[[rules]]
name = "api_timeout_handler"
pattern = "Session expires at (\\d{1,2}:\\d{2}\\s?(?:am|pm)) \\((.*?)\\)"
action_type = "wait_for_time"
command = "refresh\n"
enabled = true
```

**Matches:**
- Input: `"Session expires at 3pm (America/Santiago)"`
- Extracts: time="3pm", timezone="America/Santiago"
- Waits until 3pm Santiago time
- Sends: `refresh\n`

### 2. Immediate Action

**Type:** `immediate`

Sends a command instantly when pattern matches.

**Required Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `pattern` | string | Regex pattern (no capture group requirements) |
| `command` | string | Command to send immediately |

**Pattern Requirements:**
- No capture group requirements
- Can be simple string or regex

**Behavior:**
1. Matches pattern
2. Sends command immediately
3. Waits for terminal content to change
4. Resumes monitoring

**Example:**
```toml
[[rules]]
name = "press_enter"
pattern = "Press Enter to continue"
action_type = "immediate"
command = "\n"
enabled = true
```

**Matches:**
- Input: `"Press Enter to continue"`
- Sends: `\n` (Enter key) immediately

**Common Use Cases:**
- Auto-respond to prompts
- Send Enter/Space keys
- Auto-confirm dialogs

### 3. Conditional Action

**Type:** `conditional`

Tests a captured group against a condition and sends different commands based on the result.

**Required Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `pattern` | string | Regex with capture groups |
| `condition_pattern` | string | Regex to test against captured group |
| `then_command` | string | Command if condition matches |
| `else_command` | string | Command if condition doesn't match |

**Optional Fields:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `apply_to_capture` | integer | 1 | Which capture group to test (1-indexed) |

**Pattern Requirements:**
- Must have at least 1 capture group
- `apply_to_capture` must reference a valid capture group

**Behavior:**
1. Matches pattern and extracts capture groups
2. Gets capture group specified by `apply_to_capture`
3. Tests it against `condition_pattern`
4. If condition matches → sends `then_command`
5. If condition fails → sends `else_command`
6. Resumes monitoring

**Example:**
```toml
[[rules]]
name = "build_pipeline"
pattern = "Build (succeeded|failed) in (\\d+)s"
action_type = "conditional"
condition_pattern = "succeeded"
then_command = "deploy\n"
else_command = "echo 'Build failed, skipping deployment'\n"
apply_to_capture = 1
enabled = true
```

**Matches:**
- Input: `"Build succeeded in 42s"`
- Tests capture group 1 ("succeeded") against "succeeded"
- Condition matches → sends `deploy\n`

- Input: `"Build failed in 10s"`
- Tests capture group 1 ("failed") against "succeeded"
- Condition fails → sends `echo 'Build failed, skipping deployment'\n`

**Common Use Cases:**
- Build status branching
- Error vs success handling
- State-based automation

### 4. External Command Action

**Type:** `external_command`

Executes shell commands instead of sending to terminal. Supports capture group substitution.

**Required Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `pattern` | string | Regex (can have capture groups) |
| `external_command` | string | Shell command with $1, $2, etc. substitution |

**Optional Fields:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `working_dir` | string | current dir | Working directory for command execution |
| `timeout_secs` | integer | 30 | Command timeout in seconds (1-600) |
| `env` | table | {} | Environment variables (key-value pairs) |

**Pattern Requirements:**
- No capture group requirements
- Capture groups available as `$1`, `$2`, etc. in command

**Capture Substitution:**
- `$1` → First capture group
- `$2` → Second capture group
- `$N` → Nth capture group

**Platform Commands:**
- Windows: `cmd /C <command>`
- Linux/macOS: `sh -c <command>`

**Behavior:**
1. Matches pattern and extracts capture groups
2. Substitutes `$1`, `$2`, etc. in command
3. Executes command in shell with timeout
4. Logs stdout/stderr
5. Fails if exit code != 0
6. Resumes monitoring

**Example:**
```toml
[[rules]]
name = "deployment_logger"
pattern = "Deployed to (\\w+) in (\\d+) seconds"
action_type = "external_command"
external_command = "echo 'Deployment to $1 took $2s' >> /var/log/deploy.log"
working_dir = "/var/log"
timeout_secs = 10
enabled = true
```

**With environment variables:**
```toml
[[rules]]
pattern = "Process (\\w+)"
action_type = "external_command"
external_command = "python notify.py $1"
[rules.env]
NOTIFY_TOKEN = "secret"
NOTIFY_CHANNEL = "#deploys"
```

**Matches:**
- Input: `"Deployed to production in 120 seconds"`
- Executes: `echo 'Deployment to production took 120s' >> /var/log/deploy.log`
- Working directory: `/var/log`
- Timeout: 10 seconds

**⚠️ Security Warning:**
This executes arbitrary shell commands. Ensure:
- Patterns are carefully designed
- No untrusted input in patterns
- No command injection vulnerabilities

**Common Use Cases:**
- Logging to files
- Triggering webhooks/APIs
- Running scripts
- System notifications
- Integration with external tools

## Configuration Examples

### Minimal Configuration

```toml
[[rules]]
pattern = "Press Enter"
action_type = "immediate"
command = "\n"
```

### Full Configuration

```toml
[monitor]
poll_interval_secs = 30

[pane]
pane_id = 2

[[rules]]
name = "timeout_handler"
pattern = "resets (\\d{1,2}(?::\\d{2})?\\s?(?:am|pm)) \\((.*?)\\)"
action_type = "wait_for_time"
command = "continue\n"
enabled = true

[[rules]]
name = "build_status"
pattern = "Build (succeeded|failed)"
action_type = "conditional"
condition_pattern = "succeeded"
then_command = "deploy\n"
else_command = "echo 'Failed'\n"
apply_to_capture = 1
enabled = true

[[rules]]
name = "logger"
pattern = "Event: (.*)"
action_type = "external_command"
external_command = "echo '$1' >> events.log"
timeout_secs = 5
enabled = true
```

### Multiple Rules with Priority

```toml
# High priority: Critical errors
[[rules]]
name = "critical_error"
pattern = "CRITICAL: (.*)"
action_type = "external_command"
external_command = "alert-system '$1'"
enabled = true

# Medium priority: General errors
[[rules]]
name = "error"
pattern = "ERROR: (.*)"
action_type = "immediate"
command = "retry\n"
enabled = true

# Low priority: Warnings
[[rules]]
name = "warning"
pattern = "WARN: (.*)"
action_type = "external_command"
external_command = "echo '$1' >> warnings.log"
enabled = false  # Disabled
```

## Validation Rules

### Configuration Validation

Wez Expect validates configuration at startup:

1. **Global Settings:**
   - `poll_interval_secs` > 0
   - `poll_interval_secs` ≤ 300 (warning if exceeded)

2. **Rules:**
   - At least one rule defined
   - At least one enabled rule
   - All patterns are valid regex
   - Unknown action types rejected

3. **Action-Specific:**
   - Each action validates its own required fields
   - Capture group requirements checked
   - Field types validated

### Pattern Validation

**Regex Compilation:**
- All patterns compiled at startup
- Invalid regex causes error with rule name

**Capture Group Requirements:**
- `wait_for_time`: Exactly 2 capture groups
- `conditional`: At least 1 capture group
- `immediate`: No requirements
- `external_command`: No requirements (but $N available)

**Testing Patterns:**
Use [regex101.com](https://regex101.com) to test patterns before adding to config.

### TOML Syntax

**String Escaping:**
```toml
# Double quotes: escape backslashes
pattern = "\\d{2}:\\d{2}"

# Single quotes: backslashes are literal
pattern = '\d{2}:\d{2}'
```

**Common Patterns:**
```toml
# Match digits
pattern = "\\d+"       # or '\d+'

# Match words
pattern = "\\w+"       # or '\w+'

# Capture groups
pattern = "(\\d+) (\\w+)"  # or '(\d+) (\w+)'

# Optional parts (non-capturing)
pattern = "(?:optional )?required"
```

## Error Messages

### Common Validation Errors

**"Config must have at least one rule"**
- Add at least one `[[rules]]` section

**"At least one rule must be enabled"**
- Set `enabled = true` on at least one rule

**"Invalid regex in rule #1"**
- Check pattern syntax with regex tester
- Ensure proper escaping in TOML

**"rule #1 has action 'wait_for_time' but pattern has 1 capture groups (need exactly 2)"**
- `wait_for_time` requires exactly 2 capture groups
- Add timezone capture group: `pattern = "time (\\d+pm) (UTC)"`

**"Unknown action type: 'xyz'"**
- Check `action_type` spelling
- Must be one of: wait_for_time, immediate, conditional, external_command

**"wait_for_time action requires a non-empty 'command' field"**
- Add `command` field to rule
- Ensure command is not empty/whitespace

## Command Syntax

### Terminal Commands

Commands sent to terminal can include:
- `\n` - Enter key
- `\t` - Tab key
- Regular text
- Escape sequences supported by WezTerm

**Examples:**
```toml
command = "\n"              # Press Enter
command = "yes\n"           # Type "yes" and Enter
command = "continue\n"      # Type "continue" and Enter
command = "\t\t\n"          # Tab twice, then Enter
```

### External Commands

Shell commands executed by `external_command` action:
- Platform-specific (cmd on Windows, sh on Linux/macOS)
- Capture substitution with `$1`, `$2`, etc.
- Standard shell syntax

**Examples:**
```toml
external_command = "echo 'message' >> log.txt"
external_command = "curl -X POST https://api.example.com/notify -d '$1'"
external_command = "python script.py --arg '$1'"
```

## Configuration File Location

**Default:** `config.toml` in current directory

**Custom location:**
```bash
wez_expect --config /path/to/config.toml
```

**Recommended locations:**
- Development: `./config.toml` (project directory)
- User: `~/.config/wez-expect/config.toml`
- System: `/etc/wez-expect/config.toml`

## See Also

- [README.md](README.md) - Getting started and usage
