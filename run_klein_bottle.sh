#!/bin/bash

# 克莱因瓶反思循环运行脚本
# Klein Bottle Reflection Cycle Runner

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# 打印带颜色的消息
print_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# 显示帮助信息
show_help() {
    echo "克莱因瓶反思循环运行脚本"
    echo ""
    echo "用法: $0 [选项]"
    echo ""
    echo "选项:"
    echo "  -h, --help              显示此帮助信息"
    echo "  -q, --question QUESTION 指定问题"
    echo "  -d, --demo              使用演示问题"
    echo "  -i, --iterations NUM    迭代次数 (默认: 3)"
    echo "  -t, --threshold NUM     收敛阈值 (默认: 0.8)"
    echo "  -m, --model MODEL       模型名称 (默认: glm-4.6)"
    echo "  -o, --output FILE       输出文件"
    echo "  -c, --config FILE       配置文件"
    echo "  --check                 运行自检"
    echo "  --build                 构建项目"
    echo "  --clean                 清理构建文件"
    echo ""
    echo "示例:"
    echo "  $0 --demo                           # 交互式演示"
    echo "  $0 -q \"意识的本质是什么？\"          # 指定问题"
    echo "  $0 -q \"AI情感\" -i 5 -t 0.85 -o res.json  # 高级参数"
    echo "  $0 --check                          # 运行自检"
}

# 检查依赖
check_dependencies() {
    print_info "检查依赖..."
    
    if ! command -v cargo &> /dev/null; then
        print_error "Rust/Cargo 未安装"
        exit 1
    fi
    
    if ! cargo check --quiet 2>/dev/null; then
        print_error "项目依赖检查失败，请运行 'cargo build'"
        exit 1
    fi
    
    print_success "依赖检查通过"
}

# 构建项目
build_project() {
    print_info "构建项目..."
    cargo build --release
    print_success "项目构建完成"
}

# 清理构建文件
clean_project() {
    print_info "清理构建文件..."
    cargo clean
    print_success "清理完成"
}

# 运行自检
run_self_check() {
    print_info "运行克莱因瓶反思循环自检..."
    
    # 检查配置文件
    if [[ ! -f "config.toml" ]] && [[ ! -f "klein-bottle-config.toml" ]]; then
        print_warning "未找到配置文件，使用演示配置"
        CONFIG_FILE="klein-bottle-demo.toml"
    else
        CONFIG_FILE="config.toml"
    fi
    
    # 检查二进制文件
    if [[ ! -f "target/release/klein_bottle" ]]; then
        print_info "二进制文件不存在，开始构建..."
        build_project
    fi
    
    # 运行基本检查
    print_info "检查基本功能..."
    
    # 检查帮助信息
    if ./target/release/klein_bottle --help > /dev/null 2>&1; then
        print_success "帮助信息正常"
    else
        print_error "帮助信息异常"
        return 1
    fi
    
    # 检查配置文件解析
    if ./target/release/klein_bottle --config "$CONFIG_FILE" --help > /dev/null 2>&1; then
        print_success "配置文件解析正常"
    else
        print_error "配置文件解析异常"
        return 1
    fi
    
    print_success "自检完成"
}

# 运行克莱因瓶工作流
run_klein_bottle() {
    local question=""
    local demo=false
    local iterations=3
    local threshold=0.8
    local model="glm-4.6"
    local output=""
    local config="config.toml"
    
    # 解析参数
    while [[ $# -gt 0 ]]; do
        case $1 in
            -q|--question)
                question="$2"
                shift 2
                ;;
            -d|--demo)
                demo=true
                shift
                ;;
            -i|--iterations)
                iterations="$2"
                shift 2
                ;;
            -t|--threshold)
                threshold="$2"
                shift 2
                ;;
            -m|--model)
                model="$2"
                shift 2
                ;;
            -o|--output)
                output="$2"
                shift 2
                ;;
            -c|--config)
                config="$2"
                shift 2
                ;;
            *)
                print_error "未知参数: $1"
                show_help
                exit 1
                ;;
        esac
    done
    
    # 检查配置文件
    if [[ ! -f "$config" ]]; then
        print_warning "配置文件 '$config' 不存在，尝试使用演示配置"
        if [[ -f "klein-bottle-demo.toml" ]]; then
            config="klein-bottle-demo.toml"
        else
            print_error "未找到任何配置文件"
            exit 1
        fi
    fi
    
    # 构建命令
    local cmd="./target/release/klein_bottle"
    local args=()
    
    if [[ -n "$question" ]]; then
        args+=(--question "$question")
    fi
    
    if [[ "$demo" == true ]]; then
        args+=(--demo)
    fi
    
    args+=(--iterations "$iterations")
    args+=(--threshold "$threshold")
    args+=(--model "$model")
    args+=(--config "$config")
    
    if [[ -n "$output" ]]; then
        args+=(--output "$output")
    fi
    
    # 检查二进制文件
    if [[ ! -f "target/release/klein_bottle" ]]; then
        print_info "二进制文件不存在，开始构建..."
        build_project
    fi
    
    # 运行命令
    print_info "启动克莱因瓶反思循环..."
    print_info "配置文件: $config"
    if [[ -n "$question" ]]; then
        print_info "问题: $question"
    fi
    
    echo ""
    "$cmd" "${args[@]}"
    echo ""
    
    if [[ $? -eq 0 ]]; then
        print_success "克莱因瓶反思循环执行完成"
    else
        print_error "执行失败"
        exit 1
    fi
}

# 主函数
main() {
    case "${1:-}" in
        -h|--help)
            show_help
            ;;
        --check)
            check_dependencies
            run_self_check
            ;;
        --build)
            check_dependencies
            build_project
            ;;
        --clean)
            clean_project
            ;;
        "")
            # 默认运行演示
            run_klein_bottle --demo
            ;;
        *)
            # 传递所有参数给运行函数
            run_klein_bottle "$@"
            ;;
    esac
}

# 运行主函数
main "$@"