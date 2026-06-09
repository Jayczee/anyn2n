#!/bin/bash
# AnyN2N - 快速构建 n2n edge 二进制文件

set -e

PROJECT_DIR=$(pwd)
BINARIES_DIR="$PROJECT_DIR/src-tauri/binaries"
TEMP_DIR="/tmp/n2n-build-$$"

echo "==================================="
echo "AnyN2N - n2n Edge 构建脚本"
echo "==================================="

# 检查依赖
check_dependencies() {
    echo "检查依赖..."

    if ! command -v git &> /dev/null; then
        echo "错误: 需要安装 git"
        exit 1
    fi

    if ! command -v make &> /dev/null; then
        echo "错误: 需要安装 make"
        exit 1
    fi

    echo "✓ 依赖检查通过"
}

# 克隆 n2n
clone_n2n() {
    echo "克隆 n2n 源码..."
    rm -rf "$TEMP_DIR"
    git clone --depth 1 https://github.com/ntop/n2n.git "$TEMP_DIR"
    cd "$TEMP_DIR"
    echo "✓ 源码已下载"
}

# 编译 Linux 版本
build_linux() {
    echo ""
    echo ">>> 编译 Linux 版本..."

    make clean 2>/dev/null || true
    rm -rf config.* Makefile 2>/dev/null || true

    ./autogen.sh
    ./configure
    make

    cp edge "$BINARIES_DIR/edge-x86_64-unknown-linux-gnu"
    chmod +x "$BINARIES_DIR/edge-x86_64-unknown-linux-gnu"

    echo "✓ Linux 版本编译完成"
}

# 编译 Windows 版本（交叉编译）
build_windows() {
    echo ""
    echo ">>> 编译 Windows 版本（交叉编译）..."

    if ! command -v x86_64-w64-mingw32-gcc &> /dev/null; then
        echo "警告: 未安装 mingw-w64，跳过 Windows 编译"
        echo "安装命令: sudo apt-get install mingw-w64"
        return
    fi

    make clean 2>/dev/null || true
    rm -rf config.* Makefile 2>/dev/null || true

    ./autogen.sh
    ./configure --host=x86_64-w64-mingw32 CC=x86_64-w64-mingw32-gcc
    make

    cp edge.exe "$BINARIES_DIR/edge-x86_64-pc-windows-msvc.exe"

    echo "✓ Windows 版本编译完成"
}

# 主流程
main() {
    check_dependencies
    clone_n2n

    # 检测当前平台
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        echo "检测到 Linux 平台"
        build_linux
        build_windows
    elif [[ "$OSTYPE" == "darwin"* ]]; then
        echo "检测到 macOS 平台"
        echo ""
        echo ">>> 编译 macOS 版本..."

        # 检测架构
        ARCH=$(uname -m)
        if [[ "$ARCH" == "arm64"* ]]; then
            echo "检测到 Apple Silicon"
            ./autogen.sh
            ./configure CFLAGS="-arch arm64" LDFLAGS="-arch arm64"
            make
            cp edge "$BINARIES_DIR/edge-aarch64-apple-darwin"
            chmod +x "$BINARIES_DIR/edge-aarch64-apple-darwin"
            echo "✓ Apple Silicon 版本编译完成"
        else
            echo "检测到 Intel Mac"
            ./autogen.sh
            ./configure CFLAGS="-arch x86_64" LDFLAGS="-arch x86_64"
            make
            cp edge "$BINARIES_DIR/edge-x86_64-apple-darwin"
            chmod +x "$BINARIES_DIR/edge-x86_64-apple-darwin"
            echo "✓ Intel Mac 版本编译完成"
        fi
    else
        echo "不支持的平台: $OSTYPE"
        exit 1
    fi

    # 清理
    cd "$PROJECT_DIR"
    rm -rf "$TEMP_DIR"

    echo ""
    echo "==================================="
    echo "✓ 编译完成！"
    echo "==================================="
    echo "编译结果:"
    ls -lh "$BINARIES_DIR"/edge-* 2>/dev/null || echo "  (无可用文件)"
    echo ""
    echo "注意:"
    echo "- macOS 版本需要在 macOS 上编译"
    echo "- Windows 版本需要 mingw-w64 工具链"
    echo "- Linux 版本可以在任何 Linux 发行版编译"
}

main
