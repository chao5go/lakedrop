# LakeDrop

LakeDrop 是一款本地优先的桌面数据探查工具，用于快速打开大文件并通过 SQL 即时查询。

<div align="center">

[![Stars](https://img.shields.io/github/stars/chao5go/lakedrop?style=flat-square&labelColor=343b41)](https://github.com/chao5go/lakedrop/stargazers)
[![Forks](https://img.shields.io/github/forks/chao5go/lakedrop?style=flat-square&labelColor=343b41)](https://github.com/chao5go/lakedrop/network/members)
[![License](https://img.shields.io/github/license/chao5go/lakedrop?style=flat-square&labelColor=343b41)](https://github.com/chao5go/lakedrop/blob/main/LICENSE)

</div>

[更新日志](CHANGELOG.zh.md)

## 功能特性
- 拖拽或打开文件
- SQL 编辑器（Cmd/Ctrl+Enter 执行）
- 虚拟化结果表格，支持排序和列宽调整
- Schema 预览、文件元信息与行数统计
- 查询结果导出 CSV/XLSX
- i18n（EN/中文）与明暗主题
- 内置多格式示例数据

## 支持格式
- Parquet：`.parquet`, `.parq`
- CSV/TSV：`.csv`, `.tsv`, `.txt`（支持 `.gz`）
- JSON Lines：`.jsonl`, `.ndjson`（支持 `.gz`）
- JSON（数组）：`.json`
- Arrow/IPC：`.arrow`, `.feather`, `.ipc`
- Excel：`.xlsx`, `.xls`（默认读取第一个工作表，可切换）

## 快速开始

安装依赖：
```bash
pnpm install
```

运行桌面应用：
```bash
pnpm tauri dev
```

仅运行前端：
```bash
pnpm dev
```

## 示例数据

内置样例文件路径：
- `public/samples/`
- `src-tauri/resources/samples/`

重新生成样例：
```bash
cargo run --bin build_samples --manifest-path src-tauri/Cargo.toml
```

## SQL 示例
```sql
SELECT * FROM source LIMIT 10;

SELECT COUNT(*) AS total FROM source;

SELECT group, COUNT(*) AS cnt, AVG(score) AS avg_score
FROM source
GROUP BY group
ORDER BY cnt DESC;
```

## 目录结构
- `src/`：React UI、i18n 与样式
- `src-tauri/`：Rust 后端、Tauri 配置、示例生成器

## 说明
- JSON 文件支持 JSON Lines（`.jsonl/.ndjson`）或 JSON 数组（`.json`）。
- 文本类格式支持 Gzip。

## License
MIT 或 Apache-2.0（发布前择一）。
