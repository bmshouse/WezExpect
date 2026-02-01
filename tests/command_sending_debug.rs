#[cfg(test)]
mod command_sending_investigation {
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;
    use tokio::process::Command;

    /// Test sending the exact command pattern from config: "\ncontinue\n"
    #[tokio::test]
    #[ignore] // Run with: cargo test command_format_variations -- --ignored --nocapture
    async fn test_command_format_variations() {
        // Spawn a test pane
        let pane_id = create_test_pane().await;

        println!("Created test pane: {}", pane_id);
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Test variations of the command
        let test_cases = vec![
            ("\\ncontinue\\n", "\ncontinue\n"),
            ("continue\\n (no leading)", "continue\n"),
            ("\\n\\n (just newlines)", "\n\n"),
            ("echo TEST\\n", "echo TEST\n"),
        ];

        for (label, cmd) in test_cases {
            println!("\n=== Testing: {} ===", label);

            // Send command with --no-paste
            let result = send_with_no_paste(&pane_id, cmd).await;
            println!("Send result: {:?}", result);

            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

            // Read back content
            let content = get_pane_content(&pane_id).await;
            println!("Pane content after send:\n{}", content);
            println!("=== End {} ===\n", label);

            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }

        // Cleanup
        cleanup_pane(&pane_id).await;
    }

    /// Test if adding delay before send helps
    #[tokio::test]
    #[ignore]
    async fn test_timing_delay_impact() {
        let pane_id = create_test_pane().await;

        println!("\n=== Test 1: Immediate send (no delay) ===");
        send_with_no_paste(&pane_id, "\ncontinue\n").await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        println!("Content: {}", get_pane_content(&pane_id).await);

        println!("\n=== Test 2: Send with 100ms delay ===");
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        send_with_no_paste(&pane_id, "\ncontinue\n").await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        println!("Content: {}", get_pane_content(&pane_id).await);

        println!("\n=== Test 3: Send with 500ms delay ===");
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        send_with_no_paste(&pane_id, "\ncontinue\n").await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        println!("Content: {}", get_pane_content(&pane_id).await);

        cleanup_pane(&pane_id).await;
    }

    /// Test sending visible text to observe command registration
    #[tokio::test]
    #[ignore]
    async fn test_visible_text_sending() {
        let pane_id = create_test_pane().await;
        println!("Created test pane (cat process): {}", pane_id);

        // Send visible text that should appear immediately
        println!("\n=== Sending 'Hello World' ===");
        send_with_no_paste(&pane_id, "Hello World").await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let content = get_pane_content(&pane_id).await;
        println!("Content after 'Hello World':\n{}", content);

        // Send text with newline
        println!("\n=== Sending 'Test Line\\n' ===");
        send_with_no_paste(&pane_id, "Test Line\n").await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let content = get_pane_content(&pane_id).await;
        println!("Content after 'Test Line\\n':\n{}", content);

        // Send text with leading newline (the problematic pattern)
        println!("\n=== Sending '\\nWith Leading Newline\\n' ===");
        send_with_no_paste(&pane_id, "\nWith Leading Newline\n")
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let content = get_pane_content(&pane_id).await;
        println!("Content after '\\nWith Leading Newline\\n':\n{}", content);

        cleanup_pane(&pane_id).await;
    }

    /// Test sending without --no-paste flag
    #[tokio::test]
    #[ignore]
    async fn test_paste_vs_no_paste() {
        let pane_id = create_test_pane().await;

        // Test with --no-paste (current)
        println!("=== With --no-paste ===");
        send_with_flag(&pane_id, "\ncontinue\n", true)
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
        println!("Content: {}", get_pane_content(&pane_id).await);

        // Test without --no-paste
        println!("\n=== Without --no-paste ===");
        send_with_flag(&pane_id, "\ncontinue\n", false)
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
        println!("Content: {}", get_pane_content(&pane_id).await);

        cleanup_pane(&pane_id).await;
    }

    // Helper functions

    async fn create_test_pane() -> String {
        // Spawn a pane with a long-running command to keep it alive
        // Using 'cat' which waits for input indefinitely
        let spawn_output = Command::new("wezterm")
            .args(["cli", "spawn", "--", "cat"])
            .output()
            .await
            .expect("Failed to spawn test pane");

        let pane_id_str = String::from_utf8_lossy(&spawn_output.stdout);
        let pane_id = pane_id_str.trim().to_string();

        // Give the pane time to initialize
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        pane_id
    }

    /// Send command exactly as sender.rs does
    async fn send_with_no_paste(pane_id: &str, command: &str) -> Result<(), String> {
        let mut child = Command::new("wezterm")
            .args(["cli", "send-text", "--no-paste", "--pane-id", pane_id])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Spawn failed: {}", e))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(command.as_bytes())
                .await
                .map_err(|e| format!("Write failed: {}", e))?;
            drop(stdin);
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| format!("Wait failed: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "Command failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(())
    }

    async fn send_with_flag(pane_id: &str, command: &str, no_paste: bool) -> Result<(), String> {
        let mut args = vec!["cli", "send-text"];
        if no_paste {
            args.push("--no-paste");
        }
        args.extend(&["--pane-id", pane_id]);

        let mut child = Command::new("wezterm")
            .args(&args)
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| format!("Spawn failed: {}", e))?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(command.as_bytes()).await.unwrap();
            drop(stdin);
        }

        child.wait().await.unwrap();
        Ok(())
    }

    async fn get_pane_content(pane_id: &str) -> String {
        let output = Command::new("wezterm")
            .args(["cli", "get-text", "--pane-id", pane_id])
            .output()
            .await
            .expect("Failed to get pane text");

        String::from_utf8_lossy(&output.stdout).to_string()
    }

    async fn cleanup_pane(pane_id: &str) {
        let _ = Command::new("wezterm")
            .args(["cli", "kill-pane", "--pane-id", pane_id])
            .output()
            .await;
    }
}
