#!/bin/bash

# 遍历当前目录下的所有.json文件
for file in json/*.json; do
    if [ -f "$file" ]; then
        echo "整理：${file}"
        # 获取不带扩展名的文件名
        filename=$(basename "$file" .json)
        # 重排字幕
        ./target/release/whisper-sort-srt "$file"        
    fi
done

echo "所有文件整理完毕。"
