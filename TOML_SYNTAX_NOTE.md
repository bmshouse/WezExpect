# TOML Syntax for Regex Patterns

## Important: Use Single Quotes for Regex!

In TOML configuration files, **use single quotes `'...'` for regex patterns**, not Rust's `r#"..."#` syntax.

### ✅ Correct (TOML)
```toml
timeout_pattern = 'Your limit will reset at (\d{1,2}(?::\d{2})?\s?(?:am|pm)) \((.*?)\)\.'
```

### ❌ Wrong (This is Rust syntax, not TOML)
```toml
timeout_pattern = r#"Your limit will reset at (\d{1,2}(?::\d{2})?\s?(?:am|pm)) \((.*?)\)\."#
```

## Why Single Quotes?

In TOML:
- **Single quotes `'...'`** = Literal strings (like raw strings in other languages)
  - No escape sequences processed
  - Backslashes are literal
  - Perfect for regex patterns!
  - **LIMITATION**: Cannot contain single quotes (apostrophes)

- **Double quotes `"..."`** = Regular strings
  - Escape sequences are processed (`\n`, `\t`, `\\`, etc.)
  - Backslashes must be escaped as `\\`
  - Use when your pattern contains apostrophes

## Examples

### Good Examples

```toml
# Simple pattern
timeout_pattern = 'Timeout at (\d+) seconds'

# Pattern with special characters
timeout_pattern = 'Session expires: (\d{1,2}:\d{2}) \((.*?)\)'

# Pattern with backslashes
timeout_pattern = 'Error: \[(\w+)\] - (.*)'
```

### When You Must Use Double Quotes

**Use double quotes when your pattern contains an apostrophe:**

```toml
# Pattern with apostrophe - MUST use double quotes
timeout_pattern = "You've hit your limit · resets (\\d{1,2}(?::\\d{2})?\\s?(?:am|pm)) \\((.*?)\\)"

# Simple pattern without apostrophes - can use single quotes
timeout_pattern = 'Error: \[(\w+)\]'
```

**Remember**: With double quotes, ALL backslashes must be escaped as `\\`:

```toml
# Single quotes - backslashes are literal
timeout_pattern = 'reset at (\d+) seconds'

# Double quotes - backslashes must be doubled
timeout_pattern = "reset at (\\d+) seconds"
```

## TOML String Types

| Type | Syntax | Use Case |
|------|--------|----------|
| Literal string | `'...'` | **Regex patterns** (recommended) |
| Basic string | `"..."` | Normal text with escape sequences |
| Multi-line literal | `'''...'''` | Long regex patterns |
| Multi-line basic | `"""..."""` | Long text with escapes |

## Common Error

```
Error: Failed to parse config file as TOML

Caused by:
    TOML parse error at line 10, column 19
       |
    10 | timeout_pattern = r#"..."#
       |                   ^
    invalid string
    expected `"`, `'`
```

**Solution:** Change `r#"..."#` to `'...'`

## Quick Reference

```toml
# ✅ Use this for regex patterns
timeout_pattern = 'pattern here'

# Regular text can use double quotes
custom_command = "echo 'hello'"

# Comments start with #
# This is a comment
```

## More Info

- TOML Spec: https://toml.io/
- Regex Testing: https://regex101.com/
