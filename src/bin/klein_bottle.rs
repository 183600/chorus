use anyhow::Result;
use clap::{Arg, Command};
use serde_json;
use std::fs;
use std::path::Path;

// 导入本地模块 - 在二进制目标中需要这样导入
use chorus::config::Config;
use chorus::klein_bottle::{create_demo_config, get_demo_questions, KleinBottleWorkflow};

#[tokio::main]
async fn main() -> Result<()> {
    let matches = Command::new("klein_bottle")
        .version("1.0.0")
        .about("克莱因瓶反思循环工作流 - 实现LLM的深度递归思考")
        .arg(
            Arg::new("question")
                .short('q')
                .long("question")
                .value_name("QUESTION")
                .help("要反思的问题")
                .action(clap::ArgAction::Set),
        )
        .arg(
            Arg::new("iterations")
                .short('i')
                .long("iterations")
                .value_name("COUNT")
                .help("最大迭代次数 (默认: 3)")
                .default_value("3")
                .action(clap::ArgAction::Set),
        )
        .arg(
            Arg::new("threshold")
                .short('t')
                .long("threshold")
                .value_name("THRESHOLD")
                .help("收敛阈值 0-1 (默认: 0.8)")
                .default_value("0.8")
                .action(clap::ArgAction::Set),
        )
        .arg(
            Arg::new("model")
                .short('m')
                .long("model")
                .value_name("MODEL")
                .help("使用的模型名称 (默认: glm-4.6)")
                .default_value("glm-4.6")
                .action(clap::ArgAction::Set),
        )
        .arg(
            Arg::new("demo")
                .short('d')
                .long("demo")
                .help("使用演示问题")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .value_name("FILE")
                .help("输出结果到JSON文件")
                .action(clap::ArgAction::Set),
        )
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("配置文件路径 (默认: config.toml)")
                .default_value("config.toml")
                .action(clap::ArgAction::Set),
        )
        .get_matches();

    // 加载配置
    let config_path = matches.get_one::<String>("config").unwrap();
    let global_config = if Path::new(config_path).exists() {
        match Config::load(config_path) {
            Ok(config) => config,
            Err(e) => {
                println!("警告: 无法加载配置文件 {}: {}", config_path, e);
                return Err(e);
            }
        }
    } else {
        match Config::load_auto() {
            Ok(config) => config,
            Err(e) => {
                println!("错误: 无法加载任何配置文件。请确保 {} 存在或设置正确的配置路径。", config_path);
                println!("详细错误: {}", e);
                return Err(e);
            }
        }
    };

    // 获取问题
    let question = if matches.get_flag("demo") {
        let demo_questions = get_demo_questions();
        println!("=== 可用演示问题 ===");
        for (i, q) in demo_questions.iter().enumerate() {
            println!("{}: {}", i + 1, q);
        }
        print!("请选择问题编号 (1-{}): ", demo_questions.len());
        use std::io;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let choice: usize = input.trim().parse()
            .map_err(|_| anyhow::anyhow!("无效的选择"))?;
        
        if choice == 0 || choice > demo_questions.len() {
            return Err(anyhow::anyhow!("无效的选择"));
        }
        
        demo_questions[choice - 1].to_string()
    } else if let Some(q) = matches.get_one::<String>("question") {
        q.clone()
    } else {
        return Err(anyhow::anyhow!("请提供问题或使用 --demo 选项"));
    };

    // 配置克莱因瓶工作流
    let mut kb_config = create_demo_config();
    kb_config.max_iterations = matches.get_one::<String>("iterations")
        .unwrap()
        .parse()
        .map_err(|_| anyhow::anyhow!("无效的迭代次数"))?;
    kb_config.convergence_threshold = matches.get_one::<String>("threshold")
        .unwrap()
        .parse()
        .map_err(|_| anyhow::anyhow!("无效的收敛阈值"))?;
    kb_config.model_name = matches.get_one::<String>("model").unwrap().clone();

    println!("=== 克莱因瓶反思循环启动 ===");
    println!("问题: {}", question);
    println!("最大迭代次数: {}", kb_config.max_iterations);
    println!("收敛阈值: {}", kb_config.convergence_threshold);
    println!("使用模型: {}", kb_config.model_name);
    println!();

    // 创建工作流并执行
    let workflow = KleinBottleWorkflow::new(kb_config, &global_config)?;
    let result = workflow.execute_reflection_cycle(&question).await?;

    // 打印详细报告
    workflow.print_detailed_report(&result);

    // 保存结果到文件
    if let Some(output_file) = matches.get_one::<String>("output") {
        let json_result = serde_json::to_string_pretty(&result)?;
        fs::write(output_file, json_result)?;
        println!("\n结果已保存到: {}", output_file);
    }

    // 简单的自检
    println!("\n=== 自检结果 ===");
    if result.converged {
        println!("✓ 工作流成功收敛");
    } else {
        println!("⚠ 工作流未达到收敛阈值，但完成了所有迭代");
    }
    
    if result.total_iterations > 1 {
        println!("✓ 成功执行了 {} 次迭代", result.total_iterations);
    } else {
        println!("⚠ 仅执行了初始生成，没有反思迭代");
    }
    
    if let Some(score) = result.final_score {
        if score >= 0.7 {
            println!("✓ 最终质量评分良好: {:.2}/1.00", score);
        } else {
            println!("⚠ 最终质量评分较低: {:.2}/1.00", score);
        }
    } else {
        println!("⚠ 未能获取最终质量评分");
    }

    println!("✓ 克莱因瓶反思循环执行完成");

    Ok(())
}