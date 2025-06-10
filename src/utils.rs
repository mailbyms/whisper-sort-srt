// src/utils.rs
use jieba_rs::Jieba;
use lazy_static::lazy_static;

// 初始化分词器
lazy_static! {
    static ref JIEBA: Jieba = Jieba::new();
}

/// 对中文句子进行分词
/// 
/// # Examples
/// 
/// let result = sort_srt::utils::chinese_tokenize("你好，世界！");
/// assert_eq!(result, vec!["你好", "，", "世界", "！"]);
/// 
/// 
/// # 参数说明
/// * `text` - 需要分词的中文文本
/// 
/// # 返回值
/// 分词后的字符串向量
pub fn chinese_tokenize(text: &str) -> Vec<&str> {
    JIEBA.cut(text, false)
}