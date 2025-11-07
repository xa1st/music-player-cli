// 声明模块
mod cli;
mod utils;
mod metadata;

// 从各个模块引入所需的项
use clap::Parser;
use rodio::{Decoder, OutputStream, Sink};
use std::time::{Instant, Duration};
use std::{fs::File, io::{self, BufReader, Write}};

use rand::seq::SliceRandom; 
use unicode_width::UnicodeWidthStr;

// 从 cli 模块引入常量和参数结构体
use cli::{Args, NAME, VERSION, URL};
// 从 utils 模块引入所有公共函数，特别是用于智能解析输入的函数
use utils::{get_playlist_from_input, truncate_string, format_duration}; 
// 从 metadata 模块引入元数据获取函数
use metadata::{get_title_artist_info, get_total_duration};

// 终端交互库：用于控制终端（raw mode, 键入事件, 光标/清屏）
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{self, disable_raw_mode, enable_raw_mode, ClearType, SetTitle, SetSize},
    cursor,
};

// --- 常量定义 ---
const MIN_SKIP_INTERVAL: Duration = Duration::from_millis(250); // 最小切歌间隔
const VOLUME_STEP: f32 = 0.01; // 音量调节步长
const UPDATE_INTERVAL: Duration = Duration::from_millis(1000); // 进度更新频率

