use anyhow::{Context, Result};

pub fn run_command(command: &str) -> Result<String> {
    if command.trim().is_empty() {
        return Err(anyhow::anyhow!("Command cannot be empty"));
    }

    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .context("Failed to start shell command")?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if output.status.success() {
        Ok(stdout)
    } else {
        Err(anyhow::anyhow!("Command failed with code {}\nStderr: {}", 
            output.status.code().unwrap_or(-1), stderr))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_echo_command() {
        let result = run_command("echo 'Talon test successful'");
        assert!(result.is_ok());
        assert!(result.unwrap().contains("Talon test successful"));
    }

    #[test]
    fn test_command_with_output() {
        let result = run_command("echo 'Line 1\nLine 2\nLine 3'");
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("Line 1"));
        assert!(output.contains("Line 3"));
    }

    #[test]
    fn test_failing_command() {
        let result = run_command("ls /non/existent/path/that/should/fail 2>&1");
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_command() {
        let result = run_command("");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_whitespace_command() {
        let result = run_command("   ");
        assert!(result.is_err());
    }
}
