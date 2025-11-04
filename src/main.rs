use clap::{Parser, ValueEnum};
// 核心音频库：用于输出流、音频解码器和播放控制 (Sink)
use rodio::{Decoder, OutputStream, Sink};
// 标准库：时间处理
use std::time::{Instant, Duration};
// 标准库：文件系统操作、I/O 缓冲和写入
use std::{fs::{self, File}, io::{self, BufReader, Write}};
// 标准库：路径处理
use std::path::{Path, PathBuf};
// ID3 标签库：用于读取音频文件的元数据（歌名、作者）
use id3::TagLike; 
// 终端交互库：用于控制终端（raw mode, 键入事件, 光标/清屏）
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, ClearType},
    cursor,
};
// symphonia 核心组件：用于更精确地获取音频文件的总时长
use symphonia::core::{
    formats::FormatOptions, meta::MetadataOptions, probe::Hint,
    io::{MediaSource, MediaSourceStream},
};
// 随机数库：用于随机播放模式下的列表洗牌
use rand::seq::SliceRandom; 


// --- 常量定义 ---
const NAME: &str = "猫东东的音乐播放器";
const VERSION: &str = "1.2.0";
const URL: &str = "https://github.com/xa1st/music-player-cli";

// --- 播放模式枚举 ---
#[derive(Debug, Clone, ValueEnum)]
enum PlayMode {
    Sequential, // 顺序播放 (默认)
    Reversed,   // 倒序播放
    Random,     // 随机播放
}

// ===============================================
// 辅助函数 1: 使用 Symphonia 获取总时长 (Duration)
// 作用：比 rodio 更可靠地获取音频文件的总播放时长。
// ===============================================
fn get_total_duration(path: &Path) -> Duration {
    // 尝试打开文件并创建媒体源
    let source = match std::fs::File::open(path) {
        // 使用 as Box<dyn Trait> 修复编译错误
        Ok(file) => Box::new(file) as Box<dyn MediaSource>,
        Err(_) => return Duration::from_secs(0), // 无法打开则返回 0
    };
    let media_source_stream = MediaSourceStream::new(source, Default::default());
    
    // 准备文件格式提示 (Hint)，加速探测
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        hint.with_extension(ext);
    }
    
    // 使用 symphonia 探测格式
    let probe_result = match symphonia::default::get_probe().format(&hint, media_source_stream, &FormatOptions::default(), &MetadataOptions::default())
    {
        Ok(result) => result,
        Err(_) => return Duration::from_secs(0),
    };
    
    // 从默认音轨参数中计算总秒数
    if let Some(track) = probe_result.format.default_track() {
        if let (Some(n_frames), Some(sample_rate)) = (track.codec_params.n_frames, track.codec_params.sample_rate) {
            let seconds = (n_frames as f64) / (sample_rate as f64);
            return Duration::from_secs_f64(seconds);
        }
    }
    Duration::from_secs(0)
}

// ===============================================
// 辅助函数 2: 扫描音频文件（单个文件或目录）
// ===============================================
fn scan_audio_files(input_path: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    
    // 检查是否是单个文件
    if input_path.is_file() {
        files.push(input_path.to_path_buf());
        return Ok(files);
    }

    // 如果是目录，则遍历
    if input_path.is_dir() {
        for entry in fs::read_dir(input_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    let ext = ext.to_lowercase();
                    // 仅添加支持的音频格式（可根据需要添加更多）
                    if ext == "mp3" || ext == "flac" || ext == "wav" { 
                        files.push(path);
                    }
                }
            }
        }
    }

    Ok(files)
}

// ===============================================
// 辅助函数 3: 读取播放列表文件（.txt）
// 作用：从配置文件中按行读取文件路径
// ===============================================
fn read_playlist_file(path: &Path) -> io::Result<Vec<PathBuf>> {
    let content = fs::read_to_string(path)?;
    let files: Vec<PathBuf> = content
        .lines()
        .map(|line| line.trim()) // 移除每行路径周围的空白
        .filter(|line| !line.is_empty()) // 忽略空行
        .map(|line| PathBuf::from(line))
        .collect();
    
    if files.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "播放列表文件为空或不包含有效路径。"));
    }
    
    Ok(files)
}