// ===============================================
// MAIN 函数
// ===============================================

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 解析命令行参数
    let args = Args::parse();
    
    // 从 cli.rs 匹配新的参数名称和类型
    let input_path_str = &args.file;
    let is_simple_mode = args.clean;    // 是否启用纯净模式
    let is_random_enabled = args.random; // 是否启用随机模式
    let is_loop_enabled = args.is_loop;   // 是否启用列表循环 (使用 r#loop 避免关键字冲突)
    let initial_volume = args.volume as f32 / 100.0; // 音量 (转换为 0.0 到 1.0)
    
    // 2. 根据命令行参数获取文件列表，使用 utils::get_playlist_from_input 进行智能判断
    let mut playlist = match get_playlist_from_input(input_path_str) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("错误：处理输入路径 '{}' 时失败：{}", input_path_str, e);
            return Err(e.into());
        }
    };
    
    if playlist.is_empty() {
        eprintln!("错误：在指定的路径中未找到支持的音频文件。");
        return Ok(());
    }

    // 3. 应用播放模式：顺序或随机 (根据 args.random)
    if is_random_enabled {
        if !is_simple_mode {
             println!("启用随机播放模式...");
        }
        let mut rng = rand::thread_rng();
        playlist.shuffle(&mut rng); // 随机洗牌
    } 
    // 否则，保持顺序播放（默认状态）

    // ----------------------------------------------------
    // --- 核心播放逻辑：初始化和播放循环 ---
    // ----------------------------------------------------

    let mut stdout = io::stdout();
    
    // 终端初始化：清屏、进入 Raw Mode、隐藏光标
    execute!(stdout, terminal::Clear(ClearType::All), cursor::MoveTo(0, 0))?;
    // 🌟 新增：使用 crossterm 设置终端窗口和缓冲区大小
    // SetSize(cols: u16, rows: u16)
    if !is_simple_mode {
        execute!(stdout, SetSize(60, 8))?;  
    } else { 
        execute!(stdout, SetSize(60, 1))?;  
    }
    // 终端标题
    let initial_title = format!("{} (v{}) - 启动中...", cli::NAME, cli::VERSION);
    // 设置标题
    execute!(stdout, SetTitle(initial_title))?; 
    enable_raw_mode()?; // 启用原始模式以即时捕获按键
    execute!(stdout, cursor::Hide)?; // 隐藏光标
    
    // 初始化音频输出和 Sink (Sink 是一个播放控制结构体)
    let (_stream, stream_handle) = OutputStream::try_default()?;
    let sink = Sink::try_new(&stream_handle)?;
    // 设置命令行传入的音量，并限制在 [0.0, 1.0] 范围内
    sink.set_volume(initial_volume.min(1.0).max(0.0));
    
    // 显示界面信息（非纯净模式下）
    if !is_simple_mode { 
        // 打印程序信息和操作指南
        println!(" =====================【 {} 】======================", NAME);
        println!("  版本:v{},      主页:{}", VERSION, URL);
        println!(" ===========================================================");
        println!(" ====================【 控 制 说 明 】======================");
        println!("  [P]暂停播放      [空格]恢复播放        [Q]退出播放");
        println!("  [←]上一首    [→]下一首    [↑]音量增    [↓]音量减");
        println!(" ===========================================================");
    }

    // --- 主循环：迭代播放列表 ---
    let total_tracks = playlist.len();
    let mut current_track_index: usize = 0;
    let mut index_offset: i32 = 0; // 用于切歌时的索引调整
    let mut last_skip_time = Instant::now() - MIN_SKIP_INTERVAL; // 避免快速连续切歌
    loop { 
        // 循环播放检查
        if current_track_index >= total_tracks {
            if is_loop_enabled {
                current_track_index = 0; // 重置到第一首
            } else {
                break; // 退出整个播放循环
            }
        }
        
        let track_path = &playlist[current_track_index];
        let track_path_str = track_path.to_string_lossy();
        
        // 5. 文件加载、解码、添加到 Sink
        let file = match File::open(&track_path) {
            Ok(f) => BufReader::new(f),
            Err(e) => {
                eprintln!("\n⚠️ 跳过文件 {}: 无法打开或读取。错误: {}", track_path_str, e);
                current_track_index += 1; 
                continue; 
            }
        };
        sink.clear();
        match Decoder::new(file) {
            Ok(decoder) => sink.append(decoder),
            Err(e) => {
                eprintln!("\n⚠️ 跳过文件 {}: 无法解码。错误: {}", track_path_str, e);
                current_track_index += 1; 
                continue; 
            }
        }
        
        if sink.is_paused() {
            sink.play();
        }

        // 6. 获取元数据和总时长
        let (title, artist) = get_title_artist_info(track_path.as_path());
        // 修改标题
        let current_title = format!("{}-{}-{}v{}", title, artist, NAME, VERSION);
        execute!(stdout, SetTitle(current_title))?;

        // 获取总时长 (使用 metadata 模块的函数)
        let total_duration = get_total_duration(track_path.as_path());
        let total_duration_str = format_duration(total_duration);
        
        // 7. 计时器重置：修复暂停 BUG 的核心变量
        let start_time = Instant::now(); // 歌曲开始时的绝对时间
        let mut paused_duration = Duration::from_secs(0); // 累积的暂停时长
        let mut last_pause_time: Option<Instant> = None; // 上一次暂停的开始时刻
        let mut last_running_time = Duration::from_secs(0); // 暂停前的实际播放时间
        let mut last_progress_update = Instant::now();
        let mut forced_stop = false; // 是否由用户切歌强制停止

        // 8. 内部播放循环 (当前歌曲播放循环)
        while !sink.empty() {
            // --- 时间计算 ---
            // 1. 检查是否处于暂停状态，并记录暂停的起始时刻
            if sink.is_paused() {
                // 如果是刚刚暂停，记录暂停发生的时间点
                if last_pause_time.is_none() { 
                    last_pause_time = Some(Instant::now()); 
                    // 关键修复：在暂停发生时，记录当前的准确播放时间
                    last_running_time = start_time.elapsed().saturating_sub(paused_duration);
                }
            } else {
                // 如果正在播放（或从暂停恢复），计算并累加最近一次的暂停时长
                if let Some(pause_start) = last_pause_time.take() {
                    paused_duration += pause_start.elapsed();
                }
            }
            // 2. 最终计算：如果暂停，显示时间是静止的 last_running_time；否则是实时计算。
            let current_time = if sink.is_paused() {
                last_running_time // 暂停时，时间静止在暂停前的进度
            } else {
                // 播放时，正常计算当前进度
                start_time.elapsed().saturating_sub(paused_duration)
            };
            
            // 刷新显示
            if last_progress_update.elapsed() >= UPDATE_INTERVAL {
                let current_time_str = format_duration(current_time);
                let track_count_str = format!("[{}/{}]", current_track_index + 1, total_tracks); 
                
                // 提取文件扩展名（用于显示文件类型）
                let ext = track_path_str.split('.').last().unwrap_or("未知").to_uppercase();
                
                // 播放模式字符串：根据新的 random 和 loop 标志生成
                let random_str = if is_random_enabled { "随" } else { "顺" };
                let loop_str = if is_loop_enabled { "循" } else { "单" }; 
                let play_mode_str = format!("{}|{}", random_str, loop_str);
                
                // 组装一下当前的字符串，不含歌曲信息占位符
                // {计数}[模式][扩展名][][时间/总时间][音量%]
                let mut display_text_unpadded = format!(" {}[{}][{}][][{}/{}][{:.0}%]", 
                    track_count_str, 
                    play_mode_str, 
                    ext,
                    current_time_str, 
                    total_duration_str,
                    sink.volume() * 100.0
                );
                // 动态获取终端宽度，防止用户调整窗口大小
                let terminal_width = terminal::size().map(|(cols, _)| cols).unwrap_or(80) as usize;
                // 计算剩余宽度：减去已有的显示宽度 (注意这里使用 .width() 计算列宽)
                let current_unpadded_width = display_text_unpadded.as_str().width();
                // let music_info_width = terminal_width.saturating_sub(current_unpadded_width).saturating_sub(2); // 减去 [] 两个字符
                let music_info_width = terminal_width.saturating_sub(current_unpadded_width);
                // 拿到歌曲信息，并获取长度
                let music_info_content = format!("{}-{}", title, artist);
                // 智能截断歌曲信息
                let music_info = if music_info_width < 15 {
                    // 宽度太小，只显示歌曲名
                    truncate_string(&title, music_info_width)
                } else {
                    // 使用 music_info_width 确保不溢出
                    truncate_string(&music_info_content, music_info_width)
                };
                
                // 重新组装最终的文本
                // {计数}[模式][扩展名][歌曲信息][时间/总时间][音量%]
                display_text_unpadded = format!(" {}[{}][{}][{}][{}/{}][{:.0}%]", // 注意 [music_info] 后面的空格
                    track_count_str, 
                    play_mode_str, 
                    ext,
                    music_info,
                    current_time_str, 
                    total_duration_str,
                    sink.volume() * 100.0
                );
                
                // 获取新字符串的字符长度
                let new_len = display_text_unpadded.as_str().width();
                // 计算需要填充的空格数，直到终端总宽度
                let padding_needed = terminal_width.saturating_sub(new_len);
                // 创建填充字符串
                let padding = " ".repeat(padding_needed);
                // 最终要打印的、覆盖整行的字符串
                let display_text = format!("{}{}", display_text_unpadded, padding);
                
                // [修复闪烁]：只移动光标并打印
                execute!(stdout, cursor::MoveToColumn(0))?;
                print!("{}", display_text); // 使用 print! (不换行)
                stdout.flush()?; 
                last_progress_update = Instant::now();
            }
            // ***--- 刷新显示修改结束 ---***
            
            // --- 用户输入处理 (非阻塞) ---
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key_event) = event::read()? {
                    match key_event.code {
                        // 暂停/恢复
                        KeyCode::Char('p') | KeyCode::Char('P') => { if !sink.is_paused() { sink.pause(); } }
                        KeyCode::Char(' ') => { if sink.is_paused() { sink.play(); } }
                        // 音量控制
                        KeyCode::Up => { let current_volume = sink.volume(); let new_volume = (current_volume + VOLUME_STEP).min(1.0); sink.set_volume(new_volume); }
                        KeyCode::Down => { let current_volume = sink.volume(); let new_volume = (current_volume - VOLUME_STEP).max(0.0); sink.set_volume(new_volume); }
                        // 切歌：下一首
                        KeyCode::Right => { 
                            if last_skip_time.elapsed() < MIN_SKIP_INTERVAL { continue; }
                            // 检查是否在列表末尾且循环启用
                            if current_track_index < total_tracks.saturating_sub(1) || is_loop_enabled {
                                sink.stop(); index_offset = 1; forced_stop = true; last_skip_time = Instant::now(); break; } 
                        }
                        // 切歌：上一首
                        KeyCode::Left => { 
                            if last_skip_time.elapsed() < MIN_SKIP_INTERVAL { continue; }
                            // 检查是否在列表开头且循环启用
                            if current_track_index > 0 || is_loop_enabled {
                                sink.stop(); index_offset = -1; forced_stop = true; last_skip_time = Instant::now(); break; } 
                        }
                        // 退出
                        KeyCode::Char('q') | KeyCode::Char('Q') => {
                            // 清理并退出
                            execute!(stdout, cursor::MoveToColumn(0), terminal::Clear(ClearType::CurrentLine))?;
                            println!("👋 播放器退出。");
                            disable_raw_mode()?;
                            execute!(stdout, cursor::Show)?;
                            return Ok(());
                        }
                        _ => {}
                    }
                }
            }
        } // 内部播放循环结束
        
        // 9. 索引更新逻辑 (处理自动播放和强制切歌)
        if forced_stop {
            if index_offset > 0 {
                // 下一首，应用循环逻辑
                current_track_index = (current_track_index + 1) % total_tracks; 
            } else if index_offset < 0 {
                // 上一首，应用循环逻辑 (如果当前为 0，则跳到列表末尾)
                current_track_index = if current_track_index == 0 { total_tracks.saturating_sub(1) } else { current_track_index - 1 };
            }
            index_offset = 0; 
        } else {
            // 歌曲正常播放完毕，准备播放下一首
            execute!(stdout, cursor::MoveToColumn(0), terminal::Clear(ClearType::CurrentLine))?;
            current_track_index += 1; 
        }
    } // 主循环结束
    
    // 10. 播放列表结束后的清理工作
    execute!(stdout, cursor::MoveToColumn(0), terminal::Clear(ClearType::CurrentLine))?;
    println!("播放列表已全部播放完毕。");
    
    // 恢复终端状态
    disable_raw_mode()?;
    execute!(stdout, cursor::Show)?;
    
    Ok(())
}