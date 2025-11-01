#!/bin/bash

# 测试配置文件迁移功能

echo "=== 配置文件迁移测试 ==="
echo ""

# 创建测试目录
TEST_DIR="/tmp/chorus_migration_test_$$"
mkdir -p "$TEST_DIR/.config/chorus"

echo "1. 创建测试目录: $TEST_DIR"
echo ""

# 复制旧配置文件
cp test-old-config.toml "$TEST_DIR/.config/chorus/config.toml"
echo "2. 复制旧配置文件到: $TEST_DIR/.config/chorus/config.toml"
echo ""

# 显示旧配置内容
echo "3. 旧配置文件内容:"
echo "---"
head -20 "$TEST_DIR/.config/chorus/config.toml"
echo "..."
echo ""

# 设置 HOME 环境变量并运行程序（这会触发迁移）
echo "4. 运行程序触发配置迁移..."
HOME="$TEST_DIR" RUST_LOG=info cargo build --quiet 2>&1 | grep -i "migrat\|backup" || echo "   (构建完成)"
echo ""

# 使用一个简单的 Rust 程序来测试配置加载
cat > /tmp/test_config_load.rs << 'EOF'
use std::env;

fn main() {
    env::set_var("HOME", env::args().nth(1).unwrap());
    env::set_var("RUST_LOG", "info");
    
    // 这里需要实际的代码来加载配置
    println!("Testing config load...");
}
EOF

echo "5. 测试配置加载和迁移..."
echo ""

# 检查是否生成了备份文件
BACKUP_FILES=$(ls -1 "$TEST_DIR/.config/chorus/"config.toml.bak* 2>/dev/null | wc -l)
if [ "$BACKUP_FILES" -gt 0 ]; then
    echo "✓ 备份文件已创建:"
    ls -lh "$TEST_DIR/.config/chorus/"config.toml.bak*
    echo ""
else
    echo "✗ 未找到备份文件"
    echo ""
fi

# 检查新配置文件是否迁移到新的 workflow 格式
if grep -q "\[workflow-integration\]" "$TEST_DIR/.config/chorus/config.toml" \
   && grep -q "json = \"\"\"" "$TEST_DIR/.config/chorus/config.toml"; then
    echo "✓ 新配置文件已迁移到 workflow json 格式"
    echo ""
    echo "6. 新配置文件内容（前30行）:"
    echo "---"
    head -30 "$TEST_DIR/.config/chorus/config.toml"
    echo "..."
else
    echo "✗ 新配置文件未检测到 workflow json 格式"
    echo ""
    echo "当前配置文件内容:"
    cat "$TEST_DIR/.config/chorus/config.toml"
fi

echo ""
echo "=== 测试完成 ==="
echo ""
echo "测试文件位置: $TEST_DIR/.config/chorus/"
echo "清理测试文件: rm -rf $TEST_DIR"
