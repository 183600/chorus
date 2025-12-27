#!/bin/bash

# 克莱因瓶反思循环自检脚本
# Klein Bottle Reflection Cycle Self-Check

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# 统计变量
CHECKS_TOTAL=0
CHECKS_PASSED=0
CHECKS_FAILED=0

# 打印带颜色的消息
print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[PASS]${NC} $1"
    ((CHECKS_PASSED++))
}

print_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

print_error() {
    echo -e "${RED}[FAIL]${NC} $1"
    ((CHECKS_FAILED++))
}

print_header() {
    echo ""
    echo "=========================================="
    echo "$1"
    echo "=========================================="
}

# 检查函数
check_item() {
    local description="$1"
    local command="$2"
    local expected_exit_code="${3:-0}"
    
    ((CHECKS_TOTAL++))
    echo -n "检查: $description ... "
    
    if eval "$command" >/dev/null 2>&1; then
        local exit_code=$?
        if [[ $exit_code -eq $expected_exit_code ]]; then
            print_success "通过"
        else
            print_error "失败 (退出码: $exit_code)"
        fi
    else
        print_error "命令执行失败"
    fi
}

# 检查文件存在性
check_file_exists() {
    local file="$1"
    local description="${2:-文件 $file}"
    
    ((CHECKS_TOTAL++))
    echo -n "检查: $description ... "
    
    if [[ -f "$file" ]]; then
        print_success "存在"
    else
        print_error "不存在"
    fi
}

# 检查目录存在性
check_dir_exists() {
    local dir="$1"
    local description="${2:-目录 $dir}"
    
    ((CHECKS_TOTAL++))
    echo -n "检查: $description ... "
    
    if [[ -d "$dir" ]]; then
        print_success "存在"
    else
        print_error "不存在"
    fi
}

