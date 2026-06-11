use anyhow::Result;

pub fn check_tap_installed() -> bool {
    use std::process::Command;
    use std::os::windows::process::CommandExt;

    let output = Command::new("powershell")
        .args([
            "-Command",
            "Get-NetAdapter | Where-Object { $_.InterfaceDescription -like '*TAP-Windows*' }",
        ])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .output();

    match output {
        Ok(out) => !out.stdout.is_empty(),
        Err(_) => false,
    }
}

pub fn install_tap_driver() -> Result<()> {
    use std::process::Command;
    use std::os::windows::process::CommandExt;

    let installer = crate::embedded::tap_installer_path()
        .ok_or_else(|| anyhow::anyhow!("TAP 安装器未嵌入"))?;

    if !installer.exists() {
        return Err(anyhow::anyhow!("TAP 安装器提取失败"));
    }

    log::info!("Installing TAP driver from: {:?}", installer);

    let output = Command::new(&installer)
        .args(["/S"])
        .creation_flags(0x08000000)
        .output()?;

    if output.status.success() {
        log::info!("TAP driver installed successfully");
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow::anyhow!("TAP 驱动安装失败: {}", stderr))
    }
}

pub fn show_install_error_dialog(detail: &str) {
    use std::process::Command;
    use std::os::windows::process::CommandExt;

    let msg = format!(
        "自动安装虚拟网卡失败，请右键 anyn2n.exe 选择\"以管理员身份运行\"。\n\n错误信息: {}",
        detail
    );

    let _ = Command::new("powershell")
        .args([
            "-Command",
            &format!(
                "Add-Type -AssemblyName PresentationFramework; [System.Windows.MessageBox]::Show('{}', 'AnyN2N', 'OK', 'Error')",
                msg.replace("'", "''")
            ),
        ])
        .creation_flags(0x08000000)
        .output();
}
