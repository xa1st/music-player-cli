use std::path::Path;
use std::time::Duration;
// 引入 lofty 库的 Trait 和函数
use lofty::prelude::TaggedFileExt; 
use lofty::read_from_path; 
// 添加 Accessor Trait
use lofty::tag::Accessor;
// 引入 symphonia 库的格式和元数据选项
use symphonia::core::{
    formats::FormatOptions, meta::MetadataOptions, probe::Hint,
    io::{MediaSource, MediaSourceStream},
};

/// 安全地获取标题和艺术家信息，优先使用主标签。
/// 返回 (title, artist)
pub fn get_title_artist_info(path: &Path) -> (String, String) {
    // 尝试从路径读取 tagged file
    match read_from_path(path) { 
        Ok(tagged_file) => {
            // 获取文件的主要标签（如 ID3v2, Vorbis Comment 等）
            if let Some(tag) = tagged_file.primary_tag() {
                
                // 获取标题，使用 and_then 链式调用
                let title = tag.title()
                    // 闭包返回 Some(String)，以满足 and_then 对 Option<U> 的要求
                    .and_then(|t| Some(t.to_string())) 
                    .unwrap_or_else(|| "未知音乐名".to_string());
                
                // 获取艺术家
                let artist = tag.artist()
                    // 闭包返回 Some(String)，以满足 and_then 对 Option<U> 的要求
                    .and_then(|a| Some(a.to_string())) 
                    .unwrap_or_else(|| "未知作者".to_string());

                return (title, artist);
            }
        },
        Err(_) => {
            // 错误处理：文件可能不是支持的格式，或标签已损坏。
        }
    }
    // 所有方法失败，则回退到默认值
    ("未知".to_string(), "未知".to_string())
}

/// 使用 symphonia 库，通过探测媒体流来获取音频文件的总时长。
pub fn get_total_duration(path: &Path) -> Duration {
    // 尝试打开文件并创建 MediaSource
    let source = match std::fs::File::open(path) {
        Ok(file) => Box::new(file) as Box<dyn MediaSource>,
        Err(_) => return Duration::from_secs(0), // 无法打开则返回 0
    };
    
    // 创建媒体源流
    let media_source_stream = MediaSourceStream::new(source, Default::default());
    
    // 准备文件格式提示 (Hint)
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        hint.with_extension(ext);
    }
    
    // 使用 symphonia 探测格式
    let probe_result = match symphonia::default::get_probe().format(
        &hint, 
        media_source_stream, 
        &FormatOptions::default(), 
        &MetadataOptions::default()
    ) {
        Ok(result) => result,
        Err(_) => return Duration::from_secs(0),
    };
    
    // 从默认音轨参数中计算总秒数
    if let Some(track) = probe_result.format.default_track() {
        if let (Some(n_frames), Some(sample_rate)) = (track.codec_params.n_frames, track.codec_params.sample_rate) {
            // 计算总秒数: (总帧数 / 采样率)
            let seconds = (n_frames as f64) / (sample_rate as f64);
            return Duration::from_secs_f64(seconds);
        }
    }
    
    Duration::from_secs(0)
}