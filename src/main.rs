mod utils;

use clap::Parser;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process;

// 每行超过16个中文字：应该截断分行
const LINE_MAX_WORD_LENGTH: usize = 16;
// 每行超过10个中文字：可以截断分行
const LINE_MIN_WORD_LENGTH: usize = 10;
// 每行时长超过10秒：应该截断分行
const LINE_MAX_DURATION:  f64 = 10.0;

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// JSON 文件路径
    input: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct Word {
    start: f64,
    end: f64,
    word: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Segment {
    id: i32,
    start: f64,
    end: f64,
    text: String,
    words: Vec<Word>,
}

#[derive(Debug, Serialize, Deserialize)]
struct WhisperOutput {
    text: String,
    segments: Vec<Segment>,
}

#[derive(Debug)]
struct SubtitleLine {
    text: String,
    start_time: f64,
    end_time: f64,
}

fn format_time(seconds: f64) -> String {
    let hours = (seconds / 3600.0) as u32;
    let minutes = ((seconds % 3600.0) / 60.0) as u32;
    let seconds_whole = (seconds % 60.0) as u32;
    let milliseconds = ((seconds % 1.0) * 1000.0).round() as u32 / 10 * 10;     // 毫秒部分都被四舍五入到了最接近的10毫秒
    format!("{:02}:{:02}:{:02},{:03}", hours, minutes, seconds_whole, milliseconds)
}

fn split_text_by_punctuation(words: &[Word]) -> Vec<SubtitleLine> {
    // 如果 words 的长度不超过最优长度，直接返回
    if words.len() <= LINE_MAX_WORD_LENGTH {
        return vec![SubtitleLine {
            text: words.iter().map(|w| w.word.clone()).collect::<String>(),
            start_time: words[0].start,
            end_time: words[words.len()-1].end,
        }];
    }

    let mut result = Vec::new();
    let mut current_line = String::new();
    let mut current_start = words[0].start;
    let mut char_count = 0;
    let mut word_index = 0;

    // 识别并打印出中文分词
    let text = words.iter().map(|w| w.word.clone()).collect::<String>();
    let tokens = utils::chinese_tokenize(&text);
    //println!("打印中文分词");
    for (_i, token) in tokens.iter().enumerate() {
      //  println!("{}", token);
    }

    let punctuation = ['，', ',', '。', '！', '？', '；', '：', '、', '…', '—', '（', '）', '《', '》', '"', '"', '\'', '\'', ' '];

    for (i, word) in words.iter().enumerate() {
        //println!("{}", word.word);
        let word_len = word.word.chars().count();
        let current_duration = word.end - current_start;
        
        // 检查当前词是否是英文单词或数字或标点符号
        let is_english_or_number = word.word.chars().all(|c| c.is_ascii_alphanumeric() || punctuation.contains(&c));
        
        // 如果当前行加上新词超过16个字符，或者时长超过10秒，则强制换行
        if (char_count + word_len > LINE_MAX_WORD_LENGTH || current_duration > LINE_MAX_DURATION) && !is_english_or_number {
            if !current_line.is_empty() {
                result.push(SubtitleLine {
                    text: current_line.trim().to_string(),
                    start_time: current_start,
                    end_time: words[i-1].end,
                });

                current_line.clear();
                current_start = word.start;
                char_count = 0;
            }
        }

        // 添加当前词
        current_line.push_str(&word.word);
        char_count += word_len;
        word_index = i;

        // 如果遇到标点符号，且当前行长度大于10，立即换行
        if word.word.chars().any(|c| punctuation.contains(&c)) && char_count >= LINE_MIN_WORD_LENGTH {
            result.push(SubtitleLine {
                text: current_line.trim().to_string(),
                start_time: current_start,
                end_time: word.end,
            });

            current_line.clear();
            if i + 1 < words.len() {
                current_start = words[i + 1].start;
            }
            char_count = 0;
        }
    }

    // 处理最后一行
    if !current_line.is_empty() {
        // 如果最后一行长度小于5个字符，尝试与上一行合并
        if char_count <= LINE_MIN_WORD_LENGTH/2 && !result.is_empty() {
            let last_line = result.pop().unwrap();
            let combined_text = format!("{}{}", last_line.text, current_line.trim());
            result.push(SubtitleLine {
                text: combined_text,
                start_time: last_line.start_time,
                end_time: words[word_index].end,
            });
        } else {
            result.push(SubtitleLine {
                text: current_line.trim().to_string(),
                start_time: current_start,
                end_time: words[word_index].end,
            });
        }
    }

    result
}

fn main() -> io::Result<()> {
    let args = Args::parse();
    
    // 检查文件是否存在
    if !args.input.exists() {
        eprintln!("输入文件 {:?} 不存在！", args.input);
        process::exit(1);
    }

    // 读取 JSON 文件
    let file = File::open(&args.input)?;
    let whisper_output: WhisperOutput = serde_json::from_reader(file)?;
    
    // 生成输出文件名
    let output_path = args.input.with_extension("srt");
    let output_path_str = output_path.to_string_lossy();
    let mut output_file = File::create(&output_path)?;
    
    println!("开始生成字幕文件：{}", output_path_str);
    println!("----------------------------------------");
    
    // 写入 SRT 格式
    let mut subtitle_index = 1;
    for segment in whisper_output.segments.iter() {
        let subtitle_lines = split_text_by_punctuation(&segment.words);
        
        for line in subtitle_lines {
            let output = format!(
                "{}\n{} --> {}\n{}\n\n",
                subtitle_index,
                format_time(line.start_time),
                format_time(line.end_time),
                line.text
            );
            
            // 写入文件
            write!(output_file, "{}", output)?;
            
            // 输出到控制台
            print!("{}", output);
            
            subtitle_index += 1;
        }
    }
    
    println!("----------------------------------------");
    println!("字幕文件生成完成！");
    
    Ok(())
}
