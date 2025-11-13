// 测试配置迁移功能的独立程序
use std::env;
use std::fs;
use std::path::PathBuf;

// 引入 config 模块
mod config;
use config::Config;

fn main() -> anyhow::Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    // 获取测试目录路径
    let test_dir = env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/chorus_test".to_string());
    
    println!("=== 配置迁移测试 ===");
    println!("测试目录: {}", test_dir);
    println!();

    // 设置 HOME 环境变量
    env::set_var("HOME", &test_dir);

    // 创建配置目录
    let config_dir = PathBuf::from(&test_dir).join(".config/chorus");
    fs::create_dir_all(&config_dir)?;
    println!("✓ 创建配置目录: {}", config_dir.display());

    // 复制旧配置文件
    let old_config_path = config_dir.join("config.toml");
    fs::copy("test-old-config.toml", &old_config_path)?;
    println!("✓ 复制旧配置文件到: {}", old_config_path.display());
    println!();

    // 显示旧配置内容
    println!("旧配置文件内容（前10行）:");
    println!("---");
    let old_content = fs::read_to_string(&old_config_path)?;
    for (i, line) in old_content.lines().enumerate() {
        if i >= 10 {
            println!("...");
            break;
        }
        println!("{}", line);
    }
    println!();

    // 加载配置（这会触发迁移）
    println!("加载配置（触发自动迁移）...");
    let config = Config::load_from_user_config()?;
    println!("✓ 配置加载成功");
    println!();

    // 检查备份文件
    let backup_files: Vec<_> = fs::read_dir(&config_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("config.toml.bak")
        })
        .collect();

    if !backup_files.is_empty() {
        println!("✓ 找到备份文件:");
        for entry in backup_files {
            println!("  - {}", entry.file_name().to_string_lossy());
        }
        println!();
    } else {
        println!("✗ 未找到备份文件");
        println!();
    }

    // 检查新配置内容
    let new_content = fs::read_to_string(&old_config_path)?;
    if new_content.contains("[workflow-integration]") && new_content.contains("json = \"\"\"") {
        println!("✓ 新配置已迁移到 workflow json 格式");
        println!();
        println!("新配置文件内容（前30行）:");
        println!("---");
        for (i, line) in new_content.lines().enumerate() {
            if i >= 30 {
                println!("...");
                break;
            }
            println!("{}", line);
        }
    } else {
        println!("✗ 新配置未检测到 workflow json 格式");
    }

    println!();
    println!("=== 测试完成 ===");
    println!();
    println!("配置信息:");
    println!("  - 模型数量: {}", config.models.len());
    println!(
        "  - 分析器模型: {}",
        config.workflow_integration.analyzer.model
    );
    println!(
        "  - 工作节点数量: {}",
        config.workflow_integration.workers.len()
    );
    if let Some(synth) = &config.workflow_integration.synthesizer {
        println!("  - 综合器模型: {}", synth.model);
    } else if let Some(selector) = &config.workflow_integration.selector {
        println!("  - 综合器模型: 使用选择器 {}", selector.model);
    } else {
        println!("  - 综合器模型: 未配置");
    }

    Ok(())
}
