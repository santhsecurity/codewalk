//! Code protocol sandbox execution helpers.

use std::collections::HashMap;
use std::time::Duration;
use tokio::process::Command;

/// # Errors
/// Returns an error if execution fails or times out.
pub async fn execute_script(
    engine: &str,
    code: &str,
    target: &str,
    template_id: &str,
    variables: &HashMap<String, String>,
    timeout_dur: Duration,
) -> std::io::Result<String> {
    let allowed_engines = [
        "bash",
        "sh",
        "python",
        "python3",
        "ruby",
        "node",
        "perl",
        "powershell",
    ];
    if !allowed_engines.contains(&engine) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("engine '{engine}' is not in the allowlist"),
        ));
    }

    let substituted = substitute_target_vars(code, target, variables);

    let (mut interpreter, mut run_args) = match engine {
        "python" | "python3" => ("python3".to_string(), vec!["-c".to_string(), substituted]),
        "node" => ("node".to_string(), vec!["-e".to_string(), substituted]),
        "ruby" => ("ruby".to_string(), vec!["-e".to_string(), substituted]),
        "perl" => ("perl".to_string(), vec!["-e".to_string(), substituted]),
        "powershell" => ("pwsh".to_string(), vec!["-Command".to_string(), substituted]),
        _ => (engine.to_string(), vec!["-c".to_string(), substituted]),
    };

    #[cfg(target_os = "linux")]
    {
        let original_interpreter = interpreter.clone();
        let original_args = run_args.clone();
        interpreter = "bwrap".to_string();
        run_args = vec![
            "--unshare-all".to_string(),
            "--share-net".to_string(),
            "--ro-bind".to_string(), "/".to_string(), "/".to_string(),
            "--dev".to_string(), "/dev".to_string(),
            "--proc".to_string(), "/proc".to_string(),
            "--tmpfs".to_string(), "/tmp".to_string(),
            "--unshare-pid".to_string(),
            "--unshare-ipc".to_string(),
            "--unshare-cgroup".to_string(),
            "--die-with-parent".to_string(),
            original_interpreter,
        ];
        run_args.extend(original_args);
    }

    #[cfg(target_os = "macos")]
    {
        let profile = "(version 1) (allow default) (deny file-write*) (allow file-write* (subpath \"/dev/null\")) (allow network*)";
        let original_interpreter = interpreter.clone();
        let original_args = run_args.clone();
        interpreter = "sandbox-exec".to_string();
        run_args = vec![
            "-p".to_string(),
            profile.to_string(),
            original_interpreter,
        ];
        run_args.extend(original_args);
    }

    tracing::info!(
        engine = engine,
        target = target,
        "executing code protocol script in memory"
    );

    let hostname = extract_hostname(target);
    let port = extract_port(target);

    let result = tokio::time::timeout(
        timeout_dur,
        Command::new(interpreter)
            .args(&run_args)
            .env("TARGET", target)
            .env("HOSTNAME", &hostname)
            .env("BASE_URL", target)
            .env("PORT", &port)
            .env("TEMPLATE_ID", template_id)
            .output(),
    )
    .await
    .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "script timed out"))?;

    let output = result?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !stderr.is_empty() {
        tracing::debug!(template_id = %template_id, stderr = %stderr, "script stderr");
    }

    Ok(format!("{stdout}\n{stderr}"))
}

/// Substitutes template variables and target metadata into script source code.
#[must_use]
pub fn substitute_target_vars(
    code: &str,
    target: &str,
    variables: &HashMap<String, String>,
) -> String {
    let hostname = extract_hostname(target);
    let mut result = code
        .replace("{{BaseURL}}", target)
        .replace("{{Hostname}}", &hostname)
        .replace("{{Target}}", target)
        .replace("{{Host}}", &hostname);

    for (key, value) in variables {
        result = result.replace(&format!("{{{{{key}}}}}"), value);
    }

    result
}

/// Robustly extract hostname from a URL or raw network target.
#[must_use]
pub fn extract_hostname(target: &str) -> String {
    if let Ok(url) = url::Url::parse(target) {
        return url.host_str().unwrap_or_default().to_string();
    }
    target
        .split('/')
        .next()
        .unwrap_or(target)
        .split(':')
        .next()
        .unwrap_or(target)
        .to_string()
}

/// Robustly extract port from a URL or network target string.
#[must_use]
pub fn extract_port(target: &str) -> String {
    if let Ok(url) = url::Url::parse(target) {
        return url.port_or_known_default().unwrap_or(0).to_string();
    }
    "0".to_string()
}
