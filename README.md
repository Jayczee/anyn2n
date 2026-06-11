# AnyN2N

基于 n2n v3 的上层 Tauri + Rust 客户端，用于快速搭建局域网来达成局域网游戏联机等目的。把 n2n edge 作为后台进程管理，提供多配置保存、实时流量统计、防火墙一键放行、日志监控等便捷功能。

支持 Windows、macOS (仅 Intel)、Linux。

<p align="center"><img src="anyn2n_ui.jpg" width="720" /></p>

## 平台兼容性

| 平台 | 虚拟网卡 | 安装方式 |
|------|----------|----------|
| Windows | TAP-Windows | 首次启动自动安装 |
| Linux | TUN (内核自带) | 无需额外安装，sudo 启动 edge |
| macOS Intel | TUN/TAP | 需手动安装 tuntap 内核扩展 |
| **macOS Apple Silicon** | **不支持** | **Apple 禁止第三方内核扩展，无法使用** |

### macOS Intel 配置

1. 下载安装 [tuntap](http://tuntaposx.sourceforge.net/)
2. 重启 Mac
3. 打开「系统偏好设置 → 安全性与隐私 → 通用」，点击「允许」加载内核扩展
4. 首次连接时输入管理员密码授权 edge 创建虚拟网卡

> macOS Apple Silicon (M 系列芯片) 因没有 tuntap 内核扩展暂不支持，应用启动时会弹窗提示。

## 开发

```bash
bun install
bun run tauri dev
```

Apple Silicon Mac 开发调试可设置环境变量跳过架构检测：
```bash
ANYN2N_SKIP_ARCH_CHECK=1 bun run tauri dev
```

## 构建

```bash
bun run tauri build
```

Windows可以运行build-portable.ps1来构建portable单文件版本。

Edge 二进制和 `wintun.dll` 提前放入 `src-tauri/binaries/`。

## Edge 二进制构建

引用自 [lucktu/n2n](https://github.com/lucktu/n2n)，感谢 lucktu 维护的跨平台 n2n 构建。

## 许可

MIT