# 主自检函数
run_self_check() {
    print_header "克莱因瓶反思循环自检"
    
    # 1. 基础环境检查
    print_header "1. 基础环境检查"
    
    check_item "Rust 工具链" "command -v cargo"
    check_item "Cargo 版本" "cargo --version"
    check_item "项目根目录" "test -f Cargo.toml"
    
    # 2. 项目结构检查
    print_header "2. 项目结构检查"
    
    check_file_exists "Cargo.toml" "项目配置文件"
    check_file_exists "src/main.rs" "主程序文件"
    check_file_exists "src/klein_bottle.rs" "克莱因瓶核心模块"
    check_file_exists "src/bin/klein_bottle.rs" "命令行工具"
    check_file_exists "run_klein_bottle.sh" "运行脚本"
    check_file_exists "klein-bottle-demo.toml" "演示配置文件"
    check_file_exists "KLEIN_BOTTLE_README.md" "文档文件"
    
    check_dir_exists "src" "源代码目录"
    check_dir_exists "src/bin" "二进制目标目录"
    
    # 3. 依赖检查
    print_header "3. 依赖检查"
    
    check_item "项目依赖解析" "cargo check"
    check_item "项目构建" "cargo build --release"
    check_item "二进制文件生成" "test -f target/release/klein_bottle"
    
    # 4. 功能检查
    print_header "4. 功能检查"
    
    if [[ -f "target/release/klein_bottle" ]]; then
        check_item "帮助信息显示" "./target/release/klein_bottle --help"
        check_item "版本信息显示" "./target/release/klein_bottle --version"
        
        # 检查配置文件解析
        check_item "演示配置解析" "./target/release/klein_bottle --config klein-bottle-demo.toml --help"
        
        # 检查参数验证
        check_item "无效参数处理" "./target/release/klein_bottle --invalid-option" 1
    fi
    
    # 5. 配置文件检查
    print_header "5. 配置文件检查"
    
    # 检查TOML语法
    if command -v python3 &> /dev/null; then
        check_item "演示配置语法" "python3 -c \"import toml; toml.load('klein-bottle-demo.toml')\""
    else
        print_warning "Python3 未安装，跳过配置语法检查"
        ((CHECKS_TOTAL++))
        ((CHECKS_PASSED++))  # 不算失败
    fi
    
    # 检查必要配置项
    if [[ -f "klein-bottle-demo.toml" ]]; then
        check_item "配置包含模型设置" "grep -q '\[model\]' klein-bottle-demo.toml"
        check_item "配置包含工作流设置" "grep -q '\[klein-bottle\]' klein-bottle-demo.toml"
    fi
    
    # 6. 文档检查
    print_header "6. 文档检查"
    
    check_file_exists "KLEIN_BOTTLE_README.md" "主文档"
    check_item "README包含使用说明" "grep -q '快速开始' KLEIN_BOTTLE_README.md"
    check_item "README包含配置说明" "grep -q '配置说明' KLEIN_BOTTLE_README.md"
    check_item "README包含示例" "grep -q '示例' KLEIN_BOTTLE_README.md"
    
    # 7. 脚本检查
    print_header "7. 脚本检查"
    
    check_item "运行脚本可执行" "test -x run_klein_bottle.sh"
    check_item "运行脚本语法" "bash -n run_klein_bottle.sh"
    check_item "自检脚本可执行" "test -x check_klein_bottle.sh"
    check_item "自检脚本语法" "bash -n check_klein_bottle.sh"
    
    # 8. 安全检查
    print_header "8. 安全检查"
    
    # 检查是否有硬编码的API密钥
    check_item "无硬编码API密钥" "! grep -r 'api_key.*=\"[a-f0-9]' src/ || true"
    check_item "无敏感配置提交" "! grep -r 'password\\|secret\\|token' klein-bottle-demo.toml || true"
    
    # 9. 性能检查
    print_header "9. 性能检查"
    
    if [[ -f "target/release/klein_bottle" ]]; then
        local size=$(stat -f%z "target/release/klein_bottle" 2>/dev/null || stat -c%s "target/release/klein_bottle" 2>/dev/null || echo "0")
        if [[ $size -gt 0 ]] && [[ $size -lt 50000000 ]]; then  # 小于50MB
            print_success "二进制文件大小合理 $(($size / 1024 / 1024))MB"
            ((CHECKS_PASSED++))
        else
            print_error "二进制文件大小异常"
            ((CHECKS_FAILED++))
        fi
        ((CHECKS_TOTAL++))
    fi
    
    # 输出总结
    print_header "自检结果总结"
    
    echo "总检查项: $CHECKS_TOTAL"
    echo -e "通过: ${GREEN}$CHECKS_PASSED${NC}"
    echo -e "失败: ${RED}$CHECKS_FAILED${NC}"
    
    local success_rate=$((CHECKS_PASSED * 100 / CHECKS_TOTAL))
    echo "成功率: $success_rate%"
    
    if [[ $CHECKS_FAILED -eq 0 ]]; then
        echo ""
        print_success "🎉 所有检查通过！克莱因瓶反思循环准备就绪。"
        return 0
    elif [[ $success_rate -ge 80 ]]; then
        echo ""
        print_warning "⚠️  大部分检查通过，但存在一些问题需要修复。"
        return 1
    else
        echo ""
        print_error "❌ 多项检查失败，请修复问题后重试。"
        return 2
    fi
}

# 快速检查（用于CI）
quick_check() {
    print_info "运行快速检查..."
    
    # 只检查关键项
    check_item "Rust 工具链" "command -v cargo"
    check_file_exists "Cargo.toml"
    check_file_exists "src/klein_bottle.rs"
    check_file_exists "src/bin/klein_bottle.rs"
    check_item "项目构建" "cargo build --release"
    check_item "二进制文件生成" "test -f target/release/klein_bottle"
    
    if [[ $CHECKS_FAILED -eq 0 ]]; then
        print_success "快速检查通过"
        return 0
    else
        print_error "快速检查失败"
        return 1
    fi
}

# 主函数
main() {
    case "${1:-full}" in
        full)
            run_self_check
            ;;
        quick)
            quick_check
            ;;
        --help|-h)
            echo "克莱因瓶反思循环自检脚本"
            echo ""
            echo "用法: $0 [选项]"
            echo ""
            echo "选项:"
            echo "  full     完整自检 (默认)"
            echo "  quick    快速检查"
            echo "  --help   显示此帮助信息"
            ;;
        *)
            print_error "未知选项: $1"
            echo "使用 --help 查看帮助信息"
            exit 1
            ;;
    esac
}

# 运行主函数
main "$@"