// ===============================================
// 命令行参数结构体
// ===============================================

#[derive(Parser, Debug)]
#[clap(author, version = VERSION, about = NAME, long_about = None)]
// 关键：定义参数组，要求用户必须提供其中一个输入源（文件/目录 或 播放列表文件）
#[clap(group(
    clap::ArgGroup::new("input_source")
        .required(true) 
        .args(&["file_or_dir", "playlist_config"]),
))]
struct Args {
    // 【选项一：文件或目录路径】
    /// 要播放的单个音乐文件或包含音乐文件的目录路径
    #[clap(short = 'f', long, group = "input_source")] 
    file_or_dir: Option<PathBuf>, 
    
    // 【选项二：播放列表配置文件 (.txt)】
    /// 播放列表配置文件 (.txt, 一行一个路径) 路径
    #[clap(long, group = "input_source")] 
    playlist_config: Option<PathBuf>, 
    
    /// 启用纯净模式，不显示程序说明模式
    #[clap(long)]
    clean: bool,
    
    /// 播放模式: sequential (顺序), reversed (倒序), random (随机)
    #[clap(short, long, default_value_t = PlayMode::Sequential, value_enum)] 
    mode: PlayMode, 
}

// ===============================================
// MAIN 函数
// ===============================================
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let play_mode = &args.mode;

    // 1. 根据命令行参数获取文件列表
    let mut playlist = if let Some(path) = args.file_or_dir {
        // 模式一：文件或目录
        match scan_audio_files(path.as_path()) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("错误：无法读取路径或文件：{}", e);
                return Err(e.into());
            }
        }
    } else if let Some(config_path) = args.playlist_config {
        // 模式二：播放列表文件
        match read_playlist_file(config_path.as_path()) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("错误：无法读取播放列表配置文件 {:?}：{}", config_path, e);
                return Err(e.into());
            }
        }
    } else {
        // 理论上不可能到达这里，因为 clap 要求必须提供输入源
        unreachable!(); 
    };

    if playlist.is_empty() {
        eprintln!("错误：在指定的路径中未找到支持的音频文件 (.mp3, .flac, .wav)。");
        return Ok(());
    }

    // 2. 应用播放模式：排序或洗牌
    match play_mode {
        PlayMode::Reversed => playlist.reverse(), // 倒序
        PlayMode::Random => {
            let mut rng = rand::thread_rng();
            playlist.shuffle(&mut rng); // 随机洗牌
        },
        PlayMode::Sequential => { /* 默认顺序，无需操作 */ }
    }

    // ----------------------------------------------------
    // --- 核心播放逻辑：初始化和播放循环 ---
    // ----------------------------------------------------

    let mut stdout = std::io::stdout();
    
    // 终端初始化：清屏、进入 Raw Mode（实现实时按键监听）、隐藏光标
    execute!(stdout, crossterm::terminal::Clear(ClearType::All), crossterm::cursor::MoveTo(0, 0))?;
    enable_raw_mode()?; 
    execute!(stdout, cursor::Hide)?;
    
    // 初始化音频输出和 Sink（Rodio 核心组件）
    let (_stream, stream_handle) = OutputStream::try_default()?;
    let sink = Sink::try_new(&stream_handle)?;

    // 显示界面信息（非纯净模式下）
    if !args.clean {
        // 播放时显示的界面
        println!("\n=======================================================");
        // 使用格式化宏 {NAME:<40} 来确保 NAME 后面有足够的空格，保持右侧对齐
        println!("  {} (v.{})", NAME, VERSION);
        println!("  主页: {}", URL);
        println!("=======================================================");
        println!("==================【🕹️ 控 制 说 明】===================");
        println!("  [P] 键: ...... 暂停播放  [空格] 键: ...... 恢复播放");
        println!("  [←] 键: ...... 上一首    [→] 键: ...... 下一首");
        println!("  [Q] 键: ...... 退出播放");
        println!("=======================================================");
        // 留白一行给进度条
        // println!("\n");
    }

    // --- 主循环：迭代播放列表 ---
    let total_tracks = playlist.len();
    let mut current_track_index: usize = 0;
    // 【关键修复变量】用于记录用户切歌的偏移量 (例如 +1 或 -1)，防止跳两首歌
    let mut index_offset: i32 = 0; 
    
    // 【新增防抖机制】
    const MIN_SKIP_INTERVAL: Duration = Duration::from_millis(250); // 最小切歌间隔 (250ms)
    // 初始化为“允许立即跳过”，确保第一次按键有效
    let mut last_skip_time = Instant::now() - MIN_SKIP_INTERVAL; 
    
    while current_track_index < total_tracks {
        // 获取当前要播放的歌曲路径
        let track_path = &playlist[current_track_index];
        let track_path_str = track_path.to_string_lossy();
        
        // 1. 文件加载、解码、添加到 Sink
        let file = match File::open(&track_path) {
            Ok(f) => BufReader::new(f),
            Err(e) => {
                eprintln!("\n⚠️ 跳过文件 {}: 无法打开或读取。错误: {}", track_path_str, e);
                current_track_index += 1; // 切换到下一首
                continue; // 跳过后续逻辑，进入下一轮 while 循环
            }
        };
        
        // 清空 Sink 中的所有内容，并追加新歌
        sink.clear();
        sink.append(Decoder::new(file)?);
        
        // 【自动播放修复】：确保新歌加载后处于播放状态
        if sink.is_paused() {
            sink.play();
        }

        // 2. 获取元数据和总时长
        let (title, artist) = match id3::Tag::read_from_path(&track_path) {
            Ok(tag) => (
                tag.title().unwrap_or("未知音乐名").to_string(),
                tag.artist().unwrap_or("未知作者").to_string(),
            ),
            Err(_) => ("未知音乐名".to_string(), "未知作者".to_string()),
        };
        
        let total_duration = get_total_duration(track_path.as_path());
        let total_duration_str = if total_duration.as_secs() > 0 {
            format!("{:02}:{:02}", total_duration.as_secs() / 60, total_duration.as_secs() % 60)
        } else {
            "??:??".to_string()
        };
        
        // 3. 计时器重置
        let start_time = Instant::now();
        let mut paused_duration = Duration::from_secs(0); // 累计暂停时间
        let mut last_pause_time: Option<Instant> = None; // 上次暂停的时间点
        let mut last_progress_update = Instant::now();
        let update_interval = Duration::from_millis(1000); // 进度条刷新间隔
        
        // 用于判断是否是用户手动切歌导致的退出
        let mut forced_stop = false; 

        // 4. 内部播放循环 (当前歌曲播放循环)
        while !sink.empty() {
            // --- 时间计算 ---
            let mut current_time = Duration::from_secs(0);
            if sink.is_paused() {
                // 如果是暂停状态，记录暂停开始时间
                if last_pause_time.is_none() { last_pause_time = Some(Instant::now()); }
            } else {
                // 如果是播放状态，计算当前播放时间 (总流逝时间 - 累计暂停时间)
                current_time = start_time.elapsed() - paused_duration;
            }
            
            // --- 刷新显示 ---
            if last_progress_update.elapsed() >= update_interval {
                let current_time_str = format!("{:02}:{:02}", current_time.as_secs() / 60, current_time.as_secs() % 60);
                
                // 歌曲计数显示
                let track_count_str = format!("[{}/{}]", current_track_index + 1, total_tracks); 
                
                let display_text = format!("🎝 正在播放: {} [{} - {}] - [{}-{}]", track_count_str, title, artist, current_time_str, total_duration_str);

                // 移动光标到行首，清空当前行，并打印进度信息
                execute!(stdout, crossterm::cursor::MoveToColumn(0), crossterm::terminal::Clear(ClearType::CurrentLine))?;
                print!("{}", display_text);
                stdout.flush()?; 
                last_progress_update = Instant::now();
            }

            // --- 用户输入处理 ---
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key_event) = event::read()? {
                    match key_event.code {
                        // 暂停
                        KeyCode::Char('p') | KeyCode::Char('P') => {
                            if !sink.is_paused() { sink.pause(); last_pause_time = Some(Instant::now()); }
                        }
                        // 恢复 (空格)
                        KeyCode::Char(' ') => {
                            if sink.is_paused() { 
                                sink.play(); 
                                // 从暂停状态恢复，将暂停时间累加到 paused_duration
                                if let Some(pause_start) = last_pause_time.take() {
                                    paused_duration += pause_start.elapsed();
                                }
                            }
                        }
                        
                        // 下一首 (Right Arrow)
                        KeyCode::Right => {
                            // ✅ 防抖检查：如果距离上次跳过时间太短，则忽略
                            if last_skip_time.elapsed() < MIN_SKIP_INTERVAL {
                                continue;
                            }
                            
                            if current_track_index < total_tracks - 1 {
                                sink.stop(); 
                                index_offset = 1; // 记录下一首 (+1) 偏移量
                                forced_stop = true;
                                last_skip_time = Instant::now(); // 更新切歌时间戳
                                break; // 退出内部循环，进入下一首
                            }
                        }
                        
                        // 上一首 (Left Arrow)
                        KeyCode::Left => {
                            // ✅ 防抖检查：如果距离上次跳过时间太短，则忽略
                            if last_skip_time.elapsed() < MIN_SKIP_INTERVAL {
                                continue;
                            }
                            
                            if current_track_index > 0 {
                                sink.stop(); 
                                index_offset = -1; // 记录上一首 (-1) 偏移量
                                forced_stop = true;
                                last_skip_time = Instant::now(); // 更新切歌时间戳
                                break; // 退出内部循环，进入上一首
                            }
                        }

                        // 退出 (Q)
                        KeyCode::Char('q') | KeyCode::Char('Q') => {
                            // 清理终端，恢复模式，并退出程序
                            execute!(stdout, crossterm::cursor::MoveToColumn(0), crossterm::terminal::Clear(ClearType::CurrentLine))?;
                            println!("👋 播放器退出。");
                            disable_raw_mode()?;
                            execute!(stdout, cursor::Show)?;
                            return Ok(());
                        }
                        _ => {}
                    }
                }
            }
        } // 内部 while 循环结束 (当前歌曲播放完毕或被中断)
        
        // 【索引统一更新逻辑】
        if forced_stop {
            // 情况一：用户切歌导致的退出
            if index_offset > 0 {
                current_track_index += 1;
            } else if index_offset < 0 {
                // 使用 Safe Subtraction，因为我们在 KeyCode::Left 中已经检查了 current_track_index > 0
                current_track_index -= 1;
            }
            // 重置偏移量，等待下次用户输入
            index_offset = 0; 
        } else {
            // 情况二：歌曲正常播放完毕
            execute!(stdout, crossterm::cursor::MoveToColumn(0), crossterm::terminal::Clear(ClearType::CurrentLine))?;
            println!("🎶 歌曲 '{}' 播放完毕。", title);
            current_track_index += 1; 
        }
    } // 主 while 循环结束 (播放列表全部播放完毕)


    // 清理和退出 (循环正常结束)
    execute!(stdout, crossterm::cursor::MoveToColumn(0), crossterm::terminal::Clear(ClearType::CurrentLine))?;
    println!("播放列表已全部播放完毕。");

    // 关键：恢复终端状态（退出 Raw Mode 并显示光标）
    disable_raw_mode()?;
    execute!(stdout, cursor::Show)?;

    Ok(())
}