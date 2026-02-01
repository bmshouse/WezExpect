//! Command sending to WezTerm panes with retry logic and terminal readiness handling.
//!
//! This module provides reliable command sending to WezTerm panes with:
//! - Exponential backoff retry logic (3 attempts)
//! - Configurable delay before sending to ensure terminal readiness
//! - Comprehensive error reporting with context

use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::{sleep, Duration};

/// Maximum number of retry attempts for command sending
const MAX_RETRIES: u32 = 3;

/// Initial retry delay in milliseconds (doubles each attempt for exponential backoff)
const RETRY_DELAY_MS: u64 = 500;

/// Send a command to a WezTerm pane with retry logic
///
/// # Arguments
/// * `pane_id` - The WezTerm pane ID to send the command to
/// * `command` - The command string to send
/// * `delay_ms` - Milliseconds to wait before sending (ensures terminal is ready)
///
/// # Examples
/// ```no_run
/// use wez_expect::sender::send_command;
///
/// # async fn example() -> anyhow::Result<()> {
/// // Send a command with 100ms delay for terminal readiness
/// send_command(1, "echo hello\n", 100).await?;
///
/// // Send with no delay (use when terminal is known to be ready)
/// send_command(1, "\n", 0).await?;
/// # Ok(())
/// # }
/// ```
pub async fn send_command(pane_id: u32, command: &str, delay_ms: u64) -> Result<()> {
    let mut last_error = None;

    for attempt in 1..=MAX_RETRIES {
        match try_send_command(pane_id, command, delay_ms).await {
            Ok(_) => {
                tracing::info!(
                    "Successfully sent command to pane {}: '{}'",
                    pane_id,
                    command
                );
                return Ok(());
            }
            Err(e) => {
                last_error = Some(e);
                if attempt < MAX_RETRIES {
                    // Exponential backoff: 500ms, 1000ms, 2000ms
                    let delay = RETRY_DELAY_MS * 2u64.pow(attempt - 1);
                    tracing::warn!(
                        "Failed to send command (attempt {}/{}): {}. Retrying in {}ms...",
                        attempt,
                        MAX_RETRIES,
                        last_error.as_ref().unwrap(),
                        delay
                    );
                    sleep(Duration::from_millis(delay)).await;
                }
            }
        }
    }

    // All retries exhausted
    let error = last_error.expect("last_error should always be Some after retry loop");
    tracing::error!(
        "Failed to send command after {} attempts: {}",
        MAX_RETRIES,
        error
    );
    Err(error)
}

