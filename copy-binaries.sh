#!/bin/bash
# 将编译好的二进制文件从远程服务器复制到本地

REMOTE="root@192.168.10.229"
REMOTE_DIR="/tmp/n2n-build"
LOCAL_DIR="D:/dev/code/anyn2n/src-tauri/binaries"

echo "准备从远程服务器复制 edge 二进制文件..."

# 创建本地目录
mkdir -p "$LOCAL_DIR"

# 复制 Linux 版本
echo "复制 Linux 版本..."
scp "${REMOTE}:${REMOTE_DIR}/n2n/edge" "${LOCAL_DIR}/edge-x86_64-unknown-linux-gnu"
chmod +x "${LOCAL_DIR}/edge-x86_64-unknown-linux-gnu"

# 复制 Windows 版本
echo "复制 Windows 版本..."
scp "${REMOTE}:${REMOTE_DIR}/n2n-windows/edge.exe" "${LOCAL_DIR}/edge-x86_64-pc-windows-msvc.exe"

echo "完成！"
ls -lh "${LOCAL_DIR}"/edge-*
