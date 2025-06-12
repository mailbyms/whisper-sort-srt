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

#[derive(Debug, Clone)]
struct SubtitleLine {
    text: String,
    start_time: f64,
    end_time: f64,
}

/** 将秒数格式化为 SRT 时间格式 (HH:MM:SS,mmm)
 *  参数：
 *      seconds: f64 - 要格式化的秒数
 *  返回值：
 *      String - 格式化后的时间字符串，格式为 HH:MM:SS,mmm
 */
fn format_time(seconds: f64) -> String {
    let hours = (seconds / 3600.0) as u32;
    let minutes = ((seconds % 3600.0) / 60.0) as u32;
    let seconds_whole = (seconds % 60.0) as u32;
    let milliseconds = ((seconds % 1.0) * 1000.0).round() as u32 / 10 * 10;     // 毫秒部分都被四舍五入到了最接近的10毫秒
    format!("{:02}:{:02}:{:02},{:03}", hours, minutes, seconds_whole, milliseconds)
}

/** 根据标点符号和长度规则将文本分割成字幕行
 *  参数：
 *      words: &[Word] - 包含时间戳的单词列表
 *  返回值：
 *      Vec<SubtitleLine> - 分割后的字幕行列表，每行包含文本内容和时间信息
 */
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
    let mut tokens: Vec<&str> = utils::chinese_tokenize(&text);
    let mut word_tokens: Vec<bool> = vec![false; words.len()];

    // println!("中文分词：");
    // for (_i, token) in tokens.iter().enumerate() {
    //     println!("{}", token);
    // }
    // 为 words 更新对应中文分词信息
    match_segments(&mut tokens, words, &mut word_tokens);

    let punctuation = ['，', ',', '。', '！', '？', '；', '：', '、', '…', '—', '（', '）', '《', '》', '"', '"', '\'', '\'', ' '];

    for (i, word) in words.iter().enumerate() {
        // println!("word:{}, is_token:{}", word.word, word_tokens[i]);

        let word_len = word.word.chars().count();
        let current_duration = word.end - current_start;
        
        // 检查当前词是否是英文单词或数字，保持英文单词和数字的完整性。
        // 由于中文分词器可以保证英文单词不会被切割（但不保证数字）。这里只需要判断是否为数字，小数字点和负号
        //let is_english_or_number = word.word.chars().all(|c| c.is_ascii_alphanumeric() || punctuation.contains(&c));
        let is_number = word.word.chars().all(|c| c.is_ascii_digit() || c=='.' || c=='-');
                 
        // 添加当前词
        current_line.push_str(&word.word);
        char_count += word_len;
        word_index = i;

        // 如果遇到标点符号，且当前行长度大于10，立即换行
        // 16个字符，或者时长超过10秒，立即换行（当前word不能是数字，当前word符合中文分词）
        if (word.word.chars().any(|c| punctuation.contains(&c)) && char_count >= LINE_MIN_WORD_LENGTH)
        || ((char_count >= LINE_MAX_WORD_LENGTH || current_duration > LINE_MAX_DURATION) && !is_number && word_tokens[i]) {
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

            continue;
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

/** 将中文分词结果与语音切片进行匹配
 *  参数：
 *      token_segments: &mut Vec<&str> - 中文分词结果
 *      word_segments: &[Word] - 语音切片
 *      word_tokens: &mut [bool] - 用于标记匹配结果的布尔数组
 *  返回值：
 *      无
 */
fn match_segments(token_segments: &mut Vec<&str>, word_segments: &[Word], word_tokens: &mut [bool]) {
    let mut _v_idx = 0;  // 记录语音切片的元素下标
    let mut w_idx = 0;  // 记录中文分词的元素下标
    let mut word_iter = word_segments.iter();

    while !token_segments.is_empty() && w_idx < word_segments.len() {
        let mut v_acc = String::new();
        let mut w_acc = String::new();
        
        // 获取第一个元素
        if let Some(v) = token_segments.first() {
            v_acc.push_str(v);
            token_segments.remove(0);
            _v_idx += 1;
        }
        
        if let Some(w) = word_iter.next() {
            w_acc.push_str(&w.word);
            w_idx += 1;
        }

        loop {
            let v_len = v_acc.chars().count();
            let w_len = w_acc.chars().count();
            if v_len == w_len {
                // println!("匹配成功：'{}'->'{}' [{}]->[{}]", v_acc, w_acc, _v_idx, w_idx);
                word_tokens[w_idx-1] = true;
                break;
            }else if v_len > w_len {
                if let Some(w) = word_iter.next() {
                    w_acc.push_str(&w.word);
                    w_idx += 1;
                    continue;
                } else {
                    // println!("word_segments is empty!");
                    break;
                }
            }else {
                if let Some(v) = token_segments.first() {
                    v_acc.push_str(v);
                    token_segments.remove(0);
                    _v_idx += 1;
                }else {
                    // println!("token_segments is empty!");
                    break;
                }
            }
        }
    }
}

/** 合并相邻的字幕行
 *  合并规则：
 *      1. 仅合并持续时间小于1秒的字幕
 *      2. 相邻字幕间隔大于1秒时不合并
 *      3. 合并时根据长度决定是否换行
 *      4. 每个字幕块最多2行内容
 *  参数：
 *      blocks: Vec<SubtitleLine> - 要合并的字幕块行列表
 *  返回值：
 *      Vec<String> - 合并后的字幕字符串列表
 */
fn merge_subtitles(blocks: Vec<SubtitleLine>) -> Vec<SubtitleLine> {
    let mut merged_blocks: Vec<SubtitleLine> = Vec::new();
    let mut i = 0;
    
    // 标识循环是否需要进行 merge 操作。当前字幕太短，而上一个字幕太长时，会传到下个循环
    let mut prev_need_merge = false; 
    while i < blocks.len() {
        let current = &blocks[i];
        let duration = current.end_time - current.start_time;
        let current_need_merge = duration < 1.0;
        
        // 检查是否可以与上一个块合并
        if let Some(prev) = merged_blocks.last_mut() {
            let gap = current.start_time - prev.end_time;            
            
            // 检查是否执行合并操作
            if (prev_need_merge || current_need_merge) && gap < 1.0 {
                let prev_lines: Vec<&str> = prev.text.lines().collect();
                
                // 检查行数限制
                if prev_lines.len() < 2 {
                    let mut combined_text = prev.text.clone();
                    if !prev.text.eq_ignore_ascii_case(&current.text){
                        combined_text = if prev.text.chars().count() + current.text.chars().count() <= LINE_MAX_WORD_LENGTH {
                            format!("{}{}", prev.text, current.text)
                        } else {
                            format!("{}\n{}", prev.text, current.text)
                        };
                    }                     
                    
                    // 更新上一个块
                    prev.text = combined_text;
                    prev.end_time = current.end_time;
                    //println!("合并：{} -> {}", format_time(prev.start_time), format_time(current.start_time));

                    prev_need_merge = false;
                    i += 1;
                    continue;
                }
                
            }
        }
        
        // 如果不能合并，添加为新块
        merged_blocks.push(SubtitleLine {
            text: current.text.clone(),
            start_time: current.start_time,
            end_time: current.end_time,
        });                
        
        i += 1;
        prev_need_merge = current_need_merge;
    }
    
    // 转换回字幕格式
    merged_blocks    
}


fn main() -> io::Result<()> {
    let args = Args::parse();
    
    // 检查文件是否存在
    if !args.input.exists() {
        eprintln!("输入文件 {:?} 不存在！", args.input);
        process::exit(1);
    }

    println!("读入原始json文件：{}", args.input.to_string_lossy()); 
    // 读取 JSON 文件
    let file = File::open(&args.input)?;
    let whisper_output: WhisperOutput = serde_json::from_reader(file)?;
    
    // 生成输出文件名
    let output_path = args.input.with_extension("srt");
    
    println!("开始分割过长的字幕块");    
    // 存储所有字幕内容
    let mut all_subtitles = Vec::new();
    
    // 处理所有片段
    for segment in whisper_output.segments.iter() {
        let subtitle_lines: Vec<SubtitleLine> = split_text_by_punctuation(&segment.words);
        all_subtitles.extend(subtitle_lines);
    }
    
    // 合并字幕
    println!("合并时长过短的字幕块");
    let merged_subtitles = merge_subtitles(all_subtitles);
    //let merged_subtitles = all_subtitles;
    
    // 一次性写入文件
    println!("写入文件：{}", output_path.to_string_lossy());
    let mut output_file = File::create(&output_path)?;
    for (j, subtitle) in merged_subtitles.iter().enumerate() {
        write!(output_file, "{}\n{} --> {}\n{}\n\n",
            j + 1,
            format_time(subtitle.start_time),
            format_time(subtitle.end_time),
            subtitle.text
        )?;
    }
    
    println!("字幕文件生成完成！");
    
    Ok(())
}
