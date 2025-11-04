
use std::{fs, io, path::{Path, PathBuf}};
use std::time::Duration;
/// 将字符串截断到最大宽度 (以字符数计)，并在末尾添加 "..." (如果发生截断)。
pub fn truncate_string(s: &str, max_width: usize) -> String {
    // 留出 3 个字符给 "..."
    if max_width < 3 { return String::new(); } 
    // 实际可容纳的字符数
    let max_len_no_ellipsis = max_width.saturating_sub(3);
    
    if s.chars().count() > max_width {
        // 使用 chars().take() 安全地截断 UTF-8 字符
        let truncated: String = s.chars().take(max_len_no_ellipsis).collect();
        format!("{}...", truncated)
    } else {
        s.to_string()
    }
}

/// 递归/非递归扫描指定路径，返回支持的音频文件列表。
pub fn scan_audio_files(input_path: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    
    // 如果是单个文件，直接添加
    if input_path.is_file() {
        // 在此处也可以添加扩展名检查，但为简化逻辑，假设用户直接指定的文件是音频文件
        files.push(input_path.to_path_buf());
        return Ok(files);
    }
    
    // 如果是目录，遍历并筛选文件
    if input_path.is_dir() {
        for entry in fs::read_dir(input_path)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    let ext = ext.to_lowercase();
                    // 核心筛选逻辑：仅添加支持的音频格式
                    if ext == "mp3" || ext == "ogg" || ext == "flac" || ext == "aac" || ext == "m4a" || ext == "wav" { 
                        files.push(path);
                    }
                }
            }
        }
    }

    Ok(files)
}
/// 从 .txt 文件中读取播放列表路径，每行一个路径。
pub fn read_playlist_file(path: &Path) -> io::Result<Vec<PathBuf>> {
    // 尝试将整个文件内容读取为字符串
    let content = fs::read_to_string(path)?;
    
    let files: Vec<PathBuf> = content
        .lines()              // 按行迭代
        .map(|line| line.trim()) // 移除每行首尾空白
        .filter(|line| !line.is_empty()) // 忽略空行
        .map(|line| PathBuf::from(line)) // 将字符串转换为 PathBuf
        .collect();
    
    if files.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "播放列表文件为空或不包含有效路径。"));
    }
    
    Ok(files)
}

/// 将 Duration 格式化为 "MM:SS" 字符串。
pub fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs > 0 {
        format!("{:02}:{:02}", secs / 60, secs % 60)
    } else {
        "??:??".to_string()
    }
}