// 引入 clap 库的 Parser 宏，用于自动生成命令行解析逻辑
use clap::Parser;
// 引入标准库的 PathBuf，用于处理文件或目录路径
use std::path::PathBuf;

// --- 常量定义 ---
pub const NAME: &str = "东东播放器";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const URL: &str = "https://github.com/xa1st/mddplayer";

/// 命令行参数结构体
/// 使用 #[derive(Parser)] 自动从结构体定义生成解析器
#[derive(Parser, Debug)]
// 设置程序信息，用于 --help 或 -V
#[clap(author, version = VERSION, about = NAME, long_about = None)]
#[clap(group(
    // 定义一个参数组，要求用户必须提供 'file_or_dir' 或 'playlist_config' 中的一个
    clap::ArgGroup::new("input_source")
        .required(true) 
        .args(&["file_or_dir", "playlist_config"]),
))]
pub struct Args {
    /// 要播放的单个音乐文件或包含音乐文件的目录路径
    #[clap(short = 'f', long, group = "input_source")] 
    pub file_or_dir: Option<PathBuf>, 
    
    /// 播放列表配置文件 (.txt, 一行一个路径) 路径
    #[clap(long = "list", group = "input_source")] 
    pub playlist_config: Option<PathBuf>, 
    
    /// 启用纯净模式，不显示程序说明模式（如操作指南）
    #[clap(short = 'c', long)]
    pub clean: bool,
    
    /// 播放模式: 1 (顺序), 2 (倒序), 3 (随机)
    #[clap(short = 'm', long, default_value = "1")] 
    pub mode: u8, 
    
    /// 播放列表播放完毕后是否循环播放 (Loop Play)
    #[clap(long = "loop")]
    pub loop_play: bool,
}