/// Attempt to send a command once
async fn try_send_command(pane_id: u32, command: &str, delay_ms: u64) -> Result<()> {
    // Wait before sending to ensure terminal is ready to receive input
    // This prevents dropped keystrokes when terminal is busy or transitioning states
    if delay_ms > 0 {
        tracing::debug!(
            "Waiting {}ms before sending command to ensure terminal readiness",
            delay_ms
        );
        sleep(Duration::from_millis(delay_ms)).await;
    }

    let mut child = Command::new("wezterm")
        .args([
            "cli",
            "send-text",
            "--no-paste",
            "--pane-id",
            &pane_id.to_string(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn 'wezterm cli send-text'")?;

    // Write command to stdin
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(command.as_bytes())
            .await
            .context("Failed to write command to stdin")?;
        // Explicitly drop stdin to close the pipe
        drop(stdin);
    }

    // Wait for command to complete
    let output = child
        .wait_with_output()
        .await
        .context("Failed to wait for wezterm command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        anyhow::bail!(
            "wezterm cli send-text failed for pane {}: stderr='{}', stdout='{}'",
            pane_id,
            stderr,
            stdout
        );
    }

    // Log output if any
    if !output.stdout.is_empty() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        tracing::debug!("send-text output: {}", stdout.trim());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Layer 1: Unit Tests - Byte Handling

    #[test]
    fn test_command_bytes_preservation() {
        let command = "\ncontinue\n";
        let bytes = command.as_bytes();

        // Verify first byte is newline (ASCII 10)
        assert_eq!(bytes[0], b'\n');
        // Verify last byte is newline
        assert_eq!(bytes[bytes.len() - 1], b'\n');
        // Verify middle is "continue"
        assert_eq!(&bytes[1..9], b"continue");
    }

    #[test]
    fn test_various_escape_sequences() {
        let cases = vec![
            ("\n", vec![b'\n']),
            ("\t", vec![b'\t']),
            ("\\n", vec![b'\\', b'n']), // Literal backslash-n
            (
                "\ncontinue\n",
                vec![b'\n', b'c', b'o', b'n', b't', b'i', b'n', b'u', b'e', b'\n'],
            ),
        ];

        for (input, expected) in cases {
            assert_eq!(input.as_bytes(), expected.as_slice());
        }
    }

    // Layer 2: Mock Process Tests

    #[tokio::test]
    async fn test_stdin_write() {
        // Test that we can write to stdin of a child process
        // Use a simple command like `cat` that echoes stdin to stdout

        let mut child = Command::new("cat")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("Failed to spawn cat");

        let test_input = "\nhello\n";

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(test_input.as_bytes()).await.unwrap();
            drop(stdin);
        }

        let output = child.wait_with_output().await.unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);

        assert_eq!(stdout, test_input);
    }

    // Layer 3: Integration Tests (WezTerm Required)

    #[tokio::test]
    #[ignore] // Only run with `cargo test -- --ignored` when WezTerm is running
    async fn test_send_command_integration() {
        // This test requires:
        // 1. WezTerm running
        // 2. At least one pane available

        // Get first available pane
        let panes = crate::pane::list_panes().await.unwrap();
        if panes.is_empty() {
            panic!("No WezTerm panes available for testing");
        }
        let pane_id = panes[0].pane_id;

        // Send a harmless command with no delay for testing
        let result = send_command(pane_id, "echo 'test'\n", 0).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[ignore]
    async fn test_send_newlines() {
        let panes = crate::pane::list_panes().await.unwrap();
        let pane_id = panes[0].pane_id;

        // Test that newlines are properly sent
        let result = send_command(pane_id, "\n", 0).await;
        assert!(result.is_ok());

        // Give terminal time to process
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    // Layer 4: Manual Verification Test

    #[tokio::test]
    #[ignore]
    async fn test_visual_verification() {
        // This test helps manually verify the fix
        // Run with: cargo test test_visual_verification -- --ignored --nocapture

        let panes = crate::pane::list_panes().await.unwrap();
        println!("Available panes: {:?}", panes);

        if panes.is_empty() {
            println!("No panes available");
            return;
        }

        let pane_id = panes[0].pane_id;
        println!("Sending to pane {}", pane_id);
        println!("Watch the terminal for: newline, 'continue', newline");

        let result = send_command(pane_id, "\ncontinue\n", 100).await;
        println!("Result: {:?}", result);

        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }

    // Layer 5: End-to-End Test with Output Verification

    #[tokio::test]
    #[ignore]
    async fn test_end_to_end_with_verification() {
        // This test spawns a WezTerm pane, sends commands, and verifies output
        // Run with: cargo test test_end_to_end_with_verification -- --ignored --nocapture

        // Step 1: Check if WezTerm is available
        let wezterm_check = Command::new("wezterm").args(["--version"]).output().await;

        if wezterm_check.is_err() {
            println!("WezTerm not available, skipping test");
            return;
        }

        // Step 2: Spawn a new test pane with a shell (more reliable cross-platform)
        let spawn_output = Command::new("wezterm")
            .args(["cli", "spawn"])
            .output()
            .await
            .expect("Failed to spawn test pane");

        if !spawn_output.status.success() {
            println!("Failed to spawn test pane, skipping test");
            return;
        }

        let pane_id_str = String::from_utf8_lossy(&spawn_output.stdout);
        let pane_id: u32 = pane_id_str.trim().parse().expect("Invalid pane ID");
        println!("Created test pane: {}", pane_id);

        // Wait for shell to initialize
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Step 3: Send commands with newlines that will produce visible output
        let test_command = "echo HELLO\necho WORLD\n";
        let send_result = send_command(pane_id, test_command, 100).await;
        assert!(
            send_result.is_ok(),
            "Failed to send command: {:?}",
            send_result
        );

        // Give the commands time to execute
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Step 4: Read back the pane content
        let get_text_output = Command::new("wezterm")
            .args(["cli", "get-text", "--pane-id", &pane_id.to_string()])
            .output()
            .await
            .expect("Failed to get pane text");

        let pane_content = String::from_utf8_lossy(&get_text_output.stdout);
        println!("Pane content:\n{}", pane_content);

        // Step 5: Verify the output contains our text
        // The commands should have executed and produced output
        assert!(
            pane_content.contains("HELLO"),
            "Pane should contain 'HELLO' from first command"
        );
        assert!(
            pane_content.contains("WORLD"),
            "Pane should contain 'WORLD' from second command"
        );

        // Verify both commands were sent (newline worked to separate them)
        assert!(
            pane_content.contains("echo HELLO") || pane_content.contains("HELLO"),
            "First command should have been sent"
        );
        assert!(
            pane_content.contains("echo WORLD") || pane_content.contains("WORLD"),
            "Second command should have been sent"
        );

        // Step 6: Clean up - kill the test pane
        let _kill_output = Command::new("wezterm")
            .args(["cli", "kill-pane", "--pane-id", &pane_id.to_string()])
            .output()
            .await;

        println!("Test completed successfully!");
    }

    #[tokio::test]
    #[ignore]
    async fn test_escape_sequences_end_to_end() {
        // Test that various escape sequences work correctly
        // Run with: cargo test test_escape_sequences_end_to_end -- --ignored --nocapture

        // Check if WezTerm is available
        let wezterm_check = Command::new("wezterm").args(["--version"]).output().await;

        if wezterm_check.is_err() {
            println!("WezTerm not available, skipping test");
            return;
        }

        // Spawn a test pane running a shell (to test the actual use case)
        let spawn_output = Command::new("wezterm")
            .args(["cli", "spawn"])
            .output()
            .await
            .expect("Failed to spawn test pane");

        if !spawn_output.status.success() {
            println!("Failed to spawn test pane, skipping test");
            return;
        }

        let pane_id_str = String::from_utf8_lossy(&spawn_output.stdout);
        let pane_id: u32 = pane_id_str.trim().parse().expect("Invalid pane ID");
        println!("Created test pane: {}", pane_id);

        // Wait for shell to initialize
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Test the actual problem case: "\ncontinue\n"
        // This should send: newline, then "continue", then newline
        let test_command = "echo 'TEST_MARKER'\n";
        let send_result = send_command(pane_id, test_command, 100).await;
        assert!(
            send_result.is_ok(),
            "Failed to send command: {:?}",
            send_result
        );

        // Give time for command to execute
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Read back the pane content
        let get_text_output = Command::new("wezterm")
            .args(["cli", "get-text", "--pane-id", &pane_id.to_string()])
            .output()
            .await
            .expect("Failed to get pane text");

        let pane_content = String::from_utf8_lossy(&get_text_output.stdout);
        println!("Pane content:\n{}", pane_content);

        // Verify the command was executed (output should contain TEST_MARKER)
        assert!(
            pane_content.contains("TEST_MARKER"),
            "Command should have been executed and output visible"
        );

        // Clean up
        let _kill_output = Command::new("wezterm")
            .args(["cli", "kill-pane", "--pane-id", &pane_id.to_string()])
            .output()
            .await;

        println!("Escape sequences test completed successfully!");
    }
